//! Native reference values for the wasm parity check (`crates/carrier-wasm/parity.cjs`, run by
//! verify.sh). Uses a fixed TEST seed and the public 2026-09-29 collection id, never the real seed.
use carrier::*;

fn main() -> anyhow::Result<()> {
    let col: Collection = serde_json::from_slice(&read_capped(std::path::Path::new("evidence/testnet-2026-09-29/collection.json"))?)?;
    let coll: [u8; 32] = hex::decode(&col.coll)?.try_into().map_err(|_| anyhow::anyhow!("coll"))?;
    let sk = spending_key(&[9u8; 32], 5);
    let fvk = orchard::keys::FullViewingKey::from(&sk);
    let sig = carrier::keys::vault_sign(&orchard::keys::SpendAuthorizingKey::from(&sk), b"parity transcript", rand::rngs::OsRng);
    println!(
        "{}",
        serde_json::json!({
            "coll": col.coll,
            "seal": hex::encode(seal_address(&coll).to_raw_address_bytes()),
            "seed": hex::encode(carrier::keys::collectibles_seed(b"wallet-signature", b"Carrier collectibles key v1")?),
            "fvk": fvk_hex(&fvk),
            "sig": hex::encode(sig),
            "memo": hex::encode(encode_memo(K_HOP, &coll, 1, 2, &[])),
        })
    );
    Ok(())
}
