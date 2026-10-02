//! carrier: note-bound NFT PoC on Zcash testnet (Ironwood pool).
//!
//! Roles are separate ZIP-32 accounts derived from one local test seed:
//!   0 funding wallet, 1 issuer (sequence), 2 vault for item #1, 3 buyer vault, 4 vault for item #2.
use anyhow::{anyhow, bail, Context, Result};
use carrier::verifier::{cross_check, issuance_set, verify_item, Chain, Verdict};
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
        // Two-party atomic sale via PCZT (docs/marketplace/SPEC.md section 3). See sale_* below.
        "sale-list" => sale_list(c).await?,
        "sale-buy" => sale_buy(c).await?,
        "sale-accept" => sale_accept(c, dry).await?,
        "keys" => {
            // Public: issuer fvk (collection audit). Private to holders: vault fvks in packages.
            // acct5 (seller pay), acct6 and acct7 (buyer vaults) are the sale roles.
            for n in 0..8 {
                println!("acct{n} fvk {}", fvk_hex(&acct(n)?.fvk));
            }
        }
        _ => println!("commands: info | scan <from_height> | genesis | final | transfer | attack-double | attack-forge | attack-mint | sale-list | sale-buy | sale-accept | keys"),
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

const MAX_FILE_BYTES: u64 = 1 << 20;
const MAX_ACTIONS: usize = 16;
/// Longest PCZT validity a seller accepts, in blocks past the current tip (SPEC 3.4).
const MAX_PCZT_TTL: u64 = 40;
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

/// Reads a file received from the other party, refusing anything over `MAX_FILE_BYTES`.
fn read_capped(p: &std::path::Path) -> Result<Vec<u8>> {
    let n = fs::metadata(p).with_context(|| p.display().to_string())?.len();
    if n > MAX_FILE_BYTES {
        bail!("{} is {n} bytes, over the {MAX_FILE_BYTES} byte limit", p.display());
    }
    Ok(fs::read(p)?)
}

fn pay_address(l: &Listing) -> Result<orchard::Address> {
    let raw: [u8; 43] = hex::decode(&l.seller_pay_addr)?.try_into().map_err(|_| anyhow!("pay address length"))?;
    Option::from(orchard::Address::from_raw_address_bytes(&raw)).ok_or_else(|| anyhow!("pay address"))
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Trial-decrypts every action of an Ironwood bundle (any authorization state) with `fvk`.
fn decrypt_actions<T: orchard::bundle::Authorization>(b: &orchard::Bundle<T, zcash_protocol::value::ZatBalance>, fvk: &FullViewingKey) -> Vec<(usize, Note)> {
    let pivk = orchard::keys::PreparedIncomingViewingKey::new(&fvk.to_ivk(Scope::External));
    b.actions()
        .iter()
        .enumerate()
        .filter_map(|(i, a)| {
            let d = orchard::note_encryption::IronwoodDomain::for_action(a);
            zcash_note_encryption::try_note_decryption(&d, &pivk, a).map(|(n, _, _)| (i, n))
        })
        .collect()
}

/// Runs R1 to R3 for `pkg` with `holder` (from the collection's genesis height) and returns the
/// chain index, the held carrier and its hop number. Anything other than HELD is refused.
async fn verified_carrier(mut c: Client, col: &Collection, pkg: &Package, holder: &FullViewingKey) -> Result<(Chain, Held, u32)> {
    let (_, gh) = get_tx(&mut c, &col.genesis_txid).await?;
    let gh = gh.ok_or_else(|| anyhow!("genesis tx unmined"))?;
    let mut ch = Chain::index(c, gh).await?;
    let set = issuance_set(&mut ch, col).await?;
    match verify_item(&mut ch, col, &set, pkg, Some(holder)).await? {
        Verdict::Held { cur, txid, height, hop } => {
            cross_check(&mut ch).await?;
            Ok((ch, Held { note: note_to_json(&cur.note), cmx: hex::encode(cur.cmx), height, txid }, hop))
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
    let (ch, held, hop) = verified_carrier(c, &col, &pkg, &seller.fvk).await?;
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
    use pczt::roles::{creator::Creator, io_finalizer::IoFinalizer, prover::Prover, redactor::Redactor, signer::Signer, tx_extractor::TransactionExtractor};
    let l: Listing = serde_json::from_slice(&read_capped(&listing_path())?)?;
    let vault = fvk_from_hex(&l.vault_fvk)?;
    // Never trust the listing's note: verify the current carrier from chain with the listed key.
    let (mut ch, held, hop) = verified_carrier(c, &l.collection, &l.package, &vault).await?;
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

    let (a0, bv) = (acct(0)?, acct(env_u32("BUYER_VAULT_ACCT", 6)?)?);
    let fund_tx = std::env::var("FUND_TX").map_err(|_| anyhow!("set FUND_TX to the tx holding the buyer's funding note"))?;
    let (fund, fh) = find_change(&mut ch.c, &fund_tx, &a0.fvk).await?;
    if ch.spent_at(&fund.note.nullifier(&a0.fvk).to_bytes()).is_some() {
        bail!("the funding note in {fund_tx} is already spent");
    }
    let pay = pay_address(&l)?;
    let pay_value = env_u64("TAMPER_PRICE", l.price)?;
    let need = pay_value.checked_add(fee(3)).ok_or_else(|| anyhow!("price overflow"))?;
    let change = fund.note.value().inner().checked_sub(need).ok_or_else(|| anyhow!("funding note too small"))?;
    let coll = arr32(&l.collection.coll)?;
    let next_hop = hop.checked_add(1).ok_or_else(|| anyhow!("hop overflow"))?;
    let outs = [
        // 0: successor carrier, must share the carrier spend's Action. The buyer's own OVK, so
        //    the seller cannot recover the buyer's new vault address from chain.
        Out { to: addr(&bv), value: 0, memo: encode_memo(K_HOP, &coll, l.item, next_hop, &[]), ovk: ovk(&bv) },
        // 1: payment to the listing's pay address
        Out { to: pay, value: pay_value, memo: empty_memo(), ovk: ovk(&a0) },
        // 2: buyer change; the buyer pays the fee
        Out { to: addr(&a0), value: change, memo: empty_memo(), ovk: ovk(&a0) },
    ];
    let (cpath, anchor) = witness(&mut ch.c, &ccmx, l.carrier.height, tip_h).await?;
    let (fpath, fanchor) = witness(&mut ch.c, &fund.cmx, fh, tip_h).await?;
    if anchor != fanchor {
        bail!("witness anchors differ");
    }
    // EXPIRED=1: target far enough in the past that the expiry height is below the tip.
    let target = if std::env::var("EXPIRED").is_ok() { tip_h.checked_sub(100).ok_or_else(|| anyhow!("tip too low"))? } else { tip_h + 1 };
    let target = BlockHeight::from_u32(u32::try_from(target)?);
    let expiry = target + env_u32("PCZT_TTL", 20)?;
    let mut built = None;
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
        )
        .with_expiry_height(expiry);
        b.add_ironwood_spend::<std::convert::Infallible>(vault.clone(), carrier, cpath.clone()).map_err(|e| anyhow!("spend: {e}"))?;
        b.add_ironwood_spend::<std::convert::Infallible>(a0.fvk.clone(), fund.note, fpath.clone()).map_err(|e| anyhow!("spend: {e}"))?;
        for o in &outs {
            let memo = MemoBytes::from_bytes(&o.memo).map_err(|e| anyhow!("memo {e:?}"))?;
            let v = Zatoshis::from_u64(o.value).map_err(|_| anyhow!("amount {} out of range", o.value))?;
            b.add_ironwood_output::<std::convert::Infallible>(o.ovk.clone(), o.to, v, memo).map_err(|e| anyhow!("output: {e}"))?;
        }
        let res = b.build_for_pczt(OsRng, &FeeRule::standard()).map_err(|e| anyhow!("build_for_pczt: {e}"))?;
        let m = &res.ironwood_meta;
        let idx = |o: Option<usize>| o.ok_or_else(|| anyhow!("bundle metadata"));
        let (cai, fai) = (idx(m.spend_action_index(0))?, idx(m.spend_action_index(1))?);
        if m.output_action_index(0) != Some(cai) {
            println!("  attempt {attempt}: binding not satisfied, rebuilding");
            continue;
        }
        let (pai, xai) = (idx(m.output_action_index(1))?, idx(m.output_action_index(2))?);
        println!("  attempt {attempt}: bound ok, carrier action {cai}, funding action {fai}");
        built = Some((Creator::build_from_parts(res.pczt_parts).ok_or_else(|| anyhow!("tx version not PCZT-compatible"))?, cai, fai, pai, xai));
        break;
    }
    let (p, cai, fai, pai, xai) = built.ok_or_else(|| anyhow!("binding not satisfied after {MAX_TRIES} tries"))?;
    println!("  pczt expiry_height {}", p.global().expiry_height());
    let p = IoFinalizer::new(p).finalize_io().map_err(|e| anyhow!("io_finalize: {e:?}"))?;
    let t = std::time::Instant::now();
    let pk = orchard::circuit::ProvingKey::build(orchard::bundle::BundleVersion::ironwood_v3().circuit_version());
    let tk = t.elapsed();
    let p = Prover::new(p).create_ironwood_proof(&pk).map_err(|e| anyhow!("prove: {e:?}"))?.finish();
    println!("  proving key {:.1}s, proof {:.1}s", tk.as_secs_f64(), (t.elapsed() - tk).as_secs_f64());
    let mut s = Signer::new(p).map_err(|e| anyhow!("signer: {e:?}"))?;
    s.sign_ironwood(fai, &a0.sak).map_err(|e| anyhow!("sign funding: {e:?}"))?;
    // Redaction (SPEC 3.3): nothing about the buyer beyond what the effects must show.
    let p = Redactor::new(s.finish())
        .redact_ironwood_with(|mut r| {
            r.redact_action(fai, |mut a| {
                a.clear_spend_fvk();
                a.clear_spend_witness();
                a.clear_spend_zip32_derivation();
                a.clear_spend_recipient();
                a.clear_spend_value();
                a.clear_spend_rho();
                a.clear_spend_rseed();
                a.clear_spend_alpha();
                a.clear_spend_proprietary();
            });
            for oi in [cai, pai, xai] {
                r.redact_action(oi, |mut a| {
                    a.clear_output_recipient();
                    a.clear_output_value();
                    a.clear_output_rseed();
                    a.clear_output_ock();
                    a.clear_output_zip32_derivation();
                    a.clear_output_user_address();
                    a.clear_output_proprietary();
                });
            }
            r.redact_actions(|mut a| {
                a.clear_rcv();
                a.clear_spend_dummy_sk();
            });
        })
        .finish();
    // Self-checks. 1: the buyer alone cannot produce a valid tx (the carrier spend is unsigned).
    match TransactionExtractor::new(p.clone()).extract() {
        Ok(tx) => bail!("UNEXPECTED: buyer alone extracted tx {}", tx.txid()),
        Err(e) => println!("  buyer-alone extract refused: {e:?}"),
    }
    // 2: the redacted PCZT still parses for the seller's signer.
    Signer::new(p.clone()).map_err(|e| anyhow!("redacted PCZT no longer signable: {e:?}"))?;
    let bytes = p.serialize().map_err(|e| anyhow!("serialize: {e:?}"))?;
    // 3: none of the buyer's key, funding-note fields or addresses appear in the bytes.
    let secrets: [(&str, Vec<u8>); 5] = [
        ("funding FVK", a0.fvk.to_bytes().to_vec()),
        ("funding note rho", fund.note.rho().to_bytes().to_vec()),
        ("funding note rseed", fund.note.rseed().as_bytes().to_vec()),
        ("buyer change address", addr(&a0).to_raw_address_bytes().to_vec()),
        ("buyer vault address", addr(&bv).to_raw_address_bytes().to_vec()),
    ];
    for (what, b) in &secrets {
        if contains(&bytes, b) {
            bail!("redaction check failed: {what} is still in the PCZT");
        }
    }
    println!("  redaction check: no funding FVK, funding-note fields or buyer addresses in the PCZT");
    fs::write(sale_pczt_path(), &bytes)?;
    println!("SALE-BUY wrote {} ({} bytes), carrier action {cai}, pays {pay_value} zats", sale_pczt_path().display(), bytes.len());
    Ok(())
}

/// Seller: check the buyer's PCZT against the listing, sign the carrier spend, extract (which
/// verifies the proof and every signature) and broadcast unless DRY.
/// Test knob: TAMPER_AFTER_SIGN=1 flips one byte of the payment ciphertext after signing and
/// checks that extraction fails. Nothing is broadcast in that mode.
async fn sale_accept(c: Client, dry: bool) -> Result<()> {
    use pczt::roles::{signer::Signer, tx_extractor::TransactionExtractor};
    let l: Listing = serde_json::from_slice(&read_capped(&listing_path())?)?;
    let (seller, pay_acct) = (acct(env_u32("SELLER_ACCT", 3)?)?, acct(env_u32("PAY_ACCT", 5)?)?);
    if fvk_hex(&seller.fvk) != l.vault_fvk {
        bail!("this listing is not for SELLER_ACCT's vault");
    }
    let pay = pay_address(&l)?;
    if pay_acct.fvk.scope_for_address(&pay) != Some(Scope::External) {
        bail!("the listing's pay address does not belong to PAY_ACCT");
    }
    let p = pczt::Pczt::parse(&read_capped(&sale_pczt_path())?).map_err(|e| anyhow!("parse pczt: {e:?}"))?;
    let carrier = note_from_json(&l.carrier.note)?;
    let nf = carrier.nullifier(&seller.fvk).to_bytes();
    let ch = Chain::index(c, l.carrier.height).await?;
    let tip_h = ch.to;
    // 1. the carrier is still ours to sell, and the listing is still open
    if ch.spent_at(&nf).is_some() {
        bail!("REFUSED: the carrier is already spent");
    }
    if tip_h >= l.expiry {
        bail!("REFUSED: listing expired at {} (tip {tip_h})", l.expiry);
    }
    // 2. PCZT expiry: minable a few blocks from now, and no later than the listing or tip + MAX_PCZT_TTL
    let exp = *p.global().expiry_height() as u64;
    if exp < tip_h + 3 {
        bail!("REFUSED: PCZT expired or about to (expiry_height {exp}, tip {tip_h})");
    }
    if exp > l.expiry || exp > tip_h + MAX_PCZT_TTL {
        bail!("REFUSED: PCZT expiry_height {exp} is past the listing expiry {} or tip + {MAX_PCZT_TTL}", l.expiry);
    }
    // 3. network upgrade
    let branch = u32::from(zcash_protocol::consensus::BranchId::for_height(&TEST_NETWORK, BlockHeight::from_u32(u32::try_from(tip_h + 1)?)));
    if *p.global().consensus_branch_id() != branch {
        bail!("REFUSED: consensus branch {:08x} != {branch:08x}", p.global().consensus_branch_id());
    }
    // 4. only an Ironwood bundle, of bounded size
    if !p.orchard().actions().is_empty() || !p.sapling().spends().is_empty() || !p.sapling().outputs().is_empty() || !p.transparent().inputs().is_empty() || !p.transparent().outputs().is_empty() {
        bail!("REFUSED: PCZT uses a value pool other than Ironwood");
    }
    // Checks below read the effects (ciphertexts, nullifiers), not the PCZT's plaintext fields.
    let fx = p.clone().into_effects().map_err(|e| anyhow!("effects: {e:?}"))?;
    let b = fx.ironwood_bundle().ok_or_else(|| anyhow!("REFUSED: no Ironwood bundle"))?;
    if b.actions().len() > MAX_ACTIONS {
        bail!("REFUSED: {} actions, more than {MAX_ACTIONS}", b.actions().len());
    }
    // 5. the carrier spend is in the tx
    let ai = b.actions().iter().position(|a| a.nullifier().to_bytes() == nf).ok_or_else(|| anyhow!("REFUSED: carrier spend not in PCZT"))?;
    // 6. payment: outputs to the listing's pay address sum to the price
    let paid_outs: Vec<(usize, Note)> = decrypt_actions(b, &pay_acct.fvk).into_iter().filter(|(_, n)| n.recipient() == pay).collect();
    let paid = paid_outs.iter().try_fold(0u64, |acc, (_, n)| acc.checked_add(n.value().inner())).ok_or_else(|| anyhow!("REFUSED: payment sum overflows"))?;
    if paid != l.price {
        bail!("REFUSED: payment {paid} zats != listed price {}", l.price);
    }
    println!("  checks ok: carrier unspent, listing open, expiry {exp} (tip {tip_h}), branch {branch:08x}, {} actions, carrier spend in action {ai}, payment {paid} zats", b.actions().len());
    let mut s = Signer::new(p).map_err(|e| anyhow!("signer: {e:?}"))?;
    s.sign_ironwood(ai, &seller.sak).map_err(|e| anyhow!("sign carrier: {e:?}"))?;
    let signed = s.finish();
    if std::env::var("TAMPER_AFTER_SIGN").is_ok() {
        // P3: a seller who edits the effects after the buyer signed must not get a valid tx.
        let pa = paid_outs.first().map(|(i, _)| *i).ok_or_else(|| anyhow!("no payment output"))?;
        let enc = b.actions()[pa].encrypted_note().enc_ciphertext.to_vec();
        let mut bytes = signed.serialize().map_err(|e| anyhow!("serialize: {e:?}"))?;
        let pos = bytes.windows(64).position(|w| w == &enc[..64]).ok_or_else(|| anyhow!("payment ciphertext not found in PCZT bytes"))?;
        bytes[pos + 16] ^= 0x01;
        match pczt::Pczt::parse(&bytes) {
            Err(e) => println!("  tampered PCZT does not parse: {e:?}"),
            Ok(t) => match TransactionExtractor::new(t).extract() {
                Ok(tx) => bail!("UNEXPECTED: tampered tx extracted {}", tx.txid()),
                Err(e) => println!("  seller-tamper extract refused: {e:?}"),
            },
        }
        println!("SALE-ACCEPT TAMPER TEST done, nothing broadcast");
        return Ok(());
    }
    let mut c = ch.c;
    let tx = TransactionExtractor::new(signed).extract().map_err(|e| anyhow!("extract: {e:?}"))?;
    let bundle = tx.ironwood_bundle().ok_or_else(|| anyhow!("no Ironwood bundle"))?;
    let act = &bundle.actions()[ai];
    if act.nullifier().to_bytes() != nf {
        bail!("extracted tx moved the carrier spend");
    }
    let succ = hex::encode(act.cmx().to_bytes());
    let txid = tx.txid().to_string();
    let mut raw = vec![];
    tx.write(&mut raw)?;
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

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
