#!/usr/bin/env bash
# Broadcast runbook for marketplace SPEC P4 (vault records and account index records). Testnet only.
# Needs Ryot's approval in the session before running (CLAUDE.md rule 2).
#
# Sends ONE transaction:
#   D  sale of item #1 of the sealed collection 5fc17ef389fdae9b
#      from the main seed's vault (OFF=30 acct2, index 32) to the buyer seed's fresh vault
#      (acct7, index 17), 10,000 zats to a per-listing address of OFF=30 acct5 (index 35).
#      The buyer seed pays: funding note 275,000 zats in 281788d7..., fee 15,000.
#   The successor's memo carries the vault record (SPEC 4.2, pointer kind issuance) and the
#   buyer's change memo the account index record (SPEC 4.3, vault index 17).
#
# Then checks, from chain data only:
#   the record walk rebuilds item #1's provenance to issuance and the buyer's vault holds it (HELD),
#   the former holder cannot open it, the payment, and seed-only recovery on the buyer seed.
#
# Usage:  bash scripts/run_p4.sh          (PLAN=1 prints the plan and stops)
# Output: state/run-p4/ (git-ignored). Public artifacts are copied to evidence/ afterwards.
set -euo pipefail
cd "$(dirname "$0")/.."

C=./target/release/carrier
V=./target/release/verify
FUND=281788d78f3e8ab39b208373c9810d0e0383c2cf54beb1b342dfe5bb03c4bf98   # buyer seed acct0 change, 275,000 zats
LOG=state/run-p4
X=state/exchange-p4
mkdir -p "$LOG" "$X"
[ -f state/c2/package_item1.json ] || echo '{"item":1,"label":"item #1 at issuance","hops":[]}' > state/c2/package_item1.json

seller() { env STATE=state OFF=30 FILES_DIR=state/c2 SALE_DIR=$X SELLER_ACCT=2 PAY_ACCT=5 "$@"; }
buyer()  { env STATE=state/buyer OFF=10 SALE_DIR=$X "$@"; }
step()   { echo; echo "=== $*"; }
wait_mined() {
  echo "   waiting for $1 to be mined"
  for _ in $(seq 1 80); do
    if $V mined "$1" >/dev/null 2>&1; then echo "   mined: $($V mined "$1")"; return 0; fi
    sleep 30
  done
  echo "timed out waiting for $1"; exit 1
}
# check <log> <expected exit> <cmd...>: runs, logs output and exit code, and reports a mismatch.
check() {
  local log=$1 want=$2; shift 2
  set +e; "$@" > "$LOG/$log" 2>&1; local code=$?; set -e
  echo "exit $code" >> "$LOG/$log"
  tail -2 "$LOG/$log"
  [ "$code" -eq "$want" ] || echo "   !! expected exit $want"
}

cargo build --release --quiet
B7=$(buyer $C keys | awk '$1=="acct7"{print $3}')
S2=$(seller $C keys | awk '$1=="acct2"{print $3}')
S5=$(seller $C keys | awk '$1=="acct5"{print $3}')

step "plan"
echo "D  sale of sealed-collection item #1: OFF=30 acct2 ${S2:0:16}.. -> buyer seed acct7 ${B7:0:16}.., price 10000 to OFF=30 acct5"
echo "   funding $FUND (buyer seed acct0)"
[ -n "${PLAN:-}" ] && exit 0

step "D  sale with records (list and buy do not broadcast; accept does)"
seller PACKAGE=package_item1.json $C sale-list | tee "$LOG/D_list.log"
buyer FUND_TX=$FUND BUYER_VAULT_ACCT=7 $C sale-buy | tee "$LOG/D_buy.log"
seller $C sale-accept | tee "$LOG/D_accept.log"
TXD=$(awk '$1=="SALE-ACCEPT"{print $2; exit}' "$LOG/D_accept.log")
wait_mined "$TXD"

step "verify"
GH=4435127   # genesis height of the sealed collection
check V_walk.log 0 env HOLDER_FVK="$B7" $V state/c2/collection.json $GH "walk:1:$TXD"
check V_package.log 0 env HOLDER_FVK="$B7" $V state/c2/collection.json $GH state/c2/package_item1_hop1.json
check V_oldholder.log 2 env HOLDER_FVK="$S2" $V state/c2/collection.json $GH "walk:1:$TXD"
check V_payment.log 0 $V payment "$TXD" "$S5" 10000
check R_recover.log 0 env STATE=state/buyer BIRTHDAY=4435100 $C recover

step "done"
echo "D  $TXD" | tee "$LOG/txids.txt"
