//! carrier: note-bound NFT PoC on Zcash testnet (Ironwood pool).
//!
//! Roles are separate ZIP-32 accounts derived from one local test seed:
//!   0 funding wallet, 1 issuer (sequence), 2 vault for item #1, 3 buyer vault, 4 vault for item #2.
use anyhow::{anyhow, bail, Context, Result};
use carrier::verifier::{cross_check, issuance_set, verify_item, Chain, ChainSource, Verdict};
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
/// ZIP-32 account index of role `n`. OFF shifts every role except the funding wallet (acct 0):
/// fresh collection, no key reuse.
fn acct_index(n: u32) -> u32 {
    let off: u32 = std::env::var("OFF").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    if n == 0 { 0 } else { n + off }
}
fn acct(n: u32) -> Result<Acct> {
    let sk = spending_key(&seed()?, acct_index(n));
    Ok(Acct { sak: SpendAuthorizingKey::from(&sk), fvk: FullViewingKey::from(&sk) })
}
fn addr(a: &Acct) -> orchard::Address {
    a.fvk.address_at(0u32, Scope::External)
}

/// Working files (collection, packages, tx records). Defaults to the seed's STATE dir; FILES_DIR
/// keeps a second collection's files apart while still using the same seed.
fn files_dir() -> PathBuf {
    std::env::var("FILES_DIR").map(PathBuf::from).unwrap_or_else(|_| dir())
}

fn save<T: serde::Serialize>(name: &str, v: &T) -> Result<()> {
    fs::create_dir_all(files_dir())?;
    fs::write(files_dir().join(name), serde_json::to_string_pretty(v)?)?;
    Ok(())
}
fn load<T: serde::de::DeserializeOwned>(name: &str) -> Result<T> {
    Ok(serde_json::from_str(&fs::read_to_string(files_dir().join(name)).with_context(|| name.to_string())?)?)
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
            b.add_ironwood_output::<std::convert::Infallible>(o.ovk.clone(), o.to, Zatoshis::from_u64(o.value).map_err(|_| anyhow!("amount {} out of range", o.value))?, memo)
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
            let from: u64 = args.get(2).ok_or_else(|| anyhow!("usage: carrier scan <from_height>"))?.parse()?;
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
        // SEAL=1 sends FINAL to the collection's seal address, which nobody can spend (OPEN_ISSUES #2).
        "final" => {
            let sealed = std::env::var("SEAL").is_ok();
            let col: Collection = load("collection.json")?;
            let (a0, a1, a4) = (acct(0)?, acct(1)?, acct(4)?);
            let (s0, sh) = find_output(&mut c, &col.genesis_txid, &a1.fvk, Some(K_SEQ)).await?;
            let (ch, chh) = find_change(&mut c, &col.genesis_txid, &a0.fvk).await?;
            let coll = arr32(&col.coll)?;
            let art2 = fs::read("art/art2.bin")?;
            let outs = vec![
                Out { to: if sealed { seal_address(&coll) } else { addr(&a1) }, value: 0, memo: encode_memo(K_FINAL, &coll, 2, 1, &[]), ovk: ovk(&a1) },
                Out { to: addr(&a4), value: 0, memo: encode_memo(K_CARRIER, &coll, 2, 0, &art2), ovk: ovk(&a1) },
                Out { to: addr(&a0), value: ch.note.value().inner().checked_sub(fee(3)).ok_or_else(|| anyhow!("funding too small"))?, memo: empty_memo(), ovk: ovk(&a0) },
            ];
            let spends = vec![
                Spend { fvk: &a1.fvk, note: s0.note, cmx: s0.cmx, height: sh, bind_output: Some(0), avoid_output: None },
                Spend { fvk: &a0.fvk, note: ch.note, cmx: ch.cmx, height: chh, bind_output: None, avoid_output: None },
            ];
            let txid = build_and_send(&mut c, spends, outs, vec![a1.sak, a0.sak], dry).await?;
            save("final.json", &serde_json::json!({ "txid": txid, "sealed": sealed }))?;
            println!("FINAL {txid} sealed={sealed}");
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
        // Two-party atomic sale via PCZT (docs/marketplace/SPEC.md section 3). See sale_* below.
        "sale-list" => sale_list(c).await?,
        "sale-buy" => sale_buy(c).await?,
        "sale-accept" => sale_accept(c, dry).await?,
        "recover" => recover(c).await?,
        // Plain payment from the funding account: send <recipient_fvk_hex> <zats>, funding note in FUND_TX.
        // Used to fund a second, separately held seed (marketplace SPEC P12).
        "send" => {
            let to = fvk_from_hex(args.get(2).ok_or_else(|| anyhow!("usage: carrier send <recipient_fvk_hex> <zats>"))?)?;
            let value: u64 = args.get(3).ok_or_else(|| anyhow!("usage: carrier send <recipient_fvk_hex> <zats>"))?.parse()?;
            let a0 = acct(0)?;
            let fund_tx = std::env::var("FUND_TX").map_err(|_| anyhow!("set FUND_TX to the tx holding the funding note"))?;
            let (f, fh) = find_change(&mut c, &fund_tx, &a0.fvk).await?;
            let change = f.note.value().inner().checked_sub(value.checked_add(fee(2)).ok_or_else(|| anyhow!("amount overflow"))?).ok_or_else(|| anyhow!("funding note too small"))?;
            let outs = vec![
                Out { to: to.address_at(0u32, Scope::External), value, memo: empty_memo(), ovk: ovk(&a0) },
                Out { to: addr(&a0), value: change, memo: empty_memo(), ovk: ovk(&a0) },
            ];
            let spends = vec![Spend { fvk: &a0.fvk, note: f.note, cmx: f.cmx, height: fh, bind_output: None, avoid_output: None }];
            let txid = build_and_send(&mut c, spends, outs, vec![a0.sak], dry).await?;
            println!("SEND {txid} {value} zats, change {change}{}", if dry { " (DRY, not broadcast)" } else { "" });
        }
        "keys" => {
            // Public: issuer fvk (collection audit). Private to holders: vault fvks in packages.
            // acct5 (seller pay), acct6 and acct7 (buyer vaults) are the sale roles.
            for n in 0..8 {
                println!("acct{n} fvk {}", fvk_hex(&acct(n)?.fvk));
            }
        }
        _ => println!("commands: info | scan <from_height> | genesis | final | transfer | attack-double | attack-forge | attack-mint | sale-list | sale-buy | sale-accept | send <fvk> <zats> | recover | keys"),
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

// ---------- two-party atomic sale via PCZT (marketplace SPEC section 3) ----------
// Roles (shifted by OFF like every role except funding), overridable per run:
//   seller = SELLER_ACCT (default 3, the vault holding the item) + PAY_ACCT (default 5, payment key)
//   buyer  = acct0 (funding, note in FUND_TX) + BUYER_VAULT_ACCT (default 6, the buyer's new vault)
// Each subcommand derives only the keys of its own role. Buyer and seller can run with different
// STATE dirs (separate seeds); they exchange the listing and the PCZT through SALE_DIR.

/// Total ZEC supply in zats; any price above it is invalid input.
const MAX_MONEY: u64 = 21_000_000 * 100_000_000;

/// Listing record. In the CLI it is a local file; SPEC draft 3 keeps the vault FVK, note and
/// package out of the public order book and discloses them only to a committed buyer.
#[derive(serde::Serialize, serde::Deserialize)]
struct Listing {
    /// public collection record, needed for R1
    collection: Collection,
    item: u32,
    /// hop number of the carrier being sold; the successor gets hop + 1
    hop: u32,
    price: u64,
    /// raw 43-byte Orchard-protocol address, hex. A fresh diversified address per listing.
    seller_pay_addr: String,
    seller_pay_ua: String,
    /// listing expiry height
    expiry: u64,
    carrier: Held,
    vault_fvk: String,
    /// provenance up to (not including) the carrier being sold
    package: Package,
}

fn sale_dir() -> PathBuf {
    std::env::var("SALE_DIR").map(PathBuf::from).unwrap_or_else(|_| dir())
}

fn sale_pczt_path() -> PathBuf {
    sale_dir().join(std::env::var("SALE_PCZT").unwrap_or_else(|_| "sale.pczt".into()))
}

fn listing_path() -> PathBuf {
    sale_dir().join(std::env::var("LISTING").unwrap_or_else(|_| "listing.json".into()))
}

fn env_u64(k: &str, default: u64) -> Result<u64> {
    std::env::var(k).map(|v| v.parse::<u64>().with_context(|| k.to_string())).unwrap_or(Ok(default))
}

fn env_u32(k: &str, default: u32) -> Result<u32> {
    std::env::var(k).map(|v| v.parse::<u32>().with_context(|| k.to_string())).unwrap_or(Ok(default))
}

fn pay_address(l: &Listing) -> Result<orchard::Address> {
    let raw: [u8; 43] = hex::decode(&l.seller_pay_addr)?.try_into().map_err(|_| anyhow!("pay address length"))?;
    Option::from(orchard::Address::from_raw_address_bytes(&raw)).ok_or_else(|| anyhow!("pay address"))
}

/// Identifies one version of a listing: BLAKE2b-256 of its JSON encoding.
fn listing_id(l: &Listing) -> Result<[u8; 32]> {
    let h = blake2b_simd::Params::new().hash_length(32).personal(b"CarrierListing01").hash(&serde_json::to_vec(l)?);
    Ok(h.as_bytes().try_into()?)
}

/// Runs R1 to R3 for `pkg` with `holder` (from the collection's genesis height) and returns the
/// chain index, the held carrier, its hop number and its Action index. Anything other than HELD
/// is refused.
/// `from` lowers the scan start, so notes older than the genesis (a funding note) are covered too.
async fn verified_carrier(mut c: Client, col: &Collection, pkg: &Package, holder: &FullViewingKey, from: Option<u64>) -> Result<(Chain, Held, u32, usize)> {
    let (_, gh) = get_tx(&mut c, &col.genesis_txid).await?;
    let gh = gh.ok_or_else(|| anyhow!("genesis tx unmined"))?;
    let mut ch = Chain::index(c, from.map_or(gh, |f| f.min(gh))).await?;
    let set = issuance_set(&mut ch, col, &mut |l| println!("{l}")).await?;
    match verify_item(&mut ch, col, &set, pkg, Some(holder), &mut |l| println!("{l}")).await? {
        Verdict::Held { cur, txid, height, hop } => {
            cross_check(&mut ch).await?;
            let action = cur.action;
            Ok((ch, Held { note: note_to_json(&cur.note), cmx: hex::encode(cur.cmx), height, txid }, hop, action))
        }
        Verdict::Spent { .. } => bail!("REFUSED: the carrier is already spent"),
        Verdict::Burned { hop, why } => bail!("REFUSED: item burned at hop {hop} ({why})"),
        Verdict::Unverified { .. } => bail!("REFUSED: holder check did not run"),
    }
}

/// Seller: list the item held by SELLER_ACCT's vault, proven by PACKAGE, for PRICE zats.
async fn sale_list(c: Client) -> Result<()> {
    let col: Collection = load("collection.json")?;
    let pkg: Package = load(&std::env::var("PACKAGE").unwrap_or_else(|_| "package_item1.json".into()))?;
    let (seller, pay_acct) = (acct(env_u32("SELLER_ACCT", 3)?)?, acct(env_u32("PAY_ACCT", 5)?)?);
    // The seller lists only what the verifier says it holds.
    let (ch, held, hop, _) = verified_carrier(c, &col, &pkg, &seller.fvk, None).await?;
    let price = env_u64("PRICE", 10_000)?;
    if price == 0 || price > MAX_MONEY {
        bail!("PRICE must be between 1 and {MAX_MONEY} zats");
    }
    let expiry = ch.to.checked_add(env_u64("LISTING_TTL", 1_000)?).ok_or_else(|| anyhow!("LISTING_TTL overflow"))?;
    // A fresh diversified pay address per listing, so one seller's listings are not linkable.
    let pay = pay_acct.fvk.address_at(OsRng.next_u32(), Scope::External);
    let l = Listing {
        collection: col,
        item: pkg.item,
        hop,
        price,
        seller_pay_addr: hex::encode(pay.to_raw_address_bytes()),
        seller_pay_ua: ua(&pay),
        expiry,
        carrier: held,
        vault_fvk: fvk_hex(&seller.fvk),
        package: pkg,
    };
    fs::write(listing_path(), serde_json::to_string_pretty(&l)?)?;
    println!("LISTED item #{} hop {} price {price} zats expiry {expiry} carrier cmx {}.. in {}", l.item, l.hop, &l.carrier.cmx[..12], listing_path().display());
    Ok(())
}

/// Buyer: verify the listed item from chain (R1 to R3), then build the joint tx as a PCZT with
/// the carrier pair in one Action, prove it, sign only the buyer's funding spend, and redact.
/// Test knobs: TAMPER_PRICE=<zats> pays a different amount; EXPIRED=1 builds an already expired
/// PCZT; PCZT_TTL=<blocks> sets its validity (default 20).
async fn sale_buy(c: Client) -> Result<()> {
    let mut c = c;
    let l: Listing = serde_json::from_slice(&read_capped(&listing_path())?)?;
    let lid = listing_id(&l)?;
    let vault = fvk_from_hex(&l.vault_fvk)?;
    let bv_n = env_u32("BUYER_VAULT_ACCT", 6)?;
    let (a0, bv) = (acct(0)?, acct(bv_n)?);
    let fund_tx = std::env::var("FUND_TX").map_err(|_| anyhow!("set FUND_TX to the tx holding the buyer's funding note"))?;
    let (fund, fh) = find_change(&mut c, &fund_tx, &a0.fvk).await?;
    // Never trust the listing's note: verify the current carrier from chain with the listed key.
    // The scan also starts no later than the funding note, so its liveness check is complete.
    let (mut ch, held, hop, action) = verified_carrier(c, &l.collection, &l.package, &vault, Some(fh)).await?;
    let tip_h = ch.to;
    if tip_h >= l.expiry {
        bail!("listing expired at {} (tip {tip_h})", l.expiry);
    }
    if held.cmx != l.carrier.cmx || hop != l.hop || l.package.item != l.item {
        bail!("REFUSED: the listing does not match the verified current carrier");
    }
    let carrier = note_from_json(&held.note)?;
    let ccmx = arr32(&held.cmx)?;
    println!("  verified from chain: item #{} hop {hop}, carrier {}.. held by the listed vault key, unspent", l.item, &held.cmx[..12]);

    if ch.spent_at(&fund.note.nullifier(&a0.fvk).to_bytes()).is_some() {
        bail!("the funding note in {fund_tx} is already spent");
    }
    let pay = pay_address(&l)?;
    let pay_value = env_u64("TAMPER_PRICE", l.price)?;
    let need = pay_value.checked_add(fee(3)).ok_or_else(|| anyhow!("price overflow"))?;
    let change = fund.note.value().inner().checked_sub(need).ok_or_else(|| anyhow!("funding note too small"))?;
    let coll = arr32(&l.collection.coll)?;
    let next_hop = hop.checked_add(1).ok_or_else(|| anyhow!("hop overflow"))?;
    // SPEC 4.2: the successor's memo names the seller's vault key and where its carrier was made,
    // taken from the chain verification above, not from the listing.
    let record = VaultRecord {
        fvk: vault.to_bytes(),
        kind: if hop == 0 { PTR_ISSUANCE } else { PTR_TRANSFER },
        txid: txid_bytes(&held.txid)?,
        action: u32::try_from(action)?,
    };
    let outs = [
        // 0: successor carrier, must share the carrier spend's Action. The buyer's own OVK, so
        //    the seller cannot recover the buyer's new vault address from chain.
        Out { to: addr(&bv), value: 0, memo: encode_memo(K_HOP, &coll, l.item, next_hop, &record.encode()), ovk: ovk(&bv) },
        // 1: payment to the listing's pay address
        Out { to: pay, value: pay_value, memo: empty_memo(), ovk: ovk(&a0) },
        // 2: buyer change, present even at zero value; the buyer pays the fee. Its memo is the
        //    account index record (SPEC 4.3), so the buyer's seed alone finds the new vault.
        Out { to: addr(&a0), value: change, memo: encode_memo(K_ACCOUNT, &coll, l.item, next_hop, &encode_account_record(acct_index(bv_n))), ovk: ovk(&a0) },
    ];
    let (cpath, anchor) = witness(&mut ch.c, &ccmx, l.carrier.height, tip_h).await?;
    let (fpath, fanchor) = witness(&mut ch.c, &fund.cmx, fh, tip_h).await?;
    if anchor != fanchor {
        bail!("witness anchors differ");
    }
    // EXPIRED=1: target far enough in the past that the expiry height is below the tip.
    let target = if std::env::var("EXPIRED").is_ok() { tip_h.checked_sub(100) } else { tip_h.checked_add(1) }.ok_or_else(|| anyhow!("bad tip height"))?;
    let target = BlockHeight::from_u32(u32::try_from(target)?);
    let secrets = vec![
        ("funding FVK", a0.fvk.to_bytes().to_vec()),
        ("funding note rho", fund.note.rho().to_bytes().to_vec()),
        ("funding note rseed", fund.note.rseed().as_bytes().to_vec()),
        ("buyer change address", addr(&a0).to_raw_address_bytes().to_vec()),
        ("buyer vault address", addr(&bv).to_raw_address_bytes().to_vec()),
    ];
    let sale = carrier_sale::BuyerSale {
        vault: &vault,
        carrier,
        carrier_path: cpath,
        fund_fvk: &a0.fvk,
        fund_sak: &a0.sak,
        fund: fund.note,
        fund_path: fpath,
        anchor,
        outs: outs.map(|o| carrier_sale::Out { to: o.to, value: o.value, memo: o.memo, ovk: o.ovk }),
        target,
        expiry: target + env_u32("PCZT_TTL", 20)?,
        listing_id: lid,
        secrets,
    };
    let t = std::time::Instant::now();
    let pk = carrier_sale::proving_key();
    println!("  proving key {:.1}s", t.elapsed().as_secs_f64());
    let built = carrier_sale::buyer_build(sale, &pk, &mut OsRng, &mut |l| println!("{l}"))?;
    fs::write(sale_pczt_path(), &built.pczt)?;
    println!("SALE-BUY wrote {} ({} bytes), carrier action {}, pays {pay_value} zats", sale_pczt_path().display(), built.pczt.len(), built.carrier_action);
    Ok(())
}

/// Seller: check the buyer's PCZT against the listing, sign the carrier spend, extract (which
/// verifies the proof and every signature) and broadcast unless DRY.
/// Test knob: TAMPER_AFTER_SIGN=1 flips one byte of the payment ciphertext after signing and
/// checks that extraction fails. Nothing is broadcast in that mode.
async fn sale_accept(c: Client, dry: bool) -> Result<()> {
    let l: Listing = serde_json::from_slice(&read_capped(&listing_path())?)?;
    let (seller, pay_acct) = (acct(env_u32("SELLER_ACCT", 3)?)?, acct(env_u32("PAY_ACCT", 5)?)?);
    if fvk_hex(&seller.fvk) != l.vault_fvk {
        bail!("this listing is not for SELLER_ACCT's vault");
    }
    let pay = pay_address(&l)?;
    if pay_acct.fvk.scope_for_address(&pay) != Some(Scope::External) {
        bail!("the listing's pay address does not belong to PAY_ACCT");
    }
    let carrier = note_from_json(&l.carrier.note)?;
    let nf = carrier.nullifier(&seller.fvk).to_bytes();
    let ch = Chain::index(c, l.carrier.height).await?;
    // The carrier must still be ours to sell; the rest of the checks are in carrier-sale.
    if ch.spent_at(&nf).is_some() {
        bail!("REFUSED: the carrier is already spent");
    }
    let terms = carrier_sale::SellerTerms { listing_id: listing_id(&l)?, listing_expiry: l.expiry, price: l.price, pay, pay_fvk: &pay_acct.fvk, carrier_nf: nf, tip: ch.to };
    let checked = carrier_sale::seller_check(&read_capped(&sale_pczt_path())?, &terms)?;
    println!("  checks ok: carrier unspent, {}", checked.summary);
    let (ai, pay_actions) = (checked.carrier_action, checked.payment_actions.clone());
    let signed = carrier_sale::seller_sign(checked, &seller.sak)?;
    if std::env::var("TAMPER_AFTER_SIGN").is_ok() {
        // P3: a seller who edits the effects after the buyer signed must not get a valid tx.
        let pa = *pay_actions.first().ok_or_else(|| anyhow!("no payment output"))?;
        let fx = signed.clone().into_effects().map_err(|e| anyhow!("effects: {e:?}"))?;
        let enc = fx.ironwood_bundle().ok_or_else(|| anyhow!("no Ironwood bundle"))?.actions()[pa].encrypted_note().enc_ciphertext.to_vec();
        let mut bytes = signed.serialize().map_err(|e| anyhow!("serialize: {e:?}"))?;
        let pos = bytes.windows(64).position(|w| w == &enc[..64]).ok_or_else(|| anyhow!("payment ciphertext not found in PCZT bytes"))?;
        bytes[pos + 16] ^= 0x01;
        match pczt::Pczt::parse(&bytes) {
            Err(e) => println!("  tampered PCZT does not parse: {e:?}"),
            Ok(t) => match pczt::roles::tx_extractor::TransactionExtractor::new(t).extract() {
                Ok(tx) => bail!("UNEXPECTED: tampered tx extracted {}", tx.txid()),
                Err(e) => println!("  seller-tamper extract refused: {e:?}"),
            },
        }
        println!("SALE-ACCEPT TAMPER TEST done, nothing broadcast");
        return Ok(());
    }
    let mut c = ch.c;
    let (tx, raw, succ) = carrier_sale::extract(signed, ai, &nf)?;
    let bundle = tx.ironwood_bundle().ok_or_else(|| anyhow!("no Ironwood bundle"))?;
    let succ = hex::encode(succ);
    let txid = tx.txid().to_string();
    println!("  extracted tx {txid}: {} actions, {} bytes, successor cmx {}..", bundle.actions().len(), raw.len(), &succ[..12]);
    if dry {
        fs::write(sale_dir().join("sale_dry_tx.bin"), &raw)?;
        println!("SALE-ACCEPT DRY {txid} (not broadcast, bytes in sale_dry_tx.bin)");
        return Ok(());
    }
    let r = c.send_transaction(zcash_client_backend::proto::service::RawTransaction { data: raw, height: 0 }).await?.into_inner();
    if r.error_code != 0 {
        bail!("rejected by node: code {} {}", r.error_code, r.error_message);
    }
    let mut pkg = l.package.clone();
    pkg.hops.push(Hop { note: l.carrier.note.clone(), vault_fvk: l.vault_fvk.clone() });
    save(&format!("package_item{}_hop{}.json", l.item, pkg.hops.len()), &pkg)?;
    save("sale.json", &serde_json::json!({ "txid": txid, "carrier_action": ai, "successor_cmx": succ }))?;
    println!("SALE-ACCEPT {txid}");
    Ok(())
}

/// Seed-only recovery (marketplace SPEC 4.3): scan account 0 from BIRTHDAY (default: Ironwood
/// activation) for account index records. A record is accepted only if its tx gives the vault it
/// names a valid successor carrier (R2) for the same item and hop. Reports each item, whether its
/// carrier is still unspent, and the next free vault index. Read-only.
async fn recover(mut c: Client) -> Result<()> {
    use std::collections::{BTreeMap, BTreeSet, HashSet};
    let seed = seed()?;
    let a0 = FullViewingKey::from(&spending_key(&seed, 0));
    let pivk = orchard::keys::PreparedIncomingViewingKey::new(&a0.to_ivk(Scope::External));
    let (from, to) = (env_u64("BIRTHDAY", 4_134_000)?, tip(&mut c).await?);
    let (mut hits, mut nfs) = (BTreeSet::new(), HashSet::new());
    let mut s = from;
    while s <= to {
        let e = s.saturating_add(9_999).min(to);
        for b in blocks(&mut c, s, e).await? {
            for tx in &b.vtx {
                for a in &tx.ironwood_actions {
                    nfs.insert(a.nullifier.clone());
                    let ca = orchard::note_encryption::CompactAction::try_from(a).map_err(|e| anyhow!("compact action: {e:?}"))?;
                    let d = orchard::note_encryption::IronwoodDomain::for_compact_action(&ca);
                    if zcash_note_encryption::try_compact_note_decryption(&d, &pivk, &ca).is_some() {
                        hits.insert((b.height, txid_hex(&tx.txid)));
                    }
                }
            }
        }
        s = e + 1;
    }
    println!("scanned {from}..={to}: {} tx(s) pay account 0", hits.len());
    // vault index -> accepted (collection, item, hop, carrier txid, action, unspent)
    let mut owned: BTreeMap<u32, Vec<([u8; 32], u32, u32, String, usize, bool)>> = BTreeMap::new();
    for (_, txid) in &hits {
        let (tx, _) = get_tx(&mut c, txid).await?;
        for f in decrypt_ivk(&tx, &a0) {
            let Some(m) = decode_memo(&f.memo).filter(|m| m.kind == K_ACCOUNT) else { continue };
            let Some(idx) = decode_account_record(&m.art) else {
                println!("  ignored: malformed account index record in {}..", &txid[..12]);
                continue;
            };
            let vfvk = FullViewingKey::from(&spending_key(&seed, idx));
            let Some(cf) = decrypt_ivk(&tx, &vfvk).into_iter().find(|v| carrier::verifier::successor_valid(v, &m.coll, m.num, m.hop).is_ok()) else {
                println!("  ignored: record in {}.. names vault {idx}, which gets no valid carrier there", &txid[..12]);
                continue;
            };
            let live = !nfs.contains(&cf.note.nullifier(&vfvk).to_bytes().to_vec());
            let e = owned.entry(idx).or_default();
            if !e.iter().any(|x| x.3 == *txid) {
                e.push((m.coll, m.num, m.hop, txid.clone(), cf.action, live));
            }
        }
    }
    for (idx, recs) in &owned {
        if recs.iter().any(|r| (r.0, r.1) != (recs[0].0, recs[0].1)) {
            println!("CONFLICT: vault index {idx} is named for different items; all of them listed below");
        }
        for (coll, item, hop, txid, action, live) in recs {
            println!("RECOVERED item #{item} of collection {}.. hop {hop}, vault index {idx}, carrier {}..:{action}, {}", &hex::encode(coll)[..16], &txid[..12], if *live { "unspent (held)" } else { "spent (moved on)" });
        }
    }
    println!("next vault index: {}", owned.keys().next_back().map_or(1, |i| i + 1));
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
