#!/usr/bin/env bash
# Builds the browser bundle for web/prototype: carrier-wasm with the `sale` feature (wasi-sdk
# compiles secp256k1-sys for wasm32), then wasm-bindgen --target web into web/prototype/pkg
# (git-ignored). Serve web/prototype afterwards (deploy/ does this with Caddy).
set -euo pipefail
cd "$(dirname "$0")/.."
W=${WASI_SDK:-$HOME/.local/wasi-sdk}
CC_wasm32_unknown_unknown="$W/bin/clang" AR_wasm32_unknown_unknown="$W/bin/llvm-ar" \
  cargo build --release -p carrier-wasm --features sale --target wasm32-unknown-unknown
rm -rf web/prototype/pkg
"$HOME/.cargo/bin/wasm-bindgen" --target web --out-dir web/prototype/pkg target/wasm32-unknown-unknown/release/carrier_wasm.wasm
ls -l web/prototype/pkg/*.wasm
