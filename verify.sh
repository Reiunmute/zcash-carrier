#!/usr/bin/env bash
# Gate for this repo. Builds, re-verifies the 2026-09-29 testnet evidence from public chain data,
# and rebuilds both artworks from on-chain memo bytes. Needs network (testnet lightwalletd).
# R3 (holder) checks run only when state/seed.hex exists (Ryot's Mac). Elsewhere they are skipped.
set -euo pipefail
cd "$(dirname "$0")"

E=evidence/testnet-2026-09-29
E2=evidence/testnet-2026-09-30
SALE_TX=6bdeb823ff65
MINT3=78a9a662f825d576abdf14e4d47fe6244a68ecd0a54135fca76eca32001cc3fb
fail() { echo "VERIFY.SH FAIL: $*" >&2; exit 1; }
want() { grep -qF -- "$2" <<<"$1" || fail "expected output missing: $2"; }

mkdir -p out
echo "== build"
cargo build --release --quiet

HOLDER=""
FUNDING=""
BUYER=""
if [ -f state/seed.hex ]; then
  # acct3 at OFF=10 is the buyer vault of the 2026-09-29 run. acct0 (funding) is not shifted by OFF.
  keys=$(STATE=state OFF=10 ./target/release/carrier keys)
  HOLDER=$(awk '$1=="acct3"{print $3}' <<<"$keys")
  FUNDING=$(awk '$1=="acct0" && $2=="fvk"{print $3}' <<<"$keys")
  # acct6 at OFF=10 is the buyer's new vault of the 2026-09-30 atomic sale.
  BUYER=$(awk '$1=="acct6"{print $3}' <<<"$keys")
  [ -n "$HOLDER" ] || fail "could not derive buyer vault FVK from state/"
  [ -n "$FUNDING" ] || fail "could not derive funding FVK from state/"
  [ -n "$BUYER" ] || fail "could not derive sale buyer vault FVK from state/"
else
  echo "   state/seed.hex not found: R3 holder checks skipped"
fi

echo "== genuine package (item #1) + stray mint #3"
out=$(env ${HOLDER:+HOLDER_FVK=$HOLDER} ./target/release/verify "$E/collection.json" 4416690 "$E/package_item1.json" "$MINT3") \
  || { echo "$out"; fail "genuine package rejected"; }
echo "$out"
want "$out" "R1 issuance set: 2 item(s), finalized=true"
want "$out" "R1 REJECT claimed item #3"
want "$out" "R2 hop 0: spent in 45056894d3e1"
# acct3 sold item #1 in the 2026-09-30 atomic sale, so this carrier is now spent. Only "yes" is required.
if [ -n "$HOLDER" ]; then
  want "$out" "R2 hop 0: successor valid (HOP, value 0)"
  want "$out" "R3 holder opens current carrier: yes"
else
  want "$out" "R2 hop 0: successor validity unverified"
fi

echo "== forged package (item #2)"
set +e
out=$(env ${HOLDER:+HOLDER_FVK=$HOLDER} ./target/release/verify "$E/collection.json" 4416690 "$E/package_item2_forged.json")
code=$?
set -e
echo "$out"
want "$out" "current carrier cmx 53c2a3eb7677"
if [ -n "$HOLDER" ]; then
  # The victim buyer (acct3) cannot open the real successor, so it is rejected outright.
  [ "$code" -eq 2 ] || fail "forged package was not rejected (exit $code)"
  want "$out" "VERIFY FAIL: holder cannot open current carrier"

  echo "== forged package (item #2), opened by the real successor's recipient (funding acct0)"
  # OPEN_ISSUES #7: the real successor is a funding change note (value > 0, empty memo), so item #2 is burned.
  set +e
  out=$(HOLDER_FVK=$FUNDING ./target/release/verify "$E/collection.json" 4416690 "$E/package_item2_forged.json")
  code=$?
  set -e
  echo "$out"
  [ "$code" -eq 3 ] || fail "forged package not reported burned (exit $code)"
  want "$out" "R2 hop 0: successor invalid (value "
  want "$out" "VERIFY BURNED: item #2 burned at hop 0"
else
  want "$out" "R2 hop 0: successor validity unverified"
fi

echo "== atomic sale package (item #1, hop 1, PCZT, 2026-09-30)"
out=$(env ${BUYER:+HOLDER_FVK=$BUYER} ./target/release/verify "$E/collection.json" 4416690 "$E2/package_item1_sale.json") \
  || { echo "$out"; fail "sale package rejected"; }
echo "$out"
want "$out" "R2 hop 0: successor valid (HOP, value 0)"
want "$out" "R2 hop 1: spent in $SALE_TX"
if [ -n "$BUYER" ]; then
  want "$out" "R2 hop 1: successor valid (HOP, value 0)"
  want "$out" "R3 holder opens current carrier: yes"
else
  want "$out" "R2 hop 1: successor validity unverified"
fi

echo "== artwork rebuilt from chain bytes"
art=$(python3 scripts/render_from_chain.py 1 2)
echo "$art"
[ "$(grep -c 'identical_to_source=True' <<<"$art")" -eq 2 ] || fail "on-chain artwork differs from source"

echo "VERIFY.SH OK"
