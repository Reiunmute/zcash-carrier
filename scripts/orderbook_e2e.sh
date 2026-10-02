#!/usr/bin/env bash
# DRY end-to-end sale through the order book (marketplace SPEC 3.1, 3.2). Buyer and seller each
# have their own exchange directory and see each other only through the server. Nothing is
# broadcast: the seller's accept runs with DRY=1.
#
# Needs a running order book (cargo build --release -p orderbook; ./target/release/orderbook)
# at ORDERBOOK (default http://127.0.0.1:8787). Roles follow the holdings after the P4 sale: the
# seller is the buyer seed's vault 17 (sealed-collection item #1, hop 1), the buyer is the main
# seed (OFF=30, new vault acct6 = index 36). The disclosure carries no package (RECORDS_ONLY=1),
# so the buyer verifies from the vault record alone.
# Seller files: state/buyer/c2/{collection.json (public record), package_item1_hop1.json}.
set -euo pipefail
cd "$(dirname "$0")/.."
OB=${ORDERBOOK:-http://127.0.0.1:8787}
C=./target/release/carrier
FUND=512832b969c897752116be1649d5a982ad2a82e34199bc1d1e9f11bfef9b68f3   # main seed acct0
WORK=${WORK:-$(mktemp -d)}
XS=$WORK/seller XB=$WORK/buyer
mkdir -p "$XS" "$XB"
seller() { env STATE=state/buyer OFF=10 FILES_DIR=state/buyer/c2 SALE_DIR=$XS SELLER_ACCT=7 PAY_ACCT=5 "$@"; }
buyer()  { env STATE=state OFF=30 SALE_DIR=$XB "$@"; }
# ob <method> <path> [json-body-file]: prints the response body, fails on HTTP >= 400
ob() {
  python3 - "$OB" "$@" <<'PY'
import json, sys, urllib.request, urllib.error
base, method, path = sys.argv[1:4]
data = open(sys.argv[4], 'rb').read() if len(sys.argv) > 4 else None
req = urllib.request.Request(base + path, data=data, method=method, headers={'content-type': 'application/json'})
try:
    with urllib.request.urlopen(req) as r:
        sys.stdout.write(r.read().decode())
except urllib.error.HTTPError as e:
    sys.stderr.write(f"HTTP {e.code} {method} {path}: {e.read().decode()}\n"); sys.exit(1)
PY
}
py() { python3 -c "$@"; }

echo "== seller lists; posts the public listing with a delete token hash"
seller PACKAGE=package_item1_hop1.json $C sale-list | tail -1
head -c 32 /dev/urandom | xxd -p -c 64 > "$XS/delete_token"
py 'import json,sys,hashlib
l=json.load(open(sys.argv[1])); tok=bytes.fromhex(open(sys.argv[2]).read().strip())
l["delete_token_hash"]=hashlib.blake2b(tok,digest_size=32,person=b"CarrierListDel01").hexdigest()
json.dump(l,open(sys.argv[3],"w"))' "$XS/listing.json" "$XS/delete_token" "$XS/post.json"
ob POST /listings "$XS/post.json"
ID=$(py 'import json,sys; print(json.load(open(sys.argv[1]))["listing_id"])' "$XS/listing.json")
echo "listed $ID"

echo "== buyer reads the listing from the order book and posts an intent"
ob GET "/listings/$ID" > "$XB/listing.json"
buyer $C sale-intent | tail -1
T=$(ob POST "/listings/$ID/intents" "$XB/intent.json" | py 'import json,sys; print(json.load(sys.stdin)["transcript"])')
echo "server transcript ${T:0:12}.."

echo "== seller reads the intent, grants the lease, posts the disclosure"
ob GET "/listings/$ID/intents" | py 'import json,sys; i=json.load(sys.stdin)[0]; t=i.pop("transcript"); i["listing_id"]=sys.argv[1]; json.dump(i,open(sys.argv[2],"w")); print("intent transcript", t[:12]+"..")' "$ID" "$XS/intent.json"
seller RECORDS_ONLY=1 $C sale-disclose | tail -1
msg() { py 'import json,sys; json.dump({"transcript":sys.argv[1],"kind":int(sys.argv[2]),"direction":int(sys.argv[3]),"seq":0,"body":open(sys.argv[4],"rb").read().hex()},open(sys.argv[5],"w"))' "$@"; }
msg "$T" 2 1 "$XS/disclosure.bin" "$XS/m2.json"
ob POST /messages "$XS/m2.json"

echo "== buyer fetches the disclosure, verifies, builds, posts the encrypted PCZT"
ob GET "/messages?transcript=$T&kind=2&direction=1" | py 'import json,sys; m=json.load(sys.stdin); open(sys.argv[1],"wb").write(bytes.fromhex(m[0]["body"]))' "$XB/disclosure.bin"
buyer FUND_TX=$FUND BUYER_VAULT_ACCT=6 COLLECTION=evidence/testnet-2026-10-02/collection_sealed.json $C sale-buy | grep -E 'disclosure opened|record walk|anchor at|verified from chain|SALE-BUY'
msg "$T" 3 0 "$XB/sale.pczt" "$XB/m3.json"
ob POST /messages "$XB/m3.json"

echo "== seller fetches the PCZT and accepts (DRY)"
ob GET "/messages?transcript=$T&kind=3&direction=0" | py 'import json,sys; m=json.load(sys.stdin); open(sys.argv[1],"wb").write(bytes.fromhex(m[0]["body"]))' "$XS/sale.pczt"
seller DRY=1 $C sale-accept | tail -2

echo "== seller removes the listing (no public sold event)"
ob DELETE "/listings/$ID?token=$(cat "$XS/delete_token")"
ob GET "/listings/$ID" 2>&1 | tail -1 || true
echo "ORDERBOOK E2E OK"
