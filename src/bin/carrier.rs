//! carrier: note-bound NFT PoC on Zcash testnet (Ironwood pool).
//!
//! Roles are separate ZIP-32 accounts derived from one local test seed:
//!   0 funding wallet, 1 issuer (sequence), 2 vault for item #1, 3 buyer vault, 4 vault for item #2.
use anyhow::{anyhow, bail, Context, Result};
use carrier::*;
use orchard::{
    keys::{FullViewingKey, Scope, SpendAuthorizingKey},
    Note,
};
use rand::{rngs::OsRng, RngCore};
use sapling::prover::mock::{MockOutputProver, MockSpendProver};
use std::{fs, path::PathBuf};
use zcash_primitives::transaction::{
    builder::{BuildConfig, BundlePadding, Builder},
    fees::zip317::FeeRule,
};
use zcash_protocol::{
    consensus::{BlockHeight, TEST_NETWORK},
    memo::MemoBytes,
    value::Zatoshis,
};
use zcash_transparent::builder::TransparentSigningSet;

const FEE_PER_ACTION: u64 = 5_000;
const MAX_TRIES: usize = 64;

fn dir() -> PathBuf {
    PathBuf::from(std::env::var("STATE").unwrap_or_else(|_| "state".into()))
}

fn seed() -> Result<[u8; 32]> {
    let p = dir().join("seed.hex");
    if !p.exists() {
        fs::create_dir_all(dir())?;
        let mut s = [0u8; 32];
        OsRng.fill_bytes(&mut s);
        fs::write(&p, hex::encode(s))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&p, fs::Permissions::from_mode(0o600))?;
        }
    }
    Ok(hex::decode(fs::read_to_string(p)?.trim())?.try_into().map_err(|_| anyhow!("seed"))?)
}

struct Acct {
    sak: SpendAuthorizingKey,
    fvk: FullViewingKey,
}
fn acct(n: u32) -> Result<Acct> {
    // OFF shifts every role except the funding wallet (acct 0): fresh collection, no key reuse.
    let off: u32 = std::env::var("OFF").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let sk = spending_key(&seed()?, if n == 0 { 0 } else { n + off });
    Ok(Acct { sak: SpendAuthorizingKey::from(&sk), fvk: FullViewingKey::from(&sk) })
}
fn addr(a: &Acct) -> orchard::Address {
    a.fvk.address_at(0u32, Scope::External)
}

fn save<T: serde::Serialize>(name: &str, v: &T) -> Result<()> {
    fs::write(dir().join(name), serde_json::to_string_pretty(v)?)?;
    Ok(())
}
fn load<T: serde::de::DeserializeOwned>(name: &str) -> Result<T> {
    Ok(serde_json::from_str(&fs::read_to_string(dir().join(name)).with_context(|| name.to_string())?)?)
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct Held {
    note: NoteJson,
    cmx: String,
    height: u64,
    txid: String,
}

/// Reads a mined tx, finds the output decryptable by `fvk` whose memo kind matches (or any if None).
async fn find_output(c: &mut Client, txid: &str, fvk: &FullViewingKey, kind: Option<u8>) -> Result<(Found, u64)> {
    let (tx, h) = get_tx(c, txid).await?;
    let h = h.ok_or_else(|| anyhow!("tx {txid} not mined yet"))?;
    for f in decrypt_ivk(&tx, fvk) {
        let k = decode_memo(&f.memo).map(|m| m.kind);
        if kind.is_none() || k == kind {
            return Ok((f, h));
        }
    }
    bail!("no matching output for this key in {txid}")
}

struct Spend<'a> {
    fvk: &'a FullViewingKey,
    note: Note,
    cmx: [u8; 32],
    height: u64,
    /// If Some, the output with this index must share the spend's action (successor binding).
    bind_output: Option<usize>,
    /// If Some, the output with this index must NOT share the spend's action (forgery test).
    avoid_output: Option<usize>,
}

struct Out {
    to: orchard::Address,
    value: u64,
    memo: [u8; 512],
    /// Outgoing viewing key. Issuance outputs use the issuer ovk so auditors holding the
    /// published issuer fvk can recover every carrier minted in the sequence chain.
    ovk: Option<orchard::keys::OutgoingViewingKey>,
}

fn ovk(a: &Acct) -> Option<orchard::keys::OutgoingViewingKey> {
    Some(a.fvk.to_ovk(Scope::External))
}

/// Builds, proves and signs an Ironwood transaction. Retries the (proof-bearing) build until every
/// requested spend->output binding lands in the same action, so the successor rho = nf_old.
async fn build_and_send(c: &mut Client, spends: Vec<Spend<'_>>, outs: Vec<Out>, saks: Vec<SpendAuthorizingKey>, dry: bool) -> Result<String> {
    let tip_h = tip(c).await?;
    let anchor_h = tip_h;
    let mut paths = vec![];
    let mut anchor = None;
    for s in &spends {
        // Zero-value spends skip the in-circuit membership check, but we witness every spend
        // against the real tree anyway (and cross-check the root with the server treestate).
        let (p, a) = witness(c, &s.cmx, s.height, anchor_h).await?;
        anchor = Some(a);
        paths.push(p);
    }
    let anchor = match anchor {
        Some(a) => a,
        None => orchard::Anchor::from(ironwood_tree(c, anchor_h).await?.root()),
    };
    let target = BlockHeight::from_u32((tip_h + 1) as u32);
    for attempt in 1..=MAX_TRIES {
        let mut b = Builder::new(
            TEST_NETWORK,
            target,
            BuildConfig::Standard {
                sapling_anchor: None,
                orchard_anchor: None,
                ironwood_anchor: Some(anchor),
                orchard_padding: BundlePadding::DEFAULT,
                ironwood_padding: BundlePadding::DEFAULT,
            },
        );
        for (s, p) in spends.iter().zip(&paths) {
            b.add_ironwood_spend::<std::convert::Infallible>(s.fvk.clone(), s.note, p.clone())
                .map_err(|e| anyhow!("spend: {e}"))?;
        }
        for o in &outs {
            let memo = MemoBytes::from_bytes(&o.memo).map_err(|e| anyhow!("memo {e:?}"))?;
            b.add_ironwood_output::<std::convert::Infallible>(o.ovk.clone(), o.to, Zatoshis::from_u64(o.value).unwrap(), memo)
                .map_err(|e| anyhow!("output: {e}"))?;
        }
        let res = b
            .build(
                &TransparentSigningSet::new(),
                &[],
                &saks,
                OsRng,
                &MockSpendProver,
                &MockOutputProver,
                &FeeRule::standard(),
            )
            .map_err(|e| anyhow!("build: {e}"))?;
        let meta = res.ironwood_meta();
        let ok = spends.iter().enumerate().all(|(i, s)| match s.bind_output {
            None => true,
            Some(o) => meta.spend_action_index(i) == meta.output_action_index(o),
        }) && spends.iter().enumerate().all(|(i, s)| match s.avoid_output {
            None => true,
            Some(o) => meta.spend_action_index(i) != meta.output_action_index(o),
        });
        let tx = res.transaction();
        let txid = tx.txid().to_string();
        if !ok {
            println!("  attempt {attempt}: binding not satisfied, rebuilding");
            continue;
        }
        // post-check on the actual bytes: bound output rho == spent note nullifier
        for (i, s) in spends.iter().enumerate() {
            if let Some(o) = s.bind_output {
                let ai = meta.spend_action_index(i).unwrap();
                let act = &tx.ironwood_bundle().unwrap().actions()[ai];
                assert_eq!(act.nullifier().to_bytes(), s.note.nullifier(s.fvk).to_bytes());
                assert_eq!(meta.output_action_index(o), Some(ai));
            }
        }
        let mut raw = vec![];
        tx.write(&mut raw)?;
        println!("  attempt {attempt}: bound ok, {} actions, {} bytes, txid {txid}", tx.ironwood_bundle().map(|b| b.actions().len()).unwrap_or(0), raw.len());
        if dry {
            return Ok(txid);
        }
        let r = c
            .send_transaction(zcash_client_backend::proto::service::RawTransaction { data: raw, height: 0 })
            .await?
            .into_inner();
        if r.error_code != 0 {
            bail!("rejected by node: code {} {}", r.error_code, r.error_message);
        }
        return Ok(txid);
    }
    bail!("binding not satisfied after {MAX_TRIES} tries")
}

fn fee(actions: usize) -> u64 {
    FEE_PER_ACTION * actions.max(2) as u64
}

async fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("help");
    let dry = std::env::var("DRY").is_ok();
    let mut c = connect().await?;
    match cmd {
        "info" => {
            let i = c.get_lightd_info(zcash_client_backend::proto::service::Empty {}).await?.into_inner();
            println!("chain={} height={} branch={} upgrade={}@{} vendor={} ver={}", i.chain_name, i.block_height, i.consensus_branch_id, i.upgrade_name, i.upgrade_height, i.vendor, i.version);
            for n in 0..5 {
                println!("acct{n} {}", ua(&addr(&acct(n)?)));
            }
        }
        // Scan blocks for funding note of account 0 (compact decryption).
        "scan" => {
            let from: u64 = args[2].parse()?;
            let to = tip(&mut c).await?;
            let a = acct(0)?;
            let pivk = orchard::keys::PreparedIncomingViewingKey::new(&a.fvk.to_ivk(Scope::External));
            for b in blocks(&mut c, from, to).await? {
                for tx in &b.vtx {
                    for ca in &tx.ironwood_actions {
                        let act = orchard::note_encryption::CompactAction::try_from(ca).map_err(|_| anyhow!("ca"))?;
                        let d = orchard::note_encryption::IronwoodDomain::for_compact_action(&act);
                        if let Some((n, _)) = zcash_note_encryption::try_compact_note_decryption(&d, &pivk, &act) {
                            let txid = txid_hex(&tx.txid);
                            println!("found {} zats at h={} tx={txid}", n.value().inner(), b.height);
                            let (f, h) = find_output(&mut c, &txid, &a.fvk, None).await?;
                            save("funding.json", &Held { note: note_to_json(&f.note), cmx: hex::encode(f.cmx), height: h, txid })?;
                        }
                    }
                }
            }
        }
        // tx1: funding -> S0 (sequence, issuer) + C1 (item #1 carrier in vault 2) + change
        "genesis" => {
            let (a0, a1, a2) = (acct(0)?, acct(1)?, acct(2)?);
            let (fnote, fcmx, fh) = match std::env::var("FUND_TX") {
                Ok(t) => {
                    let (f, h) = find_change(&mut c, &t, &a0.fvk).await?;
                    (f.note, f.cmx, h)
                }
                Err(_) => {
                    let fund: Held = load("funding.json")?;
                    (note_from_json(&fund.note)?, arr32(&fund.cmx)?, fund.height)
                }
            };
            let mut coll = [0u8; 32];
            coll.copy_from_slice(&a1.fvk.to_bytes()[..32]); // collection id = issuer ak prefix
            let art1 = fs::read("art/art1.bin")?;
            let outs = vec![
                Out { to: addr(&a1), value: 0, memo: encode_memo(K_SEQ, &coll, 0, 0, &[]), ovk: ovk(&a1) },
                Out { to: addr(&a2), value: 0, memo: encode_memo(K_CARRIER, &coll, 1, 0, &art1), ovk: ovk(&a1) },
                Out { to: addr(&a0), value: fnote.value().inner() - fee(3), memo: empty_memo(), ovk: ovk(&a0) },
            ];
            let txid = build_and_send(&mut c, vec![Spend { fvk: &a0.fvk, note: fnote, cmx: fcmx, height: fh, bind_output: None, avoid_output: None }], outs, vec![a0.sak], dry).await?;
            save("collection.json", &Collection { network: "test".into(), coll: hex::encode(coll), issuer_fvk: fvk_hex(&a1.fvk), genesis_txid: txid.clone() })?;
            println!("GENESIS {txid}");
        }
        // tx2: S(prev) -> S(next, FINAL) bound in the same action, + C2 (item #2); fee from change.
        "final" => {
            let col: Collection = load("collection.json")?;
            let (a0, a1, a4) = (acct(0)?, acct(1)?, acct(4)?);
            let (s0, sh) = find_output(&mut c, &col.genesis_txid, &a1.fvk, Some(K_SEQ)).await?;
            let (ch, chh) = find_change(&mut c, &col.genesis_txid, &a0.fvk).await?;
            let coll = arr32(&col.coll)?;
            let art2 = fs::read("art/art2.bin")?;
            let outs = vec![
                Out { to: addr(&a1), value: 0, memo: encode_memo(K_FINAL, &coll, 2, 1, &[]), ovk: ovk(&a1) },
                Out { to: addr(&a4), value: 0, memo: encode_memo(K_CARRIER, &coll, 2, 0, &art2), ovk: ovk(&a1) },
                Out { to: addr(&a0), value: ch.note.value().inner() - fee(3), memo: empty_memo(), ovk: ovk(&a0) },
            ];
            let spends = vec![
                Spend { fvk: &a1.fvk, note: s0.note, cmx: s0.cmx, height: sh, bind_output: Some(0), avoid_output: None },
                Spend { fvk: &a0.fvk, note: ch.note, cmx: ch.cmx, height: chh, bind_output: None, avoid_output: None },
            ];
            let txid = build_and_send(&mut c, spends, outs, vec![a1.sak, a0.sak], dry).await?;
            save("final.json", &serde_json::json!({ "txid": txid }))?;
            println!("FINAL {txid}");
        }
        // tx3: C1 (vault 2) -> buyer vault 3, bound in the same action; fee from change.
        "transfer" => {
            let col: Collection = load("collection.json")?;
            let fin: serde_json::Value = load("final.json")?;
            let (a0, a2, a3) = (acct(0)?, acct(2)?, acct(3)?);
            let (c1, c1h) = find_output(&mut c, &col.genesis_txid, &a2.fvk, Some(K_CARRIER)).await?;
            let (ch, chh) = find_change(&mut c, fin["txid"].as_str().unwrap(), &a0.fvk).await?;
            let coll = arr32(&col.coll)?;
            let outs = vec![
                Out { to: addr(&a3), value: 0, memo: encode_memo(K_HOP, &coll, 1, 1, &[]), ovk: ovk(&a2) },
                Out { to: addr(&a0), value: ch.note.value().inner() - fee(2), memo: empty_memo(), ovk: ovk(&a0) },
            ];
            let spends = vec![
                Spend { fvk: &a2.fvk, note: c1.note, cmx: c1.cmx, height: c1h, bind_output: Some(0), avoid_output: None },
                Spend { fvk: &a0.fvk, note: ch.note, cmx: ch.cmx, height: chh, bind_output: None, avoid_output: None },
            ];
            let txid = build_and_send(&mut c, spends, outs, vec![a2.sak, a0.sak], dry).await?;
            // Seller hands the buyer the provenance package: previous note + seller vault fvk.
            let pkg = Package {
                item: 1,
                label: "genuine".into(),
                hops: vec![Hop { note: note_to_json(&c1.note), vault_fvk: fvk_hex(&a2.fvk) }],
            };
            save("package_item1.json", &pkg)?;
            save("transfer.json", &serde_json::json!({ "txid": txid }))?;
            println!("TRANSFER {txid}");
        }
        // Attack A: spend C1 again (already spent in tx3) -> node must reject (duplicate nullifier).
        "attack-double" => {
            let col: Collection = load("collection.json")?;
            // fee note = fresh change from the forge tx, so C1's nullifier is the only conflict
            let tr: serde_json::Value = load("forge.json")?;
            let (a0, a2, a3) = (acct(0)?, acct(2)?, acct(3)?);
            let (c1, c1h) = find_output(&mut c, &col.genesis_txid, &a2.fvk, Some(K_CARRIER)).await?;
            let (ch, chh) = find_change(&mut c, tr["txid"].as_str().unwrap(), &a0.fvk).await?;
            let coll = arr32(&col.coll)?;
            let outs = vec![
                Out { to: addr(&a3), value: 0, memo: encode_memo(K_HOP, &coll, 1, 1, &[]), ovk: ovk(&a2) },
                Out { to: addr(&a0), value: ch.note.value().inner() - fee(2), memo: empty_memo(), ovk: ovk(&a0) },
            ];
            let spends = vec![
                Spend { fvk: &a2.fvk, note: c1.note, cmx: c1.cmx, height: c1h, bind_output: Some(0), avoid_output: None },
                Spend { fvk: &a0.fvk, note: ch.note, cmx: ch.cmx, height: chh, bind_output: None, avoid_output: None },
            ];
            match build_and_send(&mut c, spends, outs, vec![a2.sak, a0.sak], false).await {
                Ok(t) => println!("ATTACK-DOUBLE UNEXPECTEDLY ACCEPTED {t}"),
                Err(e) => println!("ATTACK-DOUBLE REJECTED: {e}"),
            }
        }
        // Attack B: item #2 holder (vault 4) spends its carrier but puts the "successor" for a
        // victim buyer (vault 3) in a DIFFERENT action, so the buyer note is not the successor.
        "attack-forge" => {
            let col: Collection = load("collection.json")?;
            let fin: serde_json::Value = load("final.json")?;
            let tr: serde_json::Value = load("transfer.json")?;
            let (a0, a3, a4) = (acct(0)?, acct(3)?, acct(4)?);
            let (c2, c2h) = find_output(&mut c, fin["txid"].as_str().unwrap(), &a4.fvk, Some(K_CARRIER)).await?;
            let (ch, chh) = find_change(&mut c, tr["txid"].as_str().unwrap(), &a0.fvk).await?;
            let coll = arr32(&col.coll)?;
            let outs = vec![
                Out { to: addr(&a3), value: 0, memo: encode_memo(K_HOP, &coll, 2, 1, &[]), ovk: ovk(&a4) },
                Out { to: addr(&a0), value: ch.note.value().inner() - fee(2), memo: empty_memo(), ovk: ovk(&a0) },
            ];
            let spends = vec![
                Spend { fvk: &a4.fvk, note: c2.note, cmx: c2.cmx, height: c2h, bind_output: None, avoid_output: Some(0) },
                Spend { fvk: &a0.fvk, note: ch.note, cmx: ch.cmx, height: chh, bind_output: None, avoid_output: None },
            ];
            let txid = build_and_send(&mut c, spends, outs, vec![a4.sak, a0.sak], dry).await?;
            let pkg = Package { item: 2, label: "forged".into(), hops: vec![Hop { note: note_to_json(&c2.note), vault_fvk: fvk_hex(&a4.fvk) }] };
            save("package_item2_forged.json", &pkg)?;
            save("forge.json", &serde_json::json!({ "txid": txid }))?;
            println!("ATTACK-FORGE {txid}");
        }
        // Attack C: issuer mints "#3" after FINAL using an unrelated funding change note.
        "attack-mint" => {
            let col: Collection = load("collection.json")?;
            let tr: serde_json::Value = load("transfer.json")?;
            let (a0, a1) = (acct(0)?, acct(1)?);
            let _ = tr;
            let forge: serde_json::Value = load("forge.json")?;
            let (ch, chh) = find_change(&mut c, forge["txid"].as_str().unwrap(), &a0.fvk).await?;
            let coll = arr32(&col.coll)?;
            let art3 = fs::read("art/art3.bin")?;
            let outs = vec![
                Out { to: addr(&a1), value: 0, memo: encode_memo(K_CARRIER, &coll, 3, 0, &art3), ovk: ovk(&a1) },
                Out { to: addr(&a0), value: ch.note.value().inner() - fee(2), memo: empty_memo(), ovk: ovk(&a0) },
            ];
            let spends = vec![Spend { fvk: &a0.fvk, note: ch.note, cmx: ch.cmx, height: chh, bind_output: None, avoid_output: None }];
            let txid = build_and_send(&mut c, spends, outs, vec![a0.sak], dry).await?;
            save("attack_mint.json", &serde_json::json!({ "txid": txid }))?;
            println!("ATTACK-MINT {txid}");
        }
        "keys" => {
            // Public: issuer fvk (collection audit). Private to holders: vault fvks in packages.
            for n in 0..5 {
                println!("acct{n} fvk {}", fvk_hex(&acct(n)?.fvk));
            }
        }
        _ => println!("commands: info | scan <from_height> | genesis | final | transfer | attack-double | attack-forge | attack-mint | keys"),
    }
    Ok(())
}

fn arr32(s: &str) -> Result<[u8; 32]> {
    hex::decode(s)?.try_into().map_err(|_| anyhow!("len"))
}

fn empty_memo() -> [u8; 512] {
    let mut m = [0u8; 512];
    m[0] = 0xF6;
    m
}

async fn find_change(c: &mut Client, txid: &str, fvk: &FullViewingKey) -> Result<(Found, u64)> {
    let (tx, h) = get_tx(c, txid).await?;
    let h = h.ok_or_else(|| anyhow!("tx {txid} not mined yet"))?;
    decrypt_ivk(&tx, fvk)
        .into_iter()
        .find(|f| f.note.value().inner() > 0)
        .map(|f| (f, h))
        .ok_or_else(|| anyhow!("no change output in {txid}"))
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
