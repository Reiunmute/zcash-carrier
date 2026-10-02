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
MINT3=78a9a662f825d576abdf14e4d47fe6244a68ecd0a54135fca76eca32001cc3fb
SALE_TX=6bdeb823ff6556dce7ef8c254c8ceaa257913cc820c43da271c52dabdc98ba5b
fail() { echo "VERIFY.SH FAIL: $*" >&2; exit 1; }
want() { grep -qF -- "$2" <<<"$1" || fail "expected output missing: $2"; }
key() { python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))[sys.argv[2]]["fvk"])' "$E2/viewing_keys.json" "$1"; }
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
V=./target/release/verify

FUNDING=$(key acct0)   # funding; the real successor of forged item #2 is its change note
OLD=$(key acct3)       # vault that bought item #1 on 2026-09-29 and sold it on 2026-09-30
PAY=$(key acct5)       # seller pay key of the 2026-09-30 sale
BUYER=$(key acct6)     # vault that bought item #1 on 2026-09-30
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

echo "== 2026-09-30 sale package (item #1, hop 1), holder acct6"
# HELD while acct6 keeps item #1. If a later test moves it, this becomes SPENT (exit 4).
run 0 env HOLDER_FVK="$BUYER" $V "$E/collection.json" 4416690 "$E2/package_item1_sale.json"
want "$out" "R2 hop 0: successor valid (HOP, value 0)"
want "$out" "R2 hop 1: spent in ${SALE_TX:0:12}"
want "$out" "R2 hop 1: successor valid (HOP, value 0)"
want "$out" "R3 holder opens current carrier: yes, unspent=true"
want "$out" "VERIFY HELD: item #1"

echo "== same package without a holder key"
run 5 $V "$E/collection.json" 4416690 "$E2/package_item1_sale.json"
want "$out" "VERIFY UNVERIFIED: item #1"

echo "== 2026-09-30 sale payment (one tx carries the item and the payment)"
run 0 $V payment "$SALE_TX" "$PAY" 10000
want "$out" "PAYMENT 10000 zats"
[ "$(tr -d '\n' < "$E2/sale_tx.hex" | wc -c | tr -d ' ')" -eq 24644 ] || fail "sale_tx.hex size changed"

echo "== artwork rebuilt from chain bytes"
art=$(python3 scripts/render_from_chain.py 1 2)
echo "$art"
[ "$(grep -c 'identical_to_source=True' <<<"$art")" -eq 2 ] || fail "on-chain artwork differs from source"

echo "VERIFY.SH OK"
