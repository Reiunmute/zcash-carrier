#!/usr/bin/env bash
# Builds salebench with threads (nightly, build-std, atomics) into salebench/pkg. wasi-sdk compiles
# secp256k1-sys; its C objects need the same atomics and bulk-memory features as the Rust code.
set -euo pipefail
cd "$(dirname "$0")"
W=${WASI_SDK:-$HOME/.local/wasi-sdk}
export CC_wasm32_unknown_unknown="$W/bin/clang" AR_wasm32_unknown_unknown="$W/bin/llvm-ar"
export CFLAGS_wasm32_unknown_unknown="-matomics -mbulk-memory"
cargo +nightly build --release --target wasm32-unknown-unknown
rm -rf pkg
"$HOME/.cargo/bin/wasm-bindgen" --target web --out-dir pkg target/wasm32-unknown-unknown/release/salebench.wasm
cp index.html pkg/
# Without a bundler, wasm-bindgen-rayon's worker imports '../../..' (a directory); point it at the module.
sed -i '' "s#import('../../..')#import('../../../salebench.js')#" pkg/snippets/wasm-bindgen-rayon-*/src/workerHelpers.js
ls -l pkg/*.wasm
