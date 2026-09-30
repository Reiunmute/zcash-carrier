//! Measures Ironwood (PostNu6_3) Action proving time: key build + N-action proof.
use orchard::{
    builder::{Builder, BundleType},
    bundle::BundleVersion,
    circuit::ProvingKey,
    keys::{FullViewingKey, Scope, SpendingKey},
    tree::Anchor,
    value::NoteValue,
};
use rand::SeedableRng;

pub struct Timing { pub pk_ms: f64, pub prove_ms: f64, pub actions: usize }

pub fn run(actions: usize, now: &dyn Fn() -> f64) -> Timing {
    let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(7);
    let v = BundleVersion::ironwood_v3();
    let t0 = now();
    let pk = ProvingKey::build(v.circuit_version());
    let t1 = now();
    let sk = SpendingKey::from_bytes([7u8; 32]).unwrap();
    let to = FullViewingKey::from(&sk).address_at(0u32, Scope::External);
    let mut b = Builder::new(BundleType::DEFAULT, v, v.default_flags(), Anchor::empty_tree()).unwrap();
    for _ in 0..actions {
        b.add_output(None, to, NoteValue::from_raw(0), [0u8; 512]).unwrap();
    }
    let (bundle, _meta) = b.build::<i64>(&mut rng).unwrap().unwrap();
    let t2 = now();
    let _proven = bundle.create_proof(&pk, &mut rng).unwrap();
    let t3 = now();
    Timing { pk_ms: t1 - t0, prove_ms: t3 - t2, actions: _proven.actions().len() }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use wasm_bindgen::prelude::*;
    pub use wasm_bindgen_rayon::init_thread_pool;
    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = performance)]
        fn now() -> f64;
    }
    #[wasm_bindgen]
    pub fn bench(actions: usize) -> String {
        let t = super::run(actions, &|| now());
        format!("{{\"pk_ms\":{:.0},\"prove_ms\":{:.0},\"actions\":{}}}", t.pk_ms, t.prove_ms, t.actions)
    }
}
