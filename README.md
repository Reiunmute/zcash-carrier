# zcash-carrier

Note-bound NFTs on Zcash's shielded pool. A proof of concept, tested on the public testnet (Ironwood pool, NU6.3).

The ownership record is the chain itself, and the image lives in the note memo. There is no company database. The protocol is unchanged: it uses Zcash exactly as deployed.

> Status: research PoC. Testnet only. Not audited. Do not use with real funds.

## Idea in one paragraph

An NFT is a zero-value shielded note (a *carrier*). Its 512-byte memo holds the artwork: 26x26 pixels, 16 colours, 386 bytes. A transfer spends the carrier and creates the successor note **in the same Orchard-style Action**. Inside an Action, the circuit enforces `rho_new = nf_old`, so the successor is cryptographically chained to the note it replaces. Given the previous note and its viewing key, a buyer can find the unique legitimate successor. The node rejects a second spend of the same carrier through the nullifier set. Issuance is capped by a sequence chain of notes that ends in a `FINAL` note. Anyone holding the published issuer viewing key can audit that chain.

## What was proven on testnet (2026-09-29)

| Step | txid | Result |
|---|---|---|
| genesis: mint #1, open sequence | `82562648ae6421901993cedbb72ffa7cf1ed4977ec33ccd1dbb8f91985fc1ea5` | mined |
| final: mint #2, close sequence | `b193c439841d7e94cd19032ce5a45f153a5d4ee4969e285b5ca9dc80450cba30` | mined, binding found on build attempt 5 |
| transfer #1 to buyer vault | `45056894d3e1c6de389813effc972f9334bf4afe02d6842d4c824128c3614cce` | mined at 4416737 |
| attack: forged successor for #2 | `3cfcf04ed370a43eed6025e3a92860a95f5af5c21107e7f2c0441e949ce47140` | mined, **rejected by verifier** |
| attack: spend #1 carrier twice | (not broadcastable) | **rejected by node**: `ironwood double-spend: duplicate nullifier` |
| attack: mint #3 after FINAL | `78a9a662f825d576abdf14e4d47fe6244a68ecd0a54135fca76eca32001cc3fb` | mined, **rejected by verifier** |

Both images were rebuilt from bytes pulled out of on-chain memos. They are byte-identical to the source files. See `evidence/testnet-2026-09-29/`.

An earlier collection `c5d8637a...` was discarded because its issuance outputs had no issuer OVK. A public audit of that collection saw zero items. Setting the issuer OVK on every issuance output is mandatory in the spec.

## Layout

```
src/lib.rs            memo format, keys, lightwalletd client, tree witness, decryption helpers
src/bin/carrier.rs    issuer/holder tool: info, scan, genesis, final, transfer, attack-*
src/bin/verify.rs     independent verifier: uses only chain data + collection record + package
art/gen.py            generates the 26x26 16-colour test portraits
scripts/claim_jino.py testnet faucet claim (solves the faucet's sha256 PoW)
scripts/render_from_chain.py  rebuilds PNGs from verifier output
docs/SPEC.md          carrier format and verification rules
docs/OPEN_ISSUES.md   known limits and next tests
evidence/             public testnet artefacts (no spending keys)
```

## Run

```bash
cargo build --release
mkdir -p state out && chmod 700 state
export STATE=state OFF=10          # OFF shifts role accounts to a fresh key range
./target/release/carrier info      # prints role addresses; fund acct0 from a testnet faucet
./target/release/carrier scan <from_height>
./target/release/carrier genesis   # then: final, transfer, attack-forge, attack-double, attack-mint
./target/release/verify evidence/testnet-2026-09-29/collection.json 4416690 \
    evidence/testnet-2026-09-29/package_item1.json <extra txids to check>
```

`LWD` overrides the lightwalletd endpoint (default `https://testnet.zec.rocks:443`). `DRY=1` builds and proves without broadcasting.

## Privacy notes

- `evidence/*/package_*.json` contain a spent carrier note and its vault full viewing key. That disclosure is intentional: it is the provenance a seller gives a buyer. It lets the buyer see that vault's history, so every NFT uses a dedicated vault key.
- The issuer full viewing key is public by design (collection audit).
- Spending keys and the test seed never leave `state/`, which is git-ignored.
