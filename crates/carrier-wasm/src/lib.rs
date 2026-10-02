//! Browser entry points over `carrier-core`, and with the `sale` feature over `carrier-sale`.
//! Pure functions only: chain access (grpc-web) stays in JavaScript.
use wasm_bindgen::prelude::*;

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
