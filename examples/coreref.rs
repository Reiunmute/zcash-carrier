//! Native reference values for the wasm parity check (`crates/carrier-wasm/parity.cjs`, run by
//! verify.sh). Uses a fixed TEST seed and the public 2026-09-29 collection id, never the real seed.
use carrier::*;

fn main() -> anyhow::Result<()> {
    let col: Collection = serde_json::from_slice(&read_capped(std::path::Path::new("evidence/testnet-2026-09-29/collection.json"))?)?;
    let coll: [u8; 32] = hex::decode(&col.coll)?.try_into().map_err(|_| anyhow::anyhow!("coll"))?;
    let sk = spending_key(&[9u8; 32], 5);
    let fvk = orchard::keys::FullViewingKey::from(&sk);
    let sak = orchard::keys::SpendAuthorizingKey::from(&sk);
    let sig = carrier::keys::vault_sign(&sak, b"parity transcript", rand::rngs::OsRng);
    // Disclosure channel: native seals, JavaScript (wasm) opens.
    use carrier::channel::*;
    let (bsk, bpk) = channel_keypair(&mut rand::rngs::OsRng);
    let t = transcript(&Terms { listing_id: [1; 32], coll, num: 1, price: 10_000, expiry_height: 4_500_000, seller_key: [3; 32], buyer_key: bpk, nonce: [4; 32] });
    let msg = seal(&bpk, TYPE_PCZT, BUYER_TO_SELLER, 0, &t, b"parity plaintext", &mut rand::rngs::OsRng)?;
    let d = Disclosure { vault_fvk: fvk.to_bytes(), carrier_txid: [6; 32], carrier_action: 2, pay_addr: seal_address(&coll).to_raw_address_bytes(), package: b"{}".to_vec() };
    let disc = seal_disclosure(&d, &sak, &bpk, 0, &t, &mut rand::rngs::OsRng)?;
    println!(
        "{}",
        serde_json::json!({
            "coll": col.coll,
            "seal": hex::encode(seal_address(&coll).to_raw_address_bytes()),
            "seed": hex::encode(carrier::keys::collectibles_seed(b"wallet-signature", b"Carrier collectibles key v1")?),
            "fvk": fvk_hex(&fvk),
            "sig": hex::encode(sig),
            "memo": hex::encode(encode_memo(K_HOP, &coll, 1, 2, &[])),
            "ch_sk": hex::encode(bsk),
            "ch_pk": hex::encode(bpk),
            "ch_t": hex::encode(t),
            "ch_msg": hex::encode(msg),
            "ch_disc": hex::encode(disc),
            "pow_stamp": pow_find(&t, 10),
        })
    );
    Ok(())
}
