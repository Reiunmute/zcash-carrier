# Stage 1 feasibility checks (2026-09-30)

Three questions had to be answered before a public, browser-only marketplace could be specified.
All three were measured on a Mac mini (Apple M4 Pro, 14 cores: 10 performance + 4 efficiency).

## 1. Is a wallet signature stable enough to derive a key from?

**Result: yes, in Noir Wallet's `derived` mode and in its default mode.**

Method:
- Downloaded Noir Wallet 1.0.37 from the Chrome Web Store (extension id `mfoghjbpfanobmnoemoepenjjcmfpmdn`).
- Ran its own bundled worker (`zcash.worker-*.js`) and WASM (`zcash_wallet_wasm_bg-*.wasm`) unmodified in headless Chromium. Script: `noir-sign/`.
- Seed: the public BIP-39 test vector (`abandon` x23 + `art`). No funds.

Findings from the extension code:
- `zcash_signMessage(message, {signingMode})` resolves to `sign_message_with_transparent(network, usk, message, addressIndex)`.
- `signingMode: "derived"` signs with transparent address index `2147483647`. The default mode signs with the current address.
- The WASM links `secp256k1 0.29.1` and prefixes messages with `Zcash Signed Message:\n`. libsecp256k1 ECDSA uses RFC 6979 deterministic nonces.
- **The signing call takes no origin argument.** The origin is only shown in the approval popup.

Measured:

| Call | Signature prefix | Same across runs |
|---|---|---|
| derived x3 | `1bc6511a48ba…` | yes (3/3) |
| current x2 | `1cc48367e1bb…` | yes (2/2) |
| derived, message changed by 2 chars | `1b93a6eb6f34…` | differs, as expected |

The derived key also differs from the current key: `046cdb…` vs `045a6f…`.

Consequence: a site can derive a collectibles key as `HKDF(signature)`. Because the signature is not bound to the origin, **any site that shows the same text and gets the user to approve it obtains the same key.** Mitigations are listed in the SPEC.

## 2. Can a browser prove Ironwood Actions fast enough?

**Result: yes, with multithreading. Single-threaded WASM is slow but usable.**

Setup:
- `orchard 0.15.5`, `BundleVersion::ironwood_v3()` (PostNu6_3 circuit).
- Zero-value outputs and an empty anchor. Proving cost depends on the number of Actions, not on their contents.
- Crate: `provebench/`.

| Runtime | ProvingKey build | 2 Actions | 4 Actions |
|---|---|---|---|
| native, 1 thread | 2.0 s | 2.45 s | 4.50 s |
| native, 8 threads | 1.0 s | 0.50 s | 0.92 s |
| WASM, Node 24, 1 thread | 8.7 s | 11.8 s | 21.7 s |
| WASM, Chromium, 1 thread | 8.5 s | 11.9 s | 21.8 s |
| **WASM, Chromium, 14 threads** (`wasm-bindgen-rayon`, cross-origin isolated) | **3.9 s** | **1.9 s** | **3.6 s** |

Notes:
- A carrier transfer is 2 Actions. A sale is about 4.
- The ProvingKey is built once per session and can then be cached in memory.
- The multithreaded build needs nightly Rust (`build-std`, `+atomics`). The page must be served with `COOP: same-origin` and `COEP: require-corp`.
- Without a bundler, the generated `workerHelpers.js` must import `../../../provebench.js` instead of `../../..`.

## 3. Can a browser reach testnet lightwalletd?

**Result: not directly on zec.rocks. Yes through a grpc-web endpoint that sends CORS headers, or through our own relay.**

These are real `fetch` calls from a page on another origin (Chromium). Script: `pw_cors.mjs`.

| Endpoint | curl | Browser fetch | Why |
|---|---|---|---|
| `testnet.zec.rocks` | 200 (native gRPC) | fails | no CORS preflight (OPTIONS returns 404) |
| `zcash-wallet-grpc-testnet.deltarpc.com` (used by Noir) | 200, grpc-web | fails | sends `access-control-allow-origin` **twice**, which browsers reject |
| `zcash-testnet.chainsafe.dev` | 200, grpc-web | **ok**, height 4,421,308 | proper CORS |
| `zcash-mainnet.chainsafe.dev` | 200, grpc-web | ok, height 3,501,422 | proper CORS |
| own relay `relay.mjs` (grpc-web to gRPC, unary only, allowlisted methods) | 200 | **ok**, height 4,421,308 | we control the headers |

Still to verify:
- Streaming calls (`GetBlockRange`) through grpc-web. Scanning needs them.
- Whether ChainSafe's backend serves Ironwood compact actions.

## P6, 2026-10-02: grpc-web streaming and Ironwood compact data

**Result: yes. A browser streams `GetBlockRange` over grpc-web from ChainSafe directly, frames arrive incrementally, and the compact blocks carry Ironwood actions (`CompactTx.ironwood_actions`, tag 9). Our relay does the same against zec.rocks.**

Same Mac mini as above. Read-only calls only (OPTIONS, `GetBlockRange`, `GetLatestBlock`). No transaction was sent.

Protocol facts (from `zcash_client_backend-0.24.0/src/proto/{service,compact_formats}.rs`):
- `BlockRange { start: BlockID = 1, end: BlockID = 2, pool_types: repeated PoolType = 3 }`, `BlockID.height = 1`.
- `PoolType`: `SAPLING = 2`, `ORCHARD = 3`, `IRONWOOD = 4`. Empty `pool_types` means "all shielded pools, including Ironwood" (doc comment on `BlockRange`). Our Rust code (`src/lib.rs:228`) already sends `pool_types: vec![]`.
- `CompactTx.ironwood_actions = 9` (type `CompactOrchardAction { nullifier = 1, cmx = 2, ephemeral_key = 3, ciphertext = 4 }`), separate from Orchard `actions = 6`. `ChainMetadata.ironwood_commitment_tree_size = 3`.

Commands:

```bash
cd research/stage1-checks
node stream_check.mjs                                                         # ChainSafe, 4425590..4425594, default pools
node stream_check.mjs https://zcash-testnet.chainsafe.dev 4425590 4425594 4   # pool_types [IRONWOOD]
node stream_check.mjs https://zcash-testnet.chainsafe.dev 4425590 4425594 3   # pool_types [ORCHARD]
node stream_check.mjs https://zcash-testnet.chainsafe.dev 4424595 4425594     # 1,000 blocks
node stream_check.mjs https://zcash-testnet.chainsafe.dev 4415595 4425594     # 10,000 blocks
PORT=18767 node relay.mjs &                                                   # upstream https://testnet.zec.rocks:443
node stream_check.mjs http://127.0.0.1:18767 [same ranges and pools]          # relay; stopped afterwards
```

Plus one headless Chromium run (`chrome-headless-shell` 151.0.7922.34 from the existing Playwright cache, page origin `http://127.0.0.1:18768`) that ran the same `fetch` + `ReadableStream` reader inside the page. That page was served by an inline `node -e` script. No file was kept for it.

`stream_check.mjs` sends `content-type: application/grpc-web+proto`, `x-grpc-web: 1`, and an `Origin` header. It parses the 5-byte frames as each network chunk arrives and decodes block height, hash, prev_hash, chain metadata, and per tx the txid and Ironwood (nf, cmx). It also checks that each block's prev_hash equals the hash of the block before it.

### Known-fact check (range 4425590..4425594)

| Check | ChainSafe | relay -> zec.rocks | Chromium -> ChainSafe |
|---|---|---|---|
| grpc-web streaming, trailer `grpc-status 0` | yes | yes | yes |
| blocks / Ironwood actions | 5 / 8 | 5 / 8 | 5 / (not decoded) |
| tx `6bdeb823...98ba5b` at 4425592 | found, 3 Ironwood actions (tx index 1) | same | found (byte search) |
| cmx `5cb2cdaa...37d720` | found, Ironwood action 3 of that tx | same | found (byte search) |
| prev_hash links | 4/4 | 4/4 | not checked |

- The Ironwood tree size in `ChainMetadata` goes 404059, 404060, 404064, 404065, 404066 for 4425590..4425594, consistent with 1, 1, 4, 1, 1 Ironwood actions.
- Byte order: the txid is in protocol order (reverse it for display). The cmx bytes, hex-encoded as serialized, equal the evidence hex directly (no reversal). Block hashes are reversed for display (they start with `00`).
- The 1,000-block decoded output (1,357 block/tx/action lines) was identical between ChainSafe and relay -> zec.rocks (`diff` empty).
- `pool_types`: `[4]` returned the same 8 Ironwood actions (2,007 bytes). `[3]` returned blocks with 0 txs (511 bytes). Both servers prune by pool type. Whether `[4]` also drops Sapling/Orchard data is **unverified** (this range had none).

### Timings (one run each, home network, not averaged)

| Client -> server | Range | First block | Total | Bytes | Network chunks |
|---|---|---|---|---|---|
| Node -> ChainSafe | 5 blocks | 158 ms | 160 ms | 2,007 | 1 |
| Node -> ChainSafe | 1,000 | 147 ms | 247 ms | 351,742 | 52 |
| Node -> ChainSafe | 10,000 | 110 ms | 816 ms | 4,084,297 | 2,075 |
| Node -> relay -> zec.rocks | 5 | 72 ms | 73 ms | 2,021 | 2 |
| Node -> relay -> zec.rocks | 1,000 | 39 ms | 371 ms | 351,756 | 135 |
| Node -> relay -> zec.rocks | 10,000 | 42 ms | 1,450 ms | 4,084,311 | 1,242 |
| Chromium -> ChainSafe | 5 | 214 ms | 215 ms | n/a | 1 |
| Chromium -> ChainSafe | 10,000 | 69 ms | 723 ms | n/a | 92 |
| Chromium -> relay -> zec.rocks | 5 | 59 ms | 59 ms | n/a | 1 |
| Chromium -> `testnet.zec.rocks` direct | 5 | `TypeError: Failed to fetch` (CORS, as in section 3) | | | |

- First block arrives well before the end of the stream, so the response is streamed, not buffered.
- At current testnet load, 10,000 blocks are about 4.1 MB and 17,804 Ironwood actions (about 410 bytes and 1.8 actions per block).
- The ChainSafe preflight took 84 to 345 ms across the 6 runs where it was printed. Browsers cache it for 600 s (`max-age`).
- The relay figures include the hop to zec.rocks (HTTP/2). The relay adds 14 bytes because its trailer frame also carries an empty `grpc-message`.

### CORS headers returned by ChainSafe (Origin `https://marketplace.example`)

- Preflight `OPTIONS`: `200`, `access-control-allow-origin: https://marketplace.example` (echoes the request Origin), `access-control-allow-credentials: true`, `access-control-allow-methods: POST`, `access-control-allow-headers: Content-Type, X-Grpc-Web, X-User-Agent`, `access-control-max-age: 600`, `vary: Origin, Access-Control-Request-Method, Access-Control-Request-Headers`.
- `POST`: `access-control-allow-origin` (echo), `access-control-allow-credentials: true`, `access-control-expose-headers` includes `grpc-status, grpc-message` and Fly.io headers (`Fly-Request-Id`, `X-Backend`, `X-Region`, `Via`), `vary: Origin`.
- In Chromium, `res.headers.get('grpc-status')` was `null`, which is expected here: the status comes in the body trailer frame (`grpc-status: 0`), and the page reads it from there.

### Relay changes (`relay.mjs`)

- `GetBlockRange` added to the allowlist. The rest of the list is unchanged, so `SendTransaction` is still allowed.
- Upstream DATA chunks are now written to the client as they arrive, then the 0x80 trailer frame. Unary calls go through the same path. Re-checked: `GetLatestBlock` via the relay returned 200, height 4,434,159.
- If the client disconnects, the relay closes the upstream stream. After cancelling a 10,000-block stream following the first chunk, the relay kept serving (`GetLatestBlock` 200). Whether zec.rocks actually received `RST_STREAM` is **unverified** (not observed on the wire).
- Methods outside the list are still refused (`GetMempoolStream` -> 403).

Incident: a shell quoting mistake once sent `start = 0` (NaN encoded as 0) to ChainSafe, which asked for the whole chain. The request was read-only and was stopped after about 3 minutes. `stream_check.mjs` now refuses non-integer heights and ranges over 20,001 blocks.

### What this means for the browser marketplace

- Works: a static page can scan compact blocks straight from `zcash-testnet.chainsafe.dev` with plain `fetch` + `ReadableStream` and no grpc-web library. Ironwood nullifiers and cmx values are present with the default (empty) `pool_types`, which is enough for nullifier-based spend detection and cmx lookup of a carrier's successor commitment.
- Works: `relay.mjs` is a drop-in fallback for zec.rocks (same bytes, same decoded content). Before exposing it publicly it needs a cap on range size and rate limits. With no cap, one request can ask for the whole chain.
- Does not work: direct browser access to `testnet.zec.rocks` (no CORS).
- Still **unverified**: trial decryption cost in the browser (only transport was measured), whether `pool_types [4]` prunes other pools, ChainSafe availability over time and its rate limits, full-history scan time from Ironwood activation (activation height not checked here), and behaviour in Firefox/Safari (Chromium only).
- Privacy: the server sees the client IP and every block range requested. Range scans leak less than per-tx lookups (`GetTransaction`), but the SPEC should still treat ChainSafe as a party that observes access patterns.

## P7, 2026-10-02: the whole PCZT sale in wasm

`crates/carrier-sale` holds the sale as pure functions; `selftest` runs it on synthetic notes in a local two-leaf Merkle tree: buyer build (3 Actions, same-Action binding), proving key, proof, buyer signature, redaction and its checks, seller checks, seller signature, and extraction, which verifies the proof and every signature.

| Runtime | Whole self-test |
|---|---|
| native, 1 thread (`cargo test --release -p carrier-sale`) | 6.9 s |
| WASM, Node, 1 thread (`crates/carrier-wasm/sale_selftest.cjs`) | 32.4 s |
| WASM, Chromium, 14 threads (`salebench/`, 2026-10-04) | 13.4 s first run, 9.5 s second |

- The wasm module with the `sale` feature is 5.0 MB (`wasm-bindgen --target nodejs`, release, not optimised with `wasm-opt`).
- It needs a C compiler that targets wasm, for `secp256k1-sys` (transparent support in `zcash_primitives`). wasi-sdk 34 (clang 23.1) works: `CC_wasm32_unknown_unknown=~/.local/wasi-sdk/bin/clang AR_wasm32_unknown_unknown=~/.local/wasi-sdk/bin/llvm-ar`.
- The self-test does both sides: the buyer's proving key and proof, and the seller's extraction, which builds the verifying key and checks the proof. A real buyer pays only its own half; the proving key is built once per session.
- `salebench/run.sh` builds with nightly Rust, `build-std`, `+atomics,+bulk-memory` and wasm-bindgen-rayon; the C objects of `secp256k1-sys` need `CFLAGS_wasm32_unknown_unknown="-matomics -mbulk-memory"` to link into shared memory, and the worker's `import('../../..')` must be patched to the module file when there is no bundler. Served cross-origin isolated (`provebench/coi_server.py`); `crossOriginIsolated` was true and 14 threads were used (Chromium in the Claude Code browser pane, Apple Silicon).

## P6 and P7, 2026-10-04: the verifier runs in the browser

`carrier-wasm` (`sale` feature) exports `BrowserChain`: JavaScript feeds it compact blocks from `GetBlockRange` over grpc-web and a `fetch_tx` callback for `GetTransaction` (`web/prototype/lib/lwd.mjs`, no dependencies). The wasm side parses the blocks (`carrier-core::compact`, checked against the prost types in `tests/compact_parse.rs`), requires contiguous, hash-linked heights and unique nullifiers, parses every tx and checks its txid, and runs the same rules R1 to R3 and the record walk as the CLI.

`crates/carrier-wasm/browser_verify.mjs` runs the seven verdict cases of `verify.sh` (HELD, SPENT, BURNED, FAIL, UNVERIFIED, and the P4 record walk) in wasm; all seven agree with the native verifier, against two servers:

| Server | Blocks indexed | Index time | Per verdict |
|---|---|---|---|
| `zcash-testnet.chainsafe.dev` (grpc-web) | 38,471 (4416690 to 4455160) | 3.3 s | 0.3 to 0.8 s |
| our `deploy/envoy.yaml` chain, upstream `testnet.zec.rocks` | 38,493 | 2.8 s | 0.3 to 0.8 s |

Node 24, one thread. `verify.sh` runs the first row.

In Chromium (the prototype's "Check on chain", `web/prototype/verify.mjs`): the P4 item rebuilt from its vault record and HELD by vault 17, 20,283 blocks in 2.8 s; R1 of the 2026-09-29 collection, 38,720 blocks in 5.4 s. Both from ChainSafe, one server only.
