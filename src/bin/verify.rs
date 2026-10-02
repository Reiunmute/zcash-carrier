//! verify: independent verifier. Uses only chain data, the published collection record, the
//! provenance package a buyer receives, and the holder's viewing key. It never reads the local seed.
//! Rules R1 to R3 and the trust model are documented in `src/verifier.rs` and `docs/SPEC.md`.
//!
//! Usage:
//!   verify <collection.json> <from_height> [<package.json> [claimed_txid...]]   (HOLDER_FVK=<hex> for R3)
//!   verify payment <txid> <fvk_hex> <expected_zats>
//!   verify mined <txid>
//!   verify payment-file <raw_tx_hex_file> <txid> <fvk_hex> <expected_zats>
//!
//! Exit codes: 0 held (or no package), 2 fail, 3 burned, 4 spent, 5 unverified (no holder key).
use anyhow::{anyhow, bail, Result};
use carrier::verifier::{self, cross_check, issuance_set, paid_to, verify_item, Chain, ChainSource, Verdict};
use carrier::*;
use std::fs;

const USAGE: &str = "usage: verify <collection.json> <from_height> [<package.json> [claimed_txid...]] | verify payment <txid> <fvk_hex> <expected_zats> | verify mined <txid> | verify payment-file <hex_file> <txid> <fvk_hex> <expected_zats>";

fn load<T: serde::de::DeserializeOwned>(p: &str) -> Result<T> {
    Ok(serde_json::from_slice(&read_capped(std::path::Path::new(p))?)?)
}

/// `verify payment-file`: parse published raw tx bytes offline, check their txid and the payment
/// from those exact bytes, then require the server's copy to be byte-identical and mined.
async fn payment_file(a: &[String]) -> Result<i32> {
    let [file, txid, fvk, expected] = a else { return Err(anyhow!(USAGE)) };
    let hex_str = String::from_utf8(read_capped(std::path::Path::new(file))?)?;
    let raw = hex::decode(hex_str.trim())?;
    let tx = zcash_primitives::transaction::Transaction::read(&raw[..], zcash_protocol::consensus::BranchId::Nu6_3)?;
    if tx.txid().to_string() != *txid {
        bail!("{file} hashes to {}, not {txid}", tx.txid());
    }
    let paid = verifier::paid_to(tx.ironwood_bundle().ok_or_else(|| anyhow!("{file} has no Ironwood bundle"))?, &fvk_from_hex(fvk)?)?;
    let expected: u64 = expected.parse()?;
    let mut c = connect().await?;
    let (stx, h) = get_tx(&mut c, txid).await?;
    let mut sraw = vec![];
    stx.write(&mut sraw)?;
    if sraw != raw {
        bail!("server copy of {txid} differs from {file}");
    }
    let h = h.ok_or_else(|| anyhow!("{txid} unmined"))?;
    println!("PAYMENT {paid} zats to the given key in {file} (txid {}.., identical to the server copy, mined at {h}; expected {expected})", &txid[..12]);
    Ok(if paid == expected { 0 } else { 2 })
}

/// `verify payment`: does `txid` pay exactly `expected` zats to outputs `fvk` can decrypt?
async fn payment(a: &[String]) -> Result<i32> {
    let [txid, fvk, expected] = a else { return Err(anyhow!(USAGE)) };
    let fvk = fvk_from_hex(fvk)?;
    let expected: u64 = expected.parse()?;
    let mut c = connect().await?;
    let (tx, h) = get_tx(&mut c, txid).await?;
    let h = h.ok_or_else(|| anyhow!("{txid} unmined"))?;
    let paid = paid_to(tx.ironwood_bundle().ok_or_else(|| anyhow!("{txid} has no Ironwood bundle"))?, &fvk)?;
    println!("PAYMENT {paid} zats to the given key in {}..@{h} (expected {expected})", &txid[..12]);
    Ok(if paid == expected { 0 } else { 2 })
}

async fn run() -> Result<i32> {
    let a: Vec<String> = std::env::args().collect();
    if a.get(1).map(String::as_str) == Some("payment") {
        return payment(&a[2..]).await;
    }
    if a.get(1).map(String::as_str) == Some("payment-file") {
        return payment_file(&a[2..]).await;
    }
    if a.get(1).map(String::as_str) == Some("mined") {
        let txid = a.get(2).ok_or_else(|| anyhow!(USAGE))?;
        let (_, h) = get_tx(&mut connect().await?, txid).await?;
        println!("MINED {}", h.map(|h| h.to_string()).unwrap_or_else(|| "no".into()));
        return Ok(if h.is_some() { 0 } else { 2 });
    }
    let (Some(colp), Some(from)) = (a.get(1), a.get(2)) else { return Err(anyhow!(USAGE)) };
    let col: Collection = load(colp)?;
    let issuer = fvk_from_hex(&col.issuer_fvk)?;
    let from: u64 = from.parse()?;
    let mut ch = Chain::index(connect().await?, from).await?;
    println!("== collection {}", col.coll.get(..16).ok_or_else(|| anyhow!("collection id too short"))?);

    let set = issuance_set(&mut ch, &col, &mut |l| println!("{l}")).await?;
    println!("R1 issuance set: {} item(s), finalized={}, sealed={}", set.items.len(), set.finalized, set.sealed);
    fs::create_dir_all("out")?;
    for i in &set.items {
        println!("   #{} cmx {}.. art {}B in {}..", i.num, &hex::encode(i.cmx)[..12], i.art.len(), &i.txid[..12]);
        if !i.art.is_empty() {
            fs::write(format!("out/item{}_from_chain.bin", i.num), &i.art)?;
        }
    }

    // Stray claims: any tx the issuer key opens that carries a CARRIER memo but is not in the
    // issuance set (attack C). Checked for txids supplied on the command line.
    for extra in a.iter().skip(4) {
        let (tx, _) = ch.tx(extra).await?;
        for f in recover_bundle_ovk(Some(&tx), &issuer).into_iter().chain(decrypt_bundle_ivk(Some(&tx), &issuer)) {
            if let Some(m) = decode_memo(&f.memo) {
                if m.kind == K_CARRIER && !set.items.iter().any(|i| i.cmx == f.cmx) {
                    println!("R1 REJECT claimed item #{} in {}..: not in finalized issuance set", m.num, extra.get(..12).unwrap_or(extra));
                }
            }
        }
    }

    let Some(pp) = a.get(3) else {
        cross_check(&mut ch).await?;
        return Ok(0);
    };
    let pkg: Package = load(pp)?;
    println!("== package item #{} ({})", pkg.item, pkg.label);
    let holder = std::env::var("HOLDER_FVK").ok().map(|h| fvk_from_hex(&h)).transpose()?;
    let verdict = verify_item(&mut ch, &col, &set, &pkg, holder.as_ref(), &mut |l| println!("{l}")).await?;
    let code = match &verdict {
        Verdict::Held { cur, .. } => {
            fs::write("out/current_carrier_cmx.txt", hex::encode(cur.cmx))?;
            println!("VERIFY HELD: item #{} is held by the given key, unspent", pkg.item);
            0
        }
        Verdict::Spent { cmx } => {
            fs::write("out/current_carrier_cmx.txt", hex::encode(cmx))?;
            println!("VERIFY SPENT: item #{}: chain valid, but the given key's carrier is already spent", pkg.item);
            4
        }
        Verdict::Burned { .. } => 3,
        Verdict::Unverified { cmx } => {
            fs::write("out/current_carrier_cmx.txt", hex::encode(cmx))?;
            println!("VERIFY UNVERIFIED: item #{}: R1 and R2 up to the last hop only; give HOLDER_FVK for the rest", pkg.item);
            5
        }
    };
    cross_check(&mut ch).await?;
    Ok(code)
}

#[tokio::main]
async fn main() {
    match run().await {
        Ok(0) => {}
        Ok(code) => std::process::exit(code),
        Err(e) => {
            println!("VERIFY FAIL: {e:#}");
            std::process::exit(2);
        }
    }
}
