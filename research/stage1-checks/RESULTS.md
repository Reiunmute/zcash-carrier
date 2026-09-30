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
