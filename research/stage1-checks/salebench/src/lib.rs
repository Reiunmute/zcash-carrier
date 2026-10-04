//! The whole two-party sale on synthetic notes (carrier-sale::selftest), multithreaded in wasm.
use wasm_bindgen::prelude::*;
pub use wasm_bindgen_rayon::init_thread_pool;

#[wasm_bindgen]
pub fn sale_selftest() -> Result<String, JsError> {
    let mut log = String::new();
    let txid = carrier_sale::selftest(&mut rand_core::OsRng, &mut |l| {
        log.push_str(&l);
        log.push('\n');
    })
    .map_err(|e| JsError::new(&format!("{e:#}")))?;
    Ok(format!("{log}txid {txid}"))
}
