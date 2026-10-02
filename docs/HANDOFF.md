# HANDOFF: state, decisions, next tasks

Written 2026-09-30 by Rei (Hermes agent) for the next coding agent, expected to be Claude Code.
Owner: Ryot (Ryan). Treat every claim below as a hypothesis until you re-check it. `bash verify.sh` re-checks the testnet claims in seconds.

## 0. First 10 minutes

```bash
cd ~/work/zcash-carrier-poc
git log --oneline | head        # expect the handoff commit on top of 4e72655
cargo build --release
bash verify.sh                  # must end with VERIFY.SH OK
```

Then read, in order:
1. `CLAUDE.md` (hard rules)
2. this file
3. `docs/SPEC.md`
4. `docs/OPEN_ISSUES.md`
5. `docs/marketplace/SPEC.md`
6. `research/stage1-checks/RESULTS.md`

## 1. Goal

zkSNARKs (the Zcash collection traded on Zilkroad) keeps ownership in the operator's database. Ryot wants a Zcash-native alternative that is more on-chain and closer to real zk than that:
- ownership is a shielded note on chain
- the link from each owner to the next is enforced by Zcash's Action circuit (successor validity is a verifier rule)
- the artwork is in the memo
- anyone can verify without trusting a website (today the verifier still trusts its lightwalletd server; see OPEN_ISSUES #5)

The end product is an OpenSea-style public website where people connect a wallet, see their items, and buy and sell them.

Naming: these are not NFTs in the ERC-721 sense. The working name is **Carrier**. Candidates:
- Heirloom (Rei's pick: the product is the lineage)
- Relic
- Noteprint
- Veil

Ryot has not decided. Do not rename anything until he does. (Vizor came up in the discussion as a wallet, by Chainapsis. It is an existing product name, not a naming candidate.)

## 2. Status matrix

| Area | State | Evidence |
|---|---|---|
| Carrier format + rules R1 to R3 | implemented, testnet-proven. Since 2026-10-02 in `src/verifier.rs`, verdicts HELD / SPENT / BURNED / UNVERIFIED / FAIL | `docs/SPEC.md`, `verify.sh` |
| Issue, transfer, supply cap (FINAL) | proven on testnet 2026-09-29 | README table, `evidence/` |
| Attacks: double spend, forged successor, mint after FINAL | all rejected (node / verifier / verifier) | README table |
| Artwork on chain | byte-identical rebuild | `scripts/render_from_chain.py` |
| Verifier successor-validity rule | done 2026-09-30 (OPEN_ISSUES #7 closed) | `verify.sh` |
| Atomic sale via PCZT | mined on testnet 2026-09-30 (CLI; one local seed, roles separated in code), and **between separately held seeds** on 2026-10-02 (tx `281788d78f3e…`, item #1 now held by the buyer seed's acct6). Hardened and DRY-tested 2026-10-02: buyer verifies first, full redaction, 9 negative checks incl. seller tamper | `evidence/testnet-2026-09-30/`, `evidence/testnet-2026-10-02/`, `verify.sh` |
| Codex review | first review 14 findings; re-review counted 3 fixed and 11 partly fixed. Code-level follow-ups done 2026-10-02; design-level items proposed as SPEC changes (section 15, not approved) | `docs/research/codex-review-2026-10-02.md`, `docs/research/codex-review-2026-10-02-rereview.md` |
| lightwalletd trust | one server. txid, hash-link and compact/full checks done; `LWD2` content cross-check implemented, no independent second server reachable yet; no proof-of-work validation | OPEN_ISSUES #5 |
| FINAL hard seal | done on testnet 2026-10-02: collection `5fc17ef389fdae9b`, FINAL tx `512832b969c8…`, R1 `sealed=true` | OPEN_ISSUES #2, `evidence/testnet-2026-10-02/` |
| P5 Action counts | measured 2026-10-02; the 3-Action sale shape is in SPEC draft 4 | `research/p5-action-counts-2026-10-02.md` |
| Vault and account index records (P4) | implemented 2026-10-02: `sale-buy` writes both records, `verify <collection> <from> walk:<item>:<txid>` rebuilds provenance from records, `carrier recover` finds owned items from the seed alone. DRY-tested only; the on-chain test is one broadcast (`scripts/run_p4.sh`, needs Ryot's approval) | `evidence/testnet-2026-10-02/p4_dry_records.log` |
| Core crates (P7) | `crates/carrier-core`: formats, keys, decryption, rules R1 to R3, vault signatures, collectibles seed. `crates/carrier-sale`: the PCZT sale (buyer build, prove, sign, redact; seller checks, sign, extract), used by the CLI. Both build for wasm32; `crates/carrier-wasm` exports them (`sale` feature), and `verify.sh` checks JavaScript results match native and runs the sale self-test in wasm (32.4 s, Node, 1 thread) | `crates/`, `research/stage1-checks/RESULTS.md` |
| Browser network (P6) | grpc-web streaming with Ironwood actions verified on ChainSafe, relay streams | `research/stage1-checks/RESULTS.md` |
| Browser feasibility (sign, prove, network) | measured 2026-09-30 | `research/stage1-checks/RESULTS.md` |
| Marketplace SPEC | draft 4 (draft 3 + X1 to X8), **approved by Ryot 2026-10-02**, default decisions D2 to D8 (section 10). Build follows prerequisites P4 to P12 | `docs/marketplace/SPEC.md` |
| Web app | not started | none |
| GitHub | `Reiunmute/zcash-carrier`, **private** | `git remote -v` |

## 3. Decisions already made (do not reopen without new evidence)

1. **The note-bound carrier is the right primitive today.**
   - ZSA (ZIP 226/227) has no mainnet date, so it is only a migration target. No consensus rule can carry a carrier into a native asset, so any migration would be issuer-driven (burn and reissue).
   - Transparent inscriptions clash with the shielded brand.
   - The only way a buyer can independently check a unique successor is the circuit-enforced `rho = nf_old`.
2. **Every issuance output uses the issuer OVK.** The v0 collection `c5d8637a…` was discarded for missing it.
3. **One vault key per item.** Provenance discloses a vault FVK. Per-item vaults keep that disclosure scoped to one item.
4. **Same-Action binding is done with our own builder loop.** The loop rebuilds until `BundleMetadata` puts the pair in one Action. It never trusts a seller-claimed commitment: R2 takes the `cmx` from the Action that revealed the nullifier.
5. **Ironwood pool only. Testnet only. Never sell testnet assets** (a ZecBit precedent). No transparent pool.
6. **Stage 1 marketplace = public, browser-only.** Keys and proofs stay in the user's browser. The server keeps only a public order book. An earlier local-companion design (SPEC draft 0) was rejected because the public could not use it.
7. **Existing wallets fund and receive payouts. They do not hold carriers.** Findings from the wallet survey on 2026-09-29/30 (`docs/marketplace/SPEC.md` history):
   - No Zcash wallet exposes viewing keys or memos to a dApp.
   - None can build bound transfers.
   - None can join a multi-party PCZT from a website.
   - WalletConnect for Zcash is funded (Noir team, approved 2026-08-31) but not shipped.
8. **Zcash Foundation or ECC endorsement is not needed.** Ryot agreed. Wallet-developer awareness still matters: generic wallets might spend zero-value notes as inputs. `zcash_client_backend` 0.24.0 `fees/common.rs:886` only says such notes "may" be excluded.
9. **Stages 2 and 3 are later and not required** for a working product. Stage 2: carrier support in Vizor. Stage 3: a carrier method set in Zcash WalletConnect.

## 4. Environment facts

- **Host:** Ryot's Mac mini, Apple M4 Pro, 14 cores. Repo at `~/work/zcash-carrier-poc`.
- **Toolchains:**
  - Rust stable 1.94 builds the repo.
  - `nightly` is installed with `rust-src` and `wasm32-unknown-unknown`. The multithreaded WASM bench needs it.
  - `wasm-bindgen-cli` 0.2.129 is in `~/.cargo/bin`.
- **Pinned crates** (see `Cargo.toml`):
  - `orchard 0.15.5`, `zcash_primitives 0.30.1`, `zcash_client_backend 0.24.0` (feature `lightwalletd-tonic`), `tonic 0.14`.
  - Sources are in `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.
- **Network:**
  - Testnet lightwalletd: `https://testnet.zec.rocks:443` (override with `LWD`).
  - Branch id `37a5165b` (NU6.3). Ironwood has been active since height 4,134,000. Blocks come about every 75 s.
- **Key roles** (`STATE=state OFF=10` for the 2026-09-29 run):

  | Account | Role | ZIP-32 index |
  |---|---|---|
  | acct0 | funding | index 0 |
  | acct1 | issuer | 1+OFF |
  | acct2 | vault of item #1 before the sale | 2+OFF |
  | acct3 | buyer vault in the 2026-09-29 transfer, seller in the 2026-09-30 sale | 3+OFF |
  | acct4 | vault of item #2 | 4+OFF |
  | acct5 | seller pay address in the sale | 5+OFF |
  | acct6 | buyer's new vault in the 2026-09-30 sale, seller in the 2026-10-02 sale | 6+OFF |
  | acct7 | buyer vault for DRY sale tests from 2026-10-02 (holds nothing) | 7+OFF |

  The FVKs of acct0, acct3, acct5 and acct6 are published in `evidence/testnet-2026-09-30/viewing_keys.json` so `verify.sh` runs fully without the seed.

  Separate buyer seed (`STATE=state/buyer OFF=10`, created 2026-10-02): acct0 funding, acct6 holds item #1 since the 2026-10-02 sale. Its FVKs are in `evidence/testnet-2026-10-02/viewing_keys.json`.

  Sealed test collection `5fc17ef389fdae9b` (`STATE=state OFF=30 FILES_DIR=state/c2`): issuer acct1 (index 31), item #1 vault acct2 (index 32), item #2 vault acct4 (index 34). Funding is still the main seed's acct0.

  `carrier keys` prints the FVKs. FVKs are fine to show. Never print spending keys.
- **Funds:** faucet 10,000,000 zats. Checked 2026-10-02 against nullifiers up to height 4435339 (scratch read-only scan):
  - main seed acct0: 9,545,000 zats, action 2 of `512832b9…` (C2 change). Use `FUND_TX=512832b969c897752116be1649d5a982ad2a82e34199bc1d1e9f11bfef9b68f3`.
  - buyer seed acct0: 275,000 zats, action 2 of `281788d7…` (sale change). Use `FUND_TX=281788d78f3e8ab39b208373c9810d0e0383c2cf54beb1b342dfe5bb03c4bf98` with `STATE=state/buyer`.
  - main seed acct5: 20,000 zats, the two 10,000 zat sale payments (`6bdeb823…`, `281788d7…`).
  - Fees paid on 2026-10-02: 10,000 (A), 15,000 each (B, C1, C2).
- **Faucets:**
  - `scripts/claim_jino.py <addr>` (jinolabs, sha256 PoW, 0.1 TAZ).
  - The Valar faucet has a global daily cap and returned 429.
- **Speed notes:**
  - Proving takes about 2.1 s per 2-Action bundle natively (single thread).
  - Same-Action binding succeeds with probability 1/2 per build for a 2-Action bundle. `MAX_TRIES = 64`.
- **GitHub:**
  - A global pre-push hook (`~/.git-hooks/pre-push`) blocks pushes unless `ALLOW_GITHUB_PUSH=1`. Push only when Ryot asks.
  - SSH auth is user `Reiunmute`.
- **Headless browser for proofs:**
  - Playwright is not installed in this repo.
  - The research scripts expect `PW_BASE` (a directory with `node_modules/playwright`) and `CHROME` (a Chromium executable) as env vars.
  - On this Mac, Chromium builds are in `~/Library/Caches/ms-playwright/`. The scripts were run from an existing project that has Playwright 1.59.1.

## 5. Next tasks, in order

> 2026-10-02: Tasks 1 to 4 are done, task 5 is done except the PCZT path. From here the work follows `docs/marketplace/SPEC.md` draft 4 section 9: open prerequisites are P4 (on-chain test), P5 (anchor age), P8, P10 and P11.

Each task lists its exit criteria. Do not start the web app (task 6) before Ryot approves the marketplace SPEC.

### Task 1. Close OPEN_ISSUES #7 (successor validity)

Rule: a successor is valid only if `value == 0` and its memo decodes to `HOP` with the same `coll` and item number. Otherwise the item is **burned**, and later hops are ignored.

Notes:
- The successor can be opened only by its recipient. So R3 (holder) enforces it for the buyer's own note. A third party needs the next vault FVK.
- In the forged-package evidence, the real successor of item #2 is a funding change note (value > 0, empty memo). After this fix the verifier should report item #2 as burned.
- Exit: `verify.sh` updated for the new verdict and green. The genuine package still passes. Add a line to OPEN_ISSUES #7 marking it closed.

### Task 2. CLI two-party atomic sale via PCZT (the biggest open risk)

Build `carrier sale-*` subcommands that run the protocol in `docs/marketplace/SPEC.md` section 3 between two local key sets.

Crate: `pczt 0.9.3`. It depends on `orchard 0.15`, `zcash_primitives 0.30`, and `zcash_protocol 0.10.4`, so the versions match ours. Every role module has Ironwood code, including `Redactor::redact_ironwood_with(...).clear_spend_fvk()`. Nothing has been run yet.

Questions to answer with real txs:
- Can the buyer create, prove, sign, and redact its own spend FVKs?
- Can the seller then sign the carrier spend, extract, and broadcast?
- Does the node accept the result?

Steps:
1. Run with `DRY=1` first.
2. Ask Ryot before broadcasting.

Exit: one mined testnet tx containing both legs, the carrier bound in the same Action, and the payment to the seller. `verify` shows the new holder. Negative checks:
- the buyer alone cannot broadcast
- a tampered amount is rejected by the seller's check
- an expired PCZT is refused

Record everything under `evidence/testnet-<date>/`.

> Status: done 2026-09-30. `carrier sale-list | sale-buy | sale-accept`. Tx `6bdeb823ff65…` mined at 4425592: carrier pair in action 2, 10,000 zats to acct5. The verifier shows acct6 as holder. All three negative checks pass under `DRY=1`. See `evidence/testnet-2026-09-30/README.md`. Caveat: buyer and seller keys came from one local seed.
>
> 2026-10-02 (after the Codex review): the buyer runs R1 to R3 before paying, redaction covers every buyer field, the successor uses the buyer's OVK, the seller also checks listing expiry, PCZT expiry cap, carrier liveness, value pools and action count, and a seller who edits the tx after the buyer signed fails extraction. All DRY: `evidence/testnet-2026-10-02/README.md`. Still open: a run with separate seeds (needs a broadcast).

### Task 3. Browser network: streaming and Ironwood compact data

> Status 2026-10-02: streaming works. ChainSafe grpc-web streams `GetBlockRange` to a cross-origin Chromium page with Ironwood actions; the relay (`research/stage1-checks/relay.mjs`) now streams as well. Results and numbers in `research/stage1-checks/RESULTS.md` (P6 section). Still open: decrypting and finding the carrier in the browser (needs the wasm tx parser, see Task 5), other browsers, rate limits.

- Check `GetBlockRange` (server streaming) over grpc-web against `zcash-testnet.chainsafe.dev`.
- Check that its compact blocks include `ironwood_actions`.
- If either fails, extend `research/stage1-checks/relay.mjs` to support streaming.
- Exit: a browser page scans a small height range and finds the item #1 carrier in the buyer vault.

### Task 4. FINAL hard seal (OPEN_ISSUES #2)

Send FINAL to an address with no known spending key, for example a nothing-up-my-sleeve point. Alternatively, prove that FINAL's nullifier is spent into nothing. This needs a new test collection and Ryot's approval to broadcast.

### Task 5. Core crate for native and WASM

> Status 2026-10-02, later: done. The PCZT sale path is in `crates/carrier-sale` and runs in wasm, built with wasi-sdk 34 (installed at `~/.local/wasi-sdk`, no sudo, SHA-256 checked against the GitHub release digest). The CLI's `sale-buy` and `sale-accept` call it; the DRY regression (happy path, tamper after sign, underpay, expired, TTL 100, listing changed, wrong seller) is in `evidence/testnet-2026-10-02/carrier_sale_refactor_dry.log`. Next for the browser: multithreaded proving (`wasm-bindgen-rayon`) and the grpc-web chain source.
>
> Earlier status: done for everything except the PCZT sale path. `carrier-core` holds no network or file code and does not depend on `zcash_primitives` (whose transparent support pulls `secp256k1-sys`, C code that Apple clang cannot compile for wasm32). Rules take Ironwood bundles; the CLI extracts them from transactions. Next: a wasm-capable C compiler to bring the PCZT path to the browser, then wasm-bindgen exports. Ryot approved Homebrew LLVM on 2026-10-02, but Homebrew is not installed and installing it needs sudo; the alternative is the official wasi-sdk release (no sudo), which needs his OK as a different download.

Extract memo format, key derivation, binding builder, and verifier into `carrier-core`, compiling to native and `wasm32-unknown-unknown`. Keep `carrier` and `verify` as thin CLIs over it. Exit: `verify.sh` is still green, and the WASM build compiles.

Remember the multithreading requirements (see `research/stage1-checks/RESULTS.md`):
- nightly Rust
- `+atomics,+bulk-memory`
- `wasm-bindgen-rayon`
- COOP/COEP headers

### Task 6. Web app (SPEC draft 4 approved 2026-10-02; ask for design direction first, D6)

Follow the approved SPEC (draft 3 at the time of writing): pages, connect flow with a Noir derived signature, deposit, intent and authenticated disclosure, buy, accept. Prerequisites P4 to P12 come first.

Before any UI work, ask Ryot for design direction. His general preferences:
- typography and reading order first
- no generic dark SaaS look
- no default Tailwind/shadcn feel
- avoid cyan, indigo, or purple accent drift

Exit: the acceptance list in SPEC section 7, with Playwright screenshots.

## 6. Known risks to keep in view

- **Origin-unbound signatures (measured).** Noir signs without origin binding. A phishing site that gets the same message approved gets the same collectibles key. The blast radius is carriers plus the collectibles account's balance. Mitigations are in SPEC 2.1. Long-term, ask Noir for origin-bound signing.
- **Two enforcement layers.** Double spend is stopped by consensus. Forged successors and extra mints are stopped by the verifier (a social layer). Say this honestly in any public copy.
- **Buyer privacy.** The buyer sees the previous vault's history. Per-item vaults limit this.
- **One lightwalletd operator.** Heights come from a single source, and a second one has not been confirmed (OPEN_ISSUES #5).
- **Browser proving speed** was measured on an M4 Pro. Low-end devices will be slower. Single-thread 4-Action proving takes 21.8 s.

## 7. How to report back to Ryot

- Reply in Korean, short, conclusion first.
- Cite commands, txids, and files for every number.
- Mark anything unverified as unverified.
- Before a testnet broadcast or a GitHub push, ask first and state exactly what will be sent.

## 8. Copy-paste prompt for starting Claude Code

```
Continue ~/work/zcash-carrier-poc. Read CLAUDE.md, then docs/HANDOFF.md, then run `cargo build --release && bash verify.sh`.
Report in Korean which HANDOFF claims you confirmed and which drifted. Then start Task 1 (OPEN_ISSUES #7).
Do not broadcast testnet transactions or push to GitHub without asking me first.
```
