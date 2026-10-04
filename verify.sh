#!/usr/bin/env bash
# Gate for this repo. Builds, re-verifies the testnet evidence from chain data, and rebuilds both
# artworks from on-chain memo bytes. Needs network (testnet lightwalletd, set LWD to change it).
#
# Every check runs from published material: the collection record, the provenance packages and the
# test viewing keys in evidence/testnet-2026-09-30/viewing_keys.json (view only, they cannot spend).
# When state/seed.hex exists, the published keys are also checked against the local seed.
#
# Trust: chain data comes from one lightwalletd unless LWD2 names a second server to cross-check.
# Not re-run here: the node's double-spend rejection (logged in evidence/testnet-2026-09-29) and the
# PCZT negative checks (logged in evidence/testnet-2026-09-30 and evidence/testnet-2026-10-02).
set -euo pipefail
cd "$(dirname "$0")"

E=evidence/testnet-2026-09-29
E2=evidence/testnet-2026-09-30
E3=evidence/testnet-2026-10-02
SALE2_TX=281788d78f3e8ab39b208373c9810d0e0383c2cf54beb1b342dfe5bb03c4bf98   # separate-seed sale
SEAL_GENESIS_H=4435127                                                    # sealed collection genesis
P4_TX=e96845ecfc84008674195d8ee77fd433cdb7cdfc3464711ae92221d58ada3b1d     # first sale with vault records
MINT3=78a9a662f825d576abdf14e4d47fe6244a68ecd0a54135fca76eca32001cc3fb
SALE_TX=6bdeb823ff6556dce7ef8c254c8ceaa257913cc820c43da271c52dabdc98ba5b
fail() { echo "VERIFY.SH FAIL: $*" >&2; exit 1; }
want() { grep -qF -- "$2" <<<"$1" || fail "expected output missing: $2"; }
key() { python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))[sys.argv[2]]["fvk"])' "$E2/viewing_keys.json" "$1"; }
key3() { python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))[sys.argv[2]]["fvk"])' "$E3/viewing_keys.json" "$1"; }
# run <expected exit code> <cmd...>: runs, prints, and checks the exit code. Output in $out.
run() {
  local want_code=$1; shift
  set +e; out=$("$@" 2>&1); local code=$?; set -e
  echo "$out"
  [ "$code" -eq "$want_code" ] || fail "exit $code, expected $want_code: $*"
}

mkdir -p out
echo "== build"
cargo build --release --quiet
cargo test --release --quiet -p carrier-core >/dev/null || fail "carrier-core unit tests"
cargo test --release --quiet -p carrier-sale >/dev/null || fail "carrier-sale self-test (native)"
cargo test --release --quiet -p orderbook >/dev/null || fail "orderbook unit tests"
# carrier-core must keep compiling for the browser (marketplace SPEC P7).
if rustup target list --installed 2>/dev/null | grep -q '^wasm32-unknown-unknown$'; then
  cargo build --release --quiet -p carrier-core --target wasm32-unknown-unknown || fail "carrier-core does not build for wasm32"
  echo "   carrier-core builds for wasm32-unknown-unknown"
  # The same functions called from JavaScript through wasm must match the native results.
  if command -v node >/dev/null && [ -x "$HOME/.cargo/bin/wasm-bindgen" ]; then
    cargo build --release --quiet -p carrier-wasm --target wasm32-unknown-unknown || fail "carrier-wasm build"
    rm -rf out/wasmpkg
    "$HOME/.cargo/bin/wasm-bindgen" --target nodejs --out-dir out/wasmpkg target/wasm32-unknown-unknown/release/carrier_wasm.wasm || fail "wasm-bindgen"
    cargo run --release --quiet --example coreref > out/coreref.json || fail "coreref"
    node crates/carrier-wasm/parity.cjs out/wasmpkg out/coreref.json >/dev/null || fail "wasm results differ from native"
    echo "   wasm (JavaScript) and native results match"
    # The PCZT sale path (carrier-sale) needs a wasm-capable C compiler for secp256k1-sys.
    WASI_SDK=${WASI_SDK:-$HOME/.local/wasi-sdk}
    if [ -x "$WASI_SDK/bin/clang" ]; then
      CC_wasm32_unknown_unknown="$WASI_SDK/bin/clang" AR_wasm32_unknown_unknown="$WASI_SDK/bin/llvm-ar" \
        cargo build --release --quiet -p carrier-wasm --features sale --target wasm32-unknown-unknown || fail "carrier-wasm sale build"
      rm -rf out/wasmsale
      "$HOME/.cargo/bin/wasm-bindgen" --target nodejs --out-dir out/wasmsale target/wasm32-unknown-unknown/release/carrier_wasm.wasm || fail "wasm-bindgen (sale)"
      node crates/carrier-wasm/sale_selftest.cjs out/wasmsale > out/wasm_sale_selftest.log || fail "wasm sale selftest"
      echo "   sale self-test in wasm: $(tail -1 out/wasm_sale_selftest.log)"
      # The browser verifier (wasm) over grpc-web must reach the same verdicts as this script.
      node crates/carrier-wasm/browser_verify.mjs out/wasmsale "${GRPC_WEB:-https://zcash-testnet.chainsafe.dev}" > out/wasm_browser_verify.log || { cat out/wasm_browser_verify.log; fail "browser verifier (wasm, grpc-web) disagrees"; }
      echo "   browser verifier (wasm over grpc-web): $(grep -c '^ok' out/wasm_browser_verify.log) cases agree"
    else
      echo "   wasi-sdk missing: wasm sale self-test skipped"
    fi
  else
    echo "   node or wasm-bindgen missing: wasm parity check skipped"
  fi
else
  echo "   wasm32-unknown-unknown target not installed: wasm build skipped"
fi
V=./target/release/verify

FUNDING=$(key acct0)   # funding; the real successor of forged item #2 is its change note
OLD=$(key acct3)       # vault that bought item #1 on 2026-09-29 and sold it on 2026-09-30
PAY=$(key acct5)       # seller pay key of the 2026-09-30 sale
BUYER=$(key acct6)     # vault that bought item #1 on 2026-09-30 and sold it on 2026-10-02
BUYER2=$(key3 buyer_acct6)   # separate buyer seed's vault, holds item #1 since 2026-10-02
P4_HOLDER=$(key3 buyer_acct7) # buyer seed's vault 17, holds sealed-collection item #1 since the P4 sale
if [ -f state/seed.hex ]; then
  keys=$(STATE=state OFF=10 ./target/release/carrier keys)
  for a in acct0 acct3 acct5 acct6; do
    [ "$(awk -v a="$a" '$1==a && $2=="fvk"{print $3}' <<<"$keys")" = "$(key "$a")" ] || fail "published $a key does not match state/"
  done
  echo "   published viewing keys match state/"
fi

echo "== 2026-09-29 package (item #1) with its former holder, plus stray mint #3"
# acct3 sold item #1 on 2026-09-30, so the chain is valid but its carrier is spent (exit 4).
run 4 env HOLDER_FVK="$OLD" $V "$E/collection.json" 4416690 "$E/package_item1.json" "$MINT3"
want "$out" "R1 issuance set: 2 item(s), finalized=true"
want "$out" "R1 REJECT claimed item #3"
want "$out" "R2 hop 0: spent in 45056894d3e1"
want "$out" "R2 hop 0: successor valid (HOP, value 0)"
want "$out" "R3 holder opens current carrier: yes, unspent=false"
want "$out" "VERIFY SPENT: item #1"

echo "== forged package (item #2), victim buyer acct3"
run 2 env HOLDER_FVK="$OLD" $V "$E/collection.json" 4416690 "$E/package_item2_forged.json"
want "$out" "current carrier cmx 53c2a3eb7677"
want "$out" "VERIFY FAIL: holder cannot open current carrier"

echo "== forged package (item #2), opened by the real successor's recipient (funding acct0)"
# OPEN_ISSUES #7: the real successor is a funding change note (value > 0, empty memo): burned.
run 3 env HOLDER_FVK="$FUNDING" $V "$E/collection.json" 4416690 "$E/package_item2_forged.json"
want "$out" "R2 hop 0: successor invalid (value 9920000 != 0)"
want "$out" "VERIFY BURNED: item #2 burned at hop 0"

echo "== 2026-09-30 sale package (item #1, hop 1), its holder acct6"
# acct6 sold item #1 again on 2026-10-02, so this chain is valid but the carrier is spent.
run 4 env HOLDER_FVK="$BUYER" $V "$E/collection.json" 4416690 "$E2/package_item1_sale.json"
want "$out" "R2 hop 0: successor valid (HOP, value 0)"
want "$out" "R2 hop 1: spent in ${SALE_TX:0:12}"
want "$out" "R2 hop 1: successor valid (HOP, value 0)"
want "$out" "R3 holder opens current carrier: yes, unspent=false"
want "$out" "VERIFY SPENT: item #1"

echo "== 2026-10-02 sale between separate seeds (item #1, hop 2), holder: buyer seed acct6"
# HELD while the buyer seed keeps item #1. If a later sale moves it, this becomes SPENT (exit 4).
run 0 env HOLDER_FVK="$BUYER2" $V "$E/collection.json" 4416690 "$E3/package_item1_hop3.json"
want "$out" "R2 hop 2: spent in ${SALE2_TX:0:12}"
want "$out" "R2 hop 2: successor valid (HOP, value 0)"
want "$out" "R3 holder opens current carrier: yes, unspent=true"
want "$out" "VERIFY HELD: item #1"
run 2 env HOLDER_FVK="$BUYER" $V "$E/collection.json" 4416690 "$E3/package_item1_hop3.json"
want "$out" "VERIFY FAIL: holder cannot open current carrier"
# Vault records (marketplace SPEC 4.2) started after this sale, so a record walk stops at once.
run 2 env HOLDER_FVK="$BUYER2" $V "$E/collection.json" 4416690 "walk:1:$SALE2_TX"
want "$out" "VERIFY FAIL: hop 3: no vault record"

echo "== FINAL hard seal (new collection, 2026-10-02)"
run 0 env -u HOLDER_FVK $V "$E3/collection_sealed.json" "$SEAL_GENESIS_H"
want "$out" "R1 issuance set: 2 item(s), finalized=true, sealed=true"

echo "== vault records (P4 sale over the disclosure channel): provenance from the chain alone"
# HELD while the buyer seed keeps the item; a later sale turns this into SPENT (exit 4).
run 0 env HOLDER_FVK="$P4_HOLDER" $V "$E3/collection_sealed.json" "$SEAL_GENESIS_H" "walk:1:$P4_TX"
want "$out" "record walk: hop 1 <- 7eab6bd6bee4..:0 (issuance)"
want "$out" "VERIFY HELD: item #1"
run 0 env HOLDER_FVK="$P4_HOLDER" $V "$E3/collection_sealed.json" "$SEAL_GENESIS_H" "$E3/package_sealed_item1_hop1.json"
want "$out" "VERIFY HELD: item #1"
run 2 env HOLDER_FVK="$(key3 sealed_acct2)" $V "$E3/collection_sealed.json" "$SEAL_GENESIS_H" "walk:1:$P4_TX"
want "$out" "VERIFY FAIL: the holder key opens no HOP carrier"
if [ -f state/buyer/seed.hex ]; then
  # Seed-only recovery from the account index record in the P4 change output.
  run 0 env STATE=state/buyer BIRTHDAY=4435100 ./target/release/carrier recover
  want "$out" "RECOVERED item #1 of collection 5fc17ef389fdae9b.. hop 1, vault index 17"
fi

echo "== a package without a holder key"
run 5 env -u HOLDER_FVK $V "$E/collection.json" 4416690 "$E3/package_item1_hop3.json"
want "$out" "VERIFY UNVERIFIED: item #1"

echo "== sale payments (one tx carries the item and the payment), from the published bytes"
# Offline: parse the published bytes, check their txid and the payment from them; then require the
# server's copy to be byte-identical and mined.
run 0 $V payment-file "$E2/sale_tx.hex" "$SALE_TX" "$PAY" 10000
want "$out" "PAYMENT 10000 zats to the given key in $E2/sale_tx.hex"
want "$out" "identical to the server copy"
run 0 $V payment-file "$E3/sale_tx.hex" "$SALE2_TX" "$PAY" 10000
want "$out" "PAYMENT 10000 zats to the given key in $E3/sale_tx.hex"
run 0 $V payment-file "$E3/sale_p4_tx.hex" "$P4_TX" "$(key3 sealed_acct5)" 10000
want "$out" "PAYMENT 10000 zats to the given key in $E3/sale_p4_tx.hex"

echo "== artwork rebuilt from chain bytes"
art=$(python3 scripts/render_from_chain.py 1 2)
echo "$art"
[ "$(grep -c 'identical_to_source=True' <<<"$art")" -eq 2 ] || fail "on-chain artwork differs from source"

echo "VERIFY.SH OK"
