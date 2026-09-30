"""Claim testnet TAZ from zcashfaucet.jinolabs.xyz (solves its sha256 PoW like the browser worker)."""
import hashlib, json, sys, time, urllib.request

BASE = "https://zcashfaucet.jinolabs.xyz"
addr = sys.argv[1]

def get(p):
    return json.load(urllib.request.urlopen(BASE + p, timeout=30))

def lz_bits(d):
    n = 0
    for b in d:
        if b == 0:
            n += 8
            continue
        n += 8 - b.bit_length()
        break
    return n

ch = get("/api/pow/challenge")
assert ch.get("ok"), ch
seed, diff = ch["seed"], ch["difficulty"]
t = time.time()
nonce = 0
while lz_bits(hashlib.sha256(f"{seed}:{nonce}".encode()).digest()) < diff:
    nonce += 1
print(f"pow solved diff={diff} nonce={nonce} in {time.time()-t:.1f}s")
body = json.dumps({"address": addr, "network": "taz",
                   "pow": {"seed": seed, "difficulty": diff, "exp": ch["exp"], "sig": ch["sig"], "nonce": str(nonce)}}).encode()
req = urllib.request.Request(BASE + "/api/faucet", data=body, headers={"content-type": "application/json"}, method="POST")
try:
    r = urllib.request.urlopen(req, timeout=90)
    print(r.status, r.read().decode()[:600])
except urllib.error.HTTPError as e:
    print(e.code, e.read().decode()[:600])
