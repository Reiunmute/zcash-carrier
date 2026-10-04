//! Browser entry points over `carrier-core`, and with the `sale` feature over `carrier-sale`.
//! Pure functions only: chain access (grpc-web) stays in JavaScript.
use wasm_bindgen::prelude::*;

#[cfg(feature = "sale")]
mod browser;

fn err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// Decodes a 512-byte carrier memo to JSON `{kind, coll, num, hop, art_len}`, or `null` if it is not one.
#[wasm_bindgen]
pub fn decode_memo(memo: &[u8]) -> Result<String, JsError> {
    let m: [u8; 512] = memo.try_into().map_err(|_| err("memo must be 512 bytes"))?;
    Ok(match carrier_core::decode_memo(&m) {
        None => "null".into(),
        Some(d) => serde_json::json!({ "kind": d.kind, "coll": hex::encode(d.coll), "num": d.num, "hop": d.hop, "art_len": d.art.len() }).to_string(),
    })
}

/// The FINAL seal address of a collection, as raw 43-byte hex.
#[wasm_bindgen]
pub fn seal_address_hex(coll_hex: &str) -> Result<String, JsError> {
    let coll: [u8; 32] = hex::decode(coll_hex).map_err(err)?.try_into().map_err(|_| err("collection id must be 32 bytes"))?;
    Ok(hex::encode(carrier_core::seal_address(&coll).to_raw_address_bytes()))
}

/// SPEC 2.1 step 4.
#[wasm_bindgen]
pub fn collectibles_seed(wallet_signature: &[u8], message: &[u8]) -> Result<Vec<u8>, JsError> {
    Ok(carrier_core::keys::collectibles_seed(wallet_signature, message).map_err(err)?.to_vec())
}

/// Verifies a vault signature against the `ak` inside a disclosed FVK (hex).
#[wasm_bindgen]
pub fn vault_verify(fvk_hex: &str, msg: &[u8], sig: &[u8]) -> Result<bool, JsError> {
    let fvk = carrier_core::fvk_from_hex(fvk_hex).map_err(err)?;
    let sig: [u8; 64] = sig.try_into().map_err(|_| err("signature must be 64 bytes"))?;
    Ok(carrier_core::keys::vault_verify(&fvk, msg, &sig).is_ok())
}

// ---------- disclosure channel (marketplace SPEC 3.2) ----------
use carrier_core::channel as ch;

fn arr<const N: usize>(b: &[u8], what: &str) -> Result<[u8; N], JsError> {
    b.try_into().map_err(|_| err(format!("{what} must be {N} bytes")))
}

/// A fresh X25519 channel key pair: 64 bytes, private key then public key.
#[wasm_bindgen]
pub fn channel_keypair() -> Vec<u8> {
    let (sk, pk) = ch::channel_keypair(&mut rand_core::OsRng);
    [sk, pk].concat()
}

/// The transcript `T` (32 bytes).
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn channel_transcript(listing_id: &[u8], coll: &[u8], num: u32, price: u64, expiry_height: u64, seller_key: &[u8], buyer_key: &[u8], nonce: &[u8]) -> Result<Vec<u8>, JsError> {
    Ok(ch::transcript(&ch::Terms {
        listing_id: arr(listing_id, "listing id")?,
        coll: arr(coll, "collection id")?,
        num,
        price,
        expiry_height,
        seller_key: arr(seller_key, "seller key")?,
        buyer_key: arr(buyer_key, "buyer key")?,
        nonce: arr(nonce, "nonce")?,
    })
    .to_vec())
}

#[wasm_bindgen]
pub fn channel_seal(recipient: &[u8], kind: u8, direction: u8, seq: u32, t: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, JsError> {
    ch::seal(&arr(recipient, "recipient key")?, kind, direction, seq, &arr(t, "transcript")?, plaintext, &mut rand_core::OsRng).map_err(err)
}

#[wasm_bindgen]
pub fn channel_open(sk: &[u8], kind: u8, direction: u8, seq: u32, t: &[u8], msg: &[u8]) -> Result<Vec<u8>, JsError> {
    ch::open(&arr(sk, "private key")?, kind, direction, seq, &arr(t, "transcript")?, msg).map_err(err)
}

/// Buyer: opens a disclosure and checks its vault signature. JSON `{vault_fvk, carrier_txid,
/// carrier_action, pay_addr, package}` (hex; the txid in display order).
#[wasm_bindgen]
pub fn disclosure_open(buyer_sk: &[u8], seq: u32, t: &[u8], msg: &[u8]) -> Result<String, JsError> {
    let d = ch::open_disclosure(&arr(buyer_sk, "private key")?, seq, &arr(t, "transcript")?, msg).map_err(err)?;
    Ok(serde_json::json!({
        "vault_fvk": hex::encode(d.vault_fvk),
        "carrier_txid": carrier_core::txid_hex(&d.carrier_txid),
        "carrier_action": d.carrier_action,
        "pay_addr": hex::encode(d.pay_addr),
        "package": hex::encode(&d.package),
    })
    .to_string())
}

#[wasm_bindgen]
pub fn pow_ok(t: &[u8], bits: u32, stamp: u64) -> Result<bool, JsError> {
    Ok(ch::pow_ok(&arr(t, "transcript")?, bits, stamp))
}

#[wasm_bindgen]
pub fn pow_find(t: &[u8], bits: u32) -> Result<u64, JsError> {
    Ok(ch::pow_find(&arr(t, "transcript")?, bits))
}

/// Runs the whole two-party sale on synthetic notes (build, prove, sign, redact, seller checks,
/// sign, extract with proof verification). Returns the log and the txid.
#[cfg(feature = "sale")]
#[wasm_bindgen]
pub fn sale_selftest() -> Result<String, JsError> {
    let mut log = String::new();
    let txid = carrier_sale::selftest(&mut rand_core::OsRng, &mut |l| {
        log.push_str(&l);
        log.push('\n');
    })
    .map_err(|e| err(format!("{e:#}")))?;
    Ok(format!("{log}txid {txid}"))
}
