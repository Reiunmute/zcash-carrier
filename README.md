# zcash-carrier

Collectibles bound to shielded notes on Zcash. This repo contains a working testnet proof of concept and the plan for a public marketplace.

> Status: research PoC. Testnet only. Not audited. Do not use real funds.
> Working name: **Carrier**. The product name is still open. These are not ERC-721 tokens, so the docs avoid calling them NFTs.

**Why this exists, in plain words:** [`docs/VISION.md`](docs/VISION.md) ([한국어](docs/VISION.ko.md)).

**New here?** Read [`docs/OVERVIEW.md`](docs/OVERVIEW.md) ([한국어](docs/OVERVIEW.ko.md)): goal, method, progress, and how this differs from zkSNARKs and Bitcoin Ordinals.

**Continuing this work with a coding agent?** Read [`docs/HANDOFF.md`](docs/HANDOFF.md) first. Claude Code also loads [`CLAUDE.md`](CLAUDE.md) automatically.

## Why this exists

zkSNARKs (the Zcash identity collection sold on Zilkroad) keeps ownership in the marketplace's database. The shielded payment is real cryptography. The title transfer, however, is a database write by the operator ([Onchain Diary analysis](https://paragraph.com/@onchaindiary/zksnarks-auction-what-bidders-bought-1)). Zcash Shielded Assets (ZSA), the native asset standard, is not on mainnet.

This project achieves the same goal with Zcash exactly as deployed today:
- **Ownership** is a shielded note on chain. No company database is involved.
- **Every transfer** is enforced by the zk proof inside Zcash's Action circuit.
- **The artwork** lives in the note memo.

## How it works

1. **Carrier.** One item is one zero-value shielded note in the Ironwood pool. Its 512-byte memo holds the artwork: 26x26 pixels, 16 colours, 386 bytes.
2. **Same-Action binding.** A transfer spends the carrier and creates its successor in the **same Action**. The circuit enforces `rho_new = nf_old` (orchard 0.15.5 `circuit.rs:801`), so the successor is cryptographically chained to the note it replaces. Given the previous note and its viewing key, anyone can find the one legitimate successor.
3. **Double spends** are rejected by the Zcash node through the nullifier set.
4. **Supply cap.** The issuer keeps a chain of sequence notes, also bound Action to Action, that ends in a `FINAL` note. The issuer viewing key is public, so anyone can count the issuance set. Anything minted after `FINAL` is invalid.
5. **Independent verifier.** `verify` checks everything from public chain data, the public collection record, and the provenance package the seller hands the buyer. No site and no database are required.

Details: [`docs/SPEC.md`](docs/SPEC.md).

## What was proven on testnet (2026-09-29, Ironwood, NU6.3)

Collection `c7fe2aea…` (full record in `evidence/testnet-2026-09-29/collection.json`)

| Step | txid | Result |
|---|---|---|
| faucet funding, 0.1 TAZ | `56bc76a2ce96…` | mined at 4416694 |
| genesis: mint #1, open sequence | `82562648ae6421901993cedbb72ffa7cf1ed4977ec33ccd1dbb8f91985fc1ea5` | mined |
| final: mint #2, close sequence | `b193c439841d7e94cd19032ce5a45f153a5d4ee4969e285b5ca9dc80450cba30` | mined. The builder shuffle needed 5 builds to land the binding. |
| transfer #1 to buyer vault | `45056894d3e1c6de389813effc972f9334bf4afe02d6842d4c824128c3614cce` | mined at 4416737 |
| attack: forged successor for #2 | `3cfcf04ed370a43eed6025e3a92860a95f5af5c21107e7f2c0441e949ce47140` | mined. **The verifier rejects it.** |
| attack: spend the #1 carrier twice | not broadcastable | **The node rejects it**: `ironwood double-spend: duplicate nullifier` |
| attack: mint #3 after FINAL | `78a9a662f825d576abdf14e4d47fe6244a68ecd0a54135fca76eca32001cc3fb` | mined. **The verifier rejects it.** |

Both images were rebuilt from bytes pulled out of on-chain memos and are byte-identical to the source files.

![on-chain items](evidence/testnet-2026-09-29/onchain_items.png)

An earlier collection `c5d8637a…` was discarded. Its issuance outputs carried no issuer OVK, so a public audit saw zero items. Every issuance output now uses the issuer OVK.

Re-check all of this at any time with `bash verify.sh`. It takes a few seconds and needs network access.

## Marketplace plan

The goal is an OpenSea-style public website: connect a wallet, see your items, list, and buy in one atomic transaction.

**Stage 1 feasibility was measured on 2026-09-30** ([`research/stage1-checks/RESULTS.md`](research/stage1-checks/RESULTS.md)):

| Question | Answer |
|---|---|
| Can a wallet signature seed a stable collectibles key? | Yes. Noir Wallet 1.0.37 `signMessage` in `derived` mode gave the same signature 3/3 times. Risk: the signature is not bound to the site's origin. |
| Can a browser prove Ironwood Actions fast enough? | Yes. Multithreaded WASM on an M4 Pro: 1.9 s for 2 Actions, 3.6 s for 4 Actions. Single-thread: 11.9 s and 21.8 s. |
| Can a browser reach testnet lightwalletd? | `zec.rocks` has no CORS. ChainSafe grpc-web works, and so does a small relay of our own. |

The plan is [`docs/marketplace/SPEC.md`](docs/marketplace/SPEC.md) (draft 1, **not yet approved**). Stage 1 needs no wallet integration: keys and proofs stay in the user's browser, and the server keeps only a public order book. Stages 2 and 3 (Vizor support, Zcash WalletConnect) come later.

## Layout

```
src/lib.rs                  memo format, keys, lightwalletd client, tree witness, decryption
src/bin/carrier.rs          issuer/holder CLI: info, scan, genesis, final, transfer, attack-*, keys
src/bin/verify.rs           independent verifier (public data only)
verify.sh                   repo gate: build + re-verify testnet evidence + rebuild art
art/gen.py                  generates the 26x26 16-colour test portraits (art*.bin)
scripts/claim_jino.py       testnet faucet claim (solves the faucet's sha256 PoW)
scripts/render_from_chain.py  rebuilds PNGs from verifier output, compares with source
docs/SPEC.md                carrier format and verification rules R1 to R3
docs/OPEN_ISSUES.md         known limits and the next tests (#7 is a verifier gap)
docs/marketplace/SPEC.md    marketplace plan, draft 1
docs/HANDOFF.md             state, decisions, next tasks for the next agent
research/stage1-checks/     browser feasibility scripts and results
evidence/                   public testnet artifacts (no spending keys)
state/                      local keys and working files, git-ignored, never commit
```

## Run

```bash
cargo build --release
bash verify.sh                       # gate: must print VERIFY.SH OK

# Driving the CLI. It needs a testnet key in state/. The first run creates state/seed.hex.
mkdir -p state out && chmod 700 state
export STATE=state OFF=10            # OFF shifts role accounts 1-4 to a fresh key range. acct0 = funding.
./target/release/carrier info        # role addresses. Fund acct0 from a faucet.
./target/release/carrier scan <from_height>
./target/release/carrier genesis     # then: final, transfer, attack-forge, attack-double, attack-mint
```

Environment variables:
- `LWD`: override the lightwalletd endpoint (default `https://testnet.zec.rocks:443`).
- `DRY=1`: build and prove without broadcasting.
- `FUND_TX`: pick a specific funding tx.
- `HOLDER_FVK`: enable the R3 holder check in `verify`.

## Privacy notes

- `evidence/*/package_*.json` hold a spent carrier note and its vault full viewing key. This is intentional: it is the provenance a seller gives a buyer. It exposes that vault's history, which is why each item uses its own vault key.
- The issuer full viewing key is public by design, for collection audits.
- Spending keys and the test seed stay in `state/`, which is git-ignored.
