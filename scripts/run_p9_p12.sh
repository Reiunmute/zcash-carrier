#!/usr/bin/env bash
# Broadcast runbook, 2026-10-02. Testnet only. Approved by Ryot in session ("위 3개 전부 진행하도록").
#   P12  sale of item #1 between two separately held seeds (state/ and state/buyer/)
#   P9   FINAL hard seal on a new test collection (OFF=30, files in state/c2)
#
# Sends four transactions from this machine's testnet keys:
#   A   send 300,000 zats: main seed acct0 -> buyer seed acct0                          fee 10,000
#   B   sale: item #1 from main acct6 to buyer-seed acct6, 10,000 zats to main acct5     fee 15,000 (buyer seed)
#   C1  genesis of a new test collection (issuer acct31, item #1 to vault acct32)       fee 15,000
#   C2  final with SEAL=1: FINAL to the collection's seal address, item #2 to acct34    fee 15,000
#
# Usage:  bash scripts/run_p9_p12.sh          (PLAN=1 prints the plan and stops)
# Output: state/run-2026-10-02/ (git-ignored). Public artifacts are copied to evidence/ afterwards.
set -euo pipefail
cd "$(dirname "$0")/.."

C=./target/release/carrier
V=./target/release/verify
FUND=6bdeb823ff6556dce7ef8c254c8ceaa257913cc820c43da271c52dabdc98ba5b   # main acct0 change, 9,885,000 zats
LOG=state/run-2026-10-02
X=state/exchange                                                         # files passed between buyer and seller
mkdir -p "$LOG" "$X"

main()  { env STATE=state OFF=10 "$@"; }
buyer() { env STATE=state/buyer OFF=10 "$@"; }
c2()    { env STATE=state OFF=30 FILES_DIR=state/c2 "$@"; }
step()  { echo; echo "=== $*"; }
wait_mined() {
  echo "   waiting for $1 to be mined"
  for _ in $(seq 1 80); do
    if $V mined "$1" >/dev/null 2>&1; then echo "   mined: $($V mined "$1")"; return 0; fi
    sleep 30
  done
  echo "timed out waiting for $1"; exit 1
}
grab() { awk -v k="$1" '$1==k{print $2; exit}'; }

cargo build --release --quiet
BUYER0=$(buyer $C keys | awk '$1=="acct0"{print $3}')
BUYER6=$(buyer $C keys | awk '$1=="acct6"{print $3}')
MAIN5=$(main $C keys | awk '$1=="acct5"{print $3}')

step "plan"
echo "A  send 300000 zats to buyer seed acct0 ${BUYER0:0:16}..  (funding $FUND)"
echo "B  sale of item #1: main acct6 -> buyer seed acct6 ${BUYER6:0:16}.., price 10000 to main acct5"
echo "C1 genesis, new collection, OFF=30"
echo "C2 final with SEAL=1"
[ -n "${PLAN:-}" ] && exit 0

step "A  send"
main FUND_TX=$FUND $C send "$BUYER0" 300000 | tee "$LOG/A_send.log"
TXA=$(grab SEND < "$LOG/A_send.log")
wait_mined "$TXA"

step "B  sale between separate seeds (list and buy do not broadcast; accept does)"
main SALE_DIR=$X SELLER_ACCT=6 PAY_ACCT=5 PACKAGE=package_item1_sale.json $C sale-list | tee "$LOG/B_list.log"
buyer SALE_DIR=$X FUND_TX=$TXA BUYER_VAULT_ACCT=6 $C sale-buy | tee "$LOG/B_buy.log"
main SALE_DIR=$X SELLER_ACCT=6 PAY_ACCT=5 $C sale-accept | tee "$LOG/B_accept.log"
TXB=$(grab SALE-ACCEPT < "$LOG/B_accept.log")

step "C1 genesis (new collection)"
c2 FUND_TX=$TXA $C genesis | tee "$LOG/C1_genesis.log"
TXC1=$(grab GENESIS < "$LOG/C1_genesis.log")
wait_mined "$TXC1"

step "C2 final, sealed"
c2 SEAL=1 $C final | tee "$LOG/C2_final.log"
TXC2=$(grab FINAL < "$LOG/C2_final.log")

wait_mined "$TXB"
wait_mined "$TXC2"

step "verify"
E=evidence/testnet-2026-09-29
set +e
env HOLDER_FVK="$BUYER6" $V "$E/collection.json" 4416690 state/package_item1_hop3.json | tee "$LOG/V_holder.log"; echo "exit $?" | tee -a "$LOG/V_holder.log"
env HOLDER_FVK="$(main $C keys | awk '$1=="acct6"{print $3}')" $V "$E/collection.json" 4416690 state/package_item1_hop3.json | tee "$LOG/V_oldholder.log"; echo "exit $?" | tee -a "$LOG/V_oldholder.log"
$V payment "$TXB" "$MAIN5" 10000 | tee "$LOG/V_payment.log"
GH=$($V mined "$TXC1" | awk '{print $2}')
$V state/c2/collection.json "$GH" | tee "$LOG/V_sealed.log"
set -e

step "done"
printf 'A  %s\nB  %s\nC1 %s\nC2 %s\n' "$TXA" "$TXB" "$TXC1" "$TXC2" | tee "$LOG/txids.txt"
