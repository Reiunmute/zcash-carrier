# Marketplace SPEC (draft 1, Stage 1: public, browser-only)

Status: draft for approval. BUILD starts after Ryot approves.
Replaces draft 0, which assumed a local companion on one Mac. That model cannot serve the public, so it was dropped.
Evidence for every measured claim: `research/stage1-checks/RESULTS.md`.

## 1. Goal

An OpenSea-style public website where anyone with a Zcash wallet can:
- connect and see the carrier items they hold, each with a verifier verdict
- list an item at a fixed price
- buy it in **one** atomic transaction, with payment and item transfer in the same tx

The site never asks for a seed phrase and never holds anyone's keys on a server.

## 2. Stage 1 model

| Piece | Where it runs | Holds |
|---|---|---|
| UI, verifier, scanner, prover, signer | user's browser (WASM, Web Workers) | the collectibles key, in memory only. The IndexedDB cache holds FVK and notes, no spending key. |
| order book | our server | public listings: order + provenance package (vault FVK, carrier note). No user keys. |
| lightwalletd access | ChainSafe grpc-web (measured OK) or our grpc-web relay (measured OK) | nothing. The relay is stateless. |
| payments in and out | the user's normal wallet (Noir first) | the user's main funds |

### 2.1 Collectibles key derivation

Connect flow:
1. Detect `window.noirwallet` (Noir 1.0.37 or later, testnet build for this phase).
2. Call `zcash_requestAccounts`. We get the shielded address, used as the payout address.
3. Call `zcash_signMessage(M, {signingMode: "derived"})`. Measured as deterministic (3/3 identical).
4. Compute `seed = HKDF-SHA256(ikm = signature, salt = "carrier/v1", info = M)`. From `seed` derive a ZIP-32 Orchard account: the **collectibles account**.
5. From the collectibles account, derive one **vault key per item**: `ZIP-32 account index = hash(coll, num) mod 2^31`. Only item-scoped FVKs are ever disclosed. This keeps a vault FVK from exposing other items.

`M` is fixed and versioned, for example:
`Carrier collectibles key v1. Only approve on https://<domain>. Approving elsewhere lets that site move your carrier items. It cannot move your ZEC.`

Security properties:
- Measured fact: Noir signs without binding the origin. Any site that gets the same `M` approved gets the same key. Blast radius: carrier items and the collectibles account's ZEC balance. The user's main wallet is out of reach.
- Mitigations: warning text in `M`; a small collectibles balance (deposit per purchase, sweep proceeds out); no auto-connect; a reproducible, public build.
- Fallback: if a wallet is not deterministic, the site generates a random collectibles seed and asks the user to back up 24 words. This works but is worse UX.

### 2.2 Money flow

- **Deposit (buyer):** the site builds a ZIP-321 request to the buyer's collectibles address for `price + fee`, then calls Noir `zcash_sendTransaction({to, amount})`. Any other wallet can pay the same request by QR.
- **Proceeds (seller):** the sale tx pays the seller's **main wallet shielded address** (from step 2) directly. The seller's collectibles account never has to hold sale proceeds.
- **Why a deposit is needed:** no wallet can join a multi-party PCZT from a website yet. The deposit costs one extra tx fee (ZIP-317, 10,000 zats for 2 Actions).

## 3. Atomic sale (PCZT, one transaction)

Actions:
1. seller vault carrier spend, and the carrier successor to the buyer's new vault, in the **same Action**
2. buyer collectibles spend(s): `price` to the seller's main address
3. buyer change; the buyer pays the fee

Flow:
1. **List:** the seller publishes `{coll, num, price, seller_pay_addr, expiry, provenance package}`. The package already discloses the vault FVK and the carrier note, as provenance requires.
2. **Buy:** the buyer's browser builds the PCZT and retries unproven builds until the carrier pair shares an Action. It then proves (about 4 Actions, measured 3.6 s on 14 threads) and signs the buyer spends. Finally it redacts the buyer FVKs and posts the PCZT to the order book.
3. **Accept:** the seller's browser opens the listing, decrypts the payment output (`price` to `seller_pay_addr`), signs the carrier spend, extracts, and broadcasts.
   - The seller must come online to accept. Keeping a tab open auto-accepts matching PCZTs.
4. Neither party can finish alone. Without both spend authorizations the tx is invalid.

Race and expiry:
- If the seller moves the carrier elsewhere first, the buyer's PCZT becomes invalid. The buyer loses nothing, because the buyer spends are unconfirmed.
- PCZTs carry `expiry_height`. The order book drops stale ones.

## 4. Pages (OpenSea information architecture)

| Route | Content |
|---|---|
| `/` Explore | collection cards: mosaic, supply (finalized or open), floor, listed count |
| `/collection/:coll` | stats header; grid with price badges; filters: listed only, sort by price |
| `/item/:coll/:num` | image rendered from the on-chain memo; owner (you or not you); price; **Buy** or **List**; provenance timeline (txid and height per hop); verifier badges R1 to R3 |
| `/account` | tabs: Collected, Listed, Offers to accept, Activity; collectibles balance; Deposit and Sweep |
| Connect modal | wallet detection, signature explainer, network badge |
| List modal | price, disclosure preview (item vault FVK and carrier note), expiry |
| Buy modal | price + fee, deposit step if needed, progress (build, prove, sign, post) |
| Accept modal | payment check result, sign, broadcast |

## 5. Runtime requirements (measured)

- Multithreaded WASM: nightly Rust, `+atomics,+bulk-memory`, `wasm-bindgen-rayon`.
  - Every page is served with `COOP: same-origin` and `COEP: require-corp`.
  - Measured on M4 Pro with 14 threads: ProvingKey 3.9 s once, 2 Actions 1.9 s, 4 Actions 3.6 s.
  - Single-thread fallback: 11.9 s for 2 Actions and 21.8 s for 4.
- Noir WASM already ships a multithreaded build. Our bundle is separate and is not shared with Noir.
- lightwalletd: `zcash-testnet.chainsafe.dev` grpc-web works from the browser. Our relay stays in reserve.
  - Not yet verified: streaming `GetBlockRange`, and Ironwood compact actions on ChainSafe.

## 6. Core prerequisites

1. OPEN_ISSUES #7: a successor is valid only if `value == 0` and the memo is `HOP` for the same `(coll, num)`. Otherwise the item is burned.
2. Per-item vault derivation (2.1 step 5) replaces the `OFF` hack from the PoC.
3. Port `carrier` and `verify` into a `carrier-core` crate: `no_std`-friendly, compiling for native and wasm32.
4. Before BUILD, confirm PCZT feasibility on testnet with a CLI two-party sale, following the steps in section 3.
   - Open question: `pczt 0.9.3` and every role for the Ironwood bundle.
   - Open question: whether the seller can sign after the buyer redacts.
   - Open question: whether the node accepts the extracted tx.

## 7. Acceptance (MVP, testnet)

1. Two Chromium profiles with the Noir testnet build (fresh test seeds) connect.
   - Profile A imports the vault holding item #1 from the 2026-09-29 run and shows it with R1 to R3 badges.
2. A lists #1 at 0.01 TAZ.
3. B deposits from Noir (one tx), clicks Buy, and a PCZT is posted.
4. A accepts. **One** testnet tx contains both legs.
5. After mining: B shows #1 in Collected, A's Noir wallet shows 0.01 TAZ received, and the CLI verifier agrees on the new owner.
6. Negative checks:
   - B alone cannot broadcast.
   - A tampered payment amount is rejected by A's accept check.
   - An expired PCZT is refused.
7. Playwright covers every page and modal. Screenshots go under `.harness/runs/`.

## 8. Out of scope (MVP)

Mainnet, Vercel production, WalletConnect, wallets other than Noir for connect (any wallet can still deposit by QR), auctions, offers, royalties, launchpad, naming.

## 9. Later stages

- **Stage 2:** propose carrier support to Vizor (Chainapsis, Keplr team, Apache-2.0, Rust core with PCZT). Keys stay inside the wallet.
- **Stage 3:** a carrier method set in Zcash WalletConnect (Noir team grant, approved 2026-08-31, not shipped).
- **Ask Noir:** origin-bound `signMessage` and a derived-key API. That would close the risk described in 2.1.
