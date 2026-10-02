# Marketplace SPEC (draft 2, Stage 1: public, browser-only)

Status: draft for approval. BUILD starts after Ryot approves. Not approved yet.
Draft 2 (2026-10-01) replaces draft 1 (2026-09-30). The changes are listed in section 14. Draft 0, a local companion on one Mac, was dropped because it could not serve the public.
Evidence for measured claims: `research/stage1-checks/RESULTS.md` (browser), `evidence/testnet-2026-09-30/` (atomic sale), `docs/research/zecbit-whitepaper.md` (lessons from the competing design).

## 1. Goal

An OpenSea-style public website where anyone with a Zcash wallet can:
- connect and see the carrier items they hold, each with a verifier verdict computed in their own browser
- list an item at a fixed price
- buy it in **one** atomic transaction, with payment and item transfer in the same tx

Product principles (see `docs/VISION.md`):
- **No custody.** The site never holds funds, items, or keys. The server keeps only public listings and relays encrypted messages.
- **Private ownership.** Nothing the server stores or publishes reveals who holds an item or an item's history.
- **Check, don't trust.** Verdicts come from chain data in the user's browser, never from our server.

## 2. Stage 1 model

| Piece | Where it runs | Holds |
|---|---|---|
| UI, verifier, scanner, prover, signer | user's browser (WASM, Web Workers) | the collectibles key, in memory only. The IndexedDB cache holds FVKs and notes, never a spending key. |
| order book | our server | public listings (section 3.1) and encrypted messages between buyer and seller. Never a vault FVK, a note, a provenance package, or a user key. |
| chain access | our lightwalletd behind a grpc-web proxy (section 6.2). ChainSafe grpc-web as fallback | nothing. Stateless. |
| payments in and out | the user's normal wallet (Noir first) | the user's main funds |

### 2.1 Collectibles key derivation

Unchanged from draft 1:
1. Detect `window.noirwallet` (Noir 1.0.37 or later, testnet build for this phase).
2. Call `zcash_requestAccounts`. We get the shielded address, used as the sweep destination.
3. Call `zcash_signMessage(M, {signingMode: "derived"})`. Measured as deterministic (3/3 identical).
4. Compute `seed = HKDF-SHA256(ikm = signature, salt = "carrier/v1", info = M)`. From `seed` derive a ZIP-32 account: the **collectibles account**.
5. From the collectibles account, derive one **vault key per item** at a hardened ZIP-32 index `hash(coll, num) mod 2^31`. Only item-scoped FVKs are ever disclosed.

`M` is fixed and versioned, for example:
`Carrier collectibles key v1. Only approve on https://<domain>. Approving elsewhere lets that site move your carrier items. It cannot move your ZEC.`

Security properties:
- Measured: Noir signs without binding the origin. Any site that gets the same `M` approved gets the same key. Blast radius: carrier items and the collectibles account's balance. The main wallet is out of reach.
- Mitigations: warning text in `M`; a small collectibles balance; no auto-connect; the browser security rules in section 6.1.
- Fallback: a random collectibles seed with a 24-word backup, for wallets that are not deterministic.

### 2.2 Money flow

- **Deposit (buyer):** a ZIP-321 request to the buyer's collectibles address for `price + fee`, paid with Noir `zcash_sendTransaction` or any wallet by QR. Needed because no wallet can join a multi-party PCZT from a website yet. Costs one extra tx fee.
- **Proceeds (seller):** the sale pays a **fresh diversified address of the seller's collectibles account, one per listing**, so listings by one seller are not linkable through the pay address. The UI then offers a sweep to the main wallet. Decision D2 (section 10) covers the alternative of paying the main wallet directly.

## 3. Atomic sale

Proven on testnet with the CLI on 2026-09-30: tx `6bdeb823ff65…`, carrier pair in one Action, payment to the seller, buyer alone cannot extract, tampered amount refused, expired PCZT refused (`evidence/testnet-2026-09-30/README.md`).

Actions in the sale tx:
1. the seller's carrier spend and its successor to the buyer's new vault, in the **same Action**
2. the buyer's collectibles spend(s), paying `price` to the listing's pay address
3. the buyer's change; the buyer pays the network fee
4. the buyer's **provenance memo**: an output to the buyer's own new vault whose memo holds the compact provenance record (section 4.2)
5. dummy Actions up to the padding target (section 5)

### 3.1 Public listing

The order book publishes only:

`{listing_id, coll, num, price, expiry_height, seller_channel_key}`

- No vault FVK, carrier note, package, nullifier, or pay address appears in public. A public FVK would let anyone read the item's whole history and spot the sale tx.
- The listing page shows the artwork and an **R1 badge** (genuine issuance), both derivable from the public issuer FVK. R2 and R3 badges appear only for a committed buyer (3.2).
- `seller_channel_key` is an ephemeral X25519 key held by the seller's browser for this listing.

### 3.2 Buy flow

1. **Intent.** The buyer's browser posts an intent for `listing_id` with its own ephemeral X25519 key. The server rate-limits intents per listing.
2. **Disclosure.** The seller's browser (an open tab auto-responds) encrypts `{provenance package, carrier note, vault FVK, pay address}` to the buyer's key and posts it. The server relays ciphertext only.
3. **Verify.** The buyer's browser runs R1 to R3 against chain data. If anything fails, the buy stops.
4. **Build.** The buyer's browser builds the PCZT, retrying unproven builds until the carrier pair shares an Action, proves all Actions (the seller's Action needs only the disclosed note, Merkle path and FVK), signs the buyer spends, and redacts (3.3).
5. **Post.** The PCZT is encrypted to the seller's channel key and posted.
6. **Accept.** The seller's browser decrypts the payment output with its pay key and checks the amount, the carrier spend, the expiry and the branch id. It then signs the carrier spend, extracts and broadcasts.

The seller must be online for steps 2 and 6. An open tab handles both automatically.

### 3.3 Redaction before posting

The buyer clears, before step 5:
- its own spend FVKs, witnesses and ZIP-32 derivations (`clear_spend_fvk`, `clear_spend_witness`, `clear_spend_zip32_derivation`)
- the plaintext fields of its change and provenance outputs (`clear_output_recipient`, `clear_output_value`, `clear_output_rseed`, `clear_output_ock`)
- per-Action value randomness (`clear_rcv`)

The seller's check reads only effects (ciphertexts and nullifiers), so it does not need these fields. Not yet verified: that extraction still succeeds after every field above is cleared (prerequisite P3).

### 3.4 Race, expiry and the free option

- Two sales of one carrier spend the same nullifier, so at most one can be mined. A double sale is impossible by construction.
- PCZTs carry `expiry_height`. Default: tip + 20 (about 25 minutes). The order book drops expired PCZTs.
- Until expiry the seller alone decides whether to broadcast (a free option). The buyer can cancel early by spending its deposit note to itself, which costs one fee. The UI says so.
- Griefing (abandoned intents, spam PCZTs) is bounded by rate limits and by expiry. No funds are at risk.

### 3.5 Marketplace fee (option, untested)

If the marketplace charges a fee, it is an extra output in the same sale tx to the marketplace's address. The order book relays a PCZT only if its own key decrypts a fee output of the right amount. The marketplace still never holds user funds. Decision D3.

## 4. Provenance

### 4.1 What the buyer must receive

Per hop: the vault FVK. The verifier decrypts each note from chain with that FVK, so notes need not be sent. Plus the collection record (public) and the item number.

### 4.2 Compact record in a self-addressed memo

The buyer's provenance output (section 3, item 4) stores, encrypted to the buyer's own new vault:
- format tag and version, collection id, item number
- genesis txid
- one 96-byte FVK per hop (`ak || nk || rivk`), chunked: about 4 hops per 512-byte memo; longer histories continue in later self-addressed outputs

Effect: restoring the collectibles key from the wallet signature recovers every item **and** its proofs, with no local files. Losing local storage loses nothing.

## 5. Transaction shape

- **Padding.** Every sale tx is padded with dummy Actions to one fixed count, chosen to match the most common Ironwood Action count on chain. The count is to be measured (prerequisite P5). The CLI sale had 3 Actions.
- **Anchor age.** The anchor is a random recent height, not always the tip, so the anchor does not reveal when the tx was built. Allowed range to be confirmed against consensus rules (P5).
- **Timing.** The site publishes no sale times. Listings are removed without a public "sold" event.

## 6. Security and privacy of the service

### 6.1 Browser code is the wallet

- Reproducible builds; the release hash is published and listed in the repo.
- Subresource Integrity on every script; strict CSP; no third-party scripts or analytics.
- COOP `same-origin` and COEP `require-corp` on every page (also needed for multithreaded WASM).
- Keys live in memory only. A CI test asserts that no request body ever contains key material.
- Precedent: in June 2026 a browser-wallet marketplace sent users' private keys to its server and wallets were drained (ZecBit whitepaper p.176, unverified).

### 6.2 Network

- Run our own lightwalletd behind a grpc-web proxy. Third-party servers learn which items users look at.
- Fetch block ranges, not single transactions, so lookups do not name the item being verified.
- Broadcast through our relay so the node operator does not see user IPs. Optional Tor guidance for users who want to hide their IP from us.

### 6.3 Content

- Artwork on chain cannot be deleted. The site can hide an item or a collection from its own pages and runs a report flow.
- A moderation policy is required before the public beta (decision D5).

## 7. Pages

| Route | Content |
|---|---|
| `/` Explore | collection cards: mosaic, supply (finalized or open), floor, listed count |
| `/collection/:coll` | stats header; grid with price badges; filters: listed only, sort by price |
| `/item/:coll/:num` | image from the on-chain memo; R1 badge; price; **Buy** or **List**. After the buyer's intent: R2 and R3 badges and the provenance timeline, shown to that buyer only |
| `/account` | Collected, Listed, Activity (local only); collectibles balance; Deposit and Sweep |
| Connect modal | wallet detection, signature explainer, network badge |
| List modal | price, expiry, what will be disclosed to a committed buyer |
| Buy modal | price + fees, deposit step, progress (intent, verify, build, prove, sign, post), cancel option |
| Accept | automatic in an open tab; manual review screen otherwise |

## 8. Runtime requirements (measured)

- Multithreaded WASM: nightly Rust, `+atomics,+bulk-memory`, `wasm-bindgen-rayon`.
  - Measured on M4 Pro, 14 threads: ProvingKey 3.9 s once, 2 Actions 1.9 s, 4 Actions 3.6 s. Single-thread: 11.9 s and 21.8 s.
  - The padded sale (section 5) may be larger than 4 Actions. Measure after P5.
- Not yet verified: streaming `GetBlockRange` over grpc-web, and Ironwood compact actions on ChainSafe (HANDOFF task 3).

## 9. Prerequisites before BUILD

| ID | Item | State |
|---|---|---|
| P1 | Successor validity rule (OPEN_ISSUES #7) | done 2026-09-30 |
| P2 | CLI two-party PCZT sale on testnet | done 2026-09-30, tx `6bdeb823ff65…` |
| P3 | Redaction (3.3) with extraction still succeeding; a seller who edits the tx after the buyer signed is rejected | to do |
| P4 | Compact provenance format (4.2), verifier reading FVK-only hops, seed-only recovery test | to do |
| P5 | Measure the Ironwood Action-count distribution; padding and anchor-age rules | to do |
| P6 | Browser network: grpc-web streaming and Ironwood compact data (HANDOFF task 3) | to do |
| P7 | `carrier-core` crate for native and wasm32 (HANDOFF task 5) | to do |
| P8 | Per-item vault derivation (2.1 step 5) replacing the PoC `OFF` offsets | to do |
| P9 | FINAL hard seal (OPEN_ISSUES #2) | to do, needs a new test collection and broadcast approval |

## 10. Decisions for Ryot

| ID | Decision | Draft 2 default |
|---|---|---|
| D1 | Approve this SPEC | pending |
| D2 | Sale proceeds to a per-listing collectibles address (unlinkable, then sweep) or directly to the main wallet (simpler, linkable) | per-listing address |
| D3 | Marketplace fee: none, or a fee output in the sale tx (3.5) | none for the testnet beta |
| D4 | Testnet beta prices: does a TAZ price count as "offering testnet assets" (CLAUDE.md rule 5)? | free transfers only, no TAZ prices, until decided |
| D5 | Content moderation policy | required before the public beta |
| D6 | Visual design direction | ask before any UI work |
| D7 | Mainnet, and legal review before any real value | out of scope for Stage 1 |

## 11. Acceptance (MVP, testnet)

1. Two Chromium profiles with the Noir testnet build (fresh test seeds) connect.
2. A lists item #1. The public listing JSON contains no FVK, note, nullifier or pay address (automated check).
3. B posts an intent, receives the disclosure, and sees R1 to R3 pass in the browser.
4. B deposits, clicks Buy; the PCZT is posted encrypted. The PCZT contains no plaintext for B's change or provenance outputs (automated check).
5. A's tab accepts. **One** testnet tx contains both legs, the provenance memo, and exactly the padding-target number of Actions.
6. After mining: B shows #1 in Collected. The CLI verifier agrees on the new owner.
7. Restore: B clears all browser storage, reconnects, and recovers #1 with its full provenance from chain alone.
8. Negative checks: B alone cannot broadcast; a tampered amount is refused by A; an expired PCZT is refused; a tx A edits after B signed fails extraction.
9. Playwright covers every page and modal. Screenshots go under `.harness/runs/`.

## 12. Out of scope (Stage 1)

Mainnet, WalletConnect, connect with wallets other than Noir (any wallet can still deposit by QR), auctions, offers, royalties, launchpad, naming.

## 13. Later stages

- **Stage 2:** propose carrier support to Vizor (Chainapsis). Keys stay inside the wallet.
- **Stage 3:** a carrier method set in Zcash WalletConnect (Noir team grant, approved 2026-08-31, not shipped).
- **Ask Noir:** origin-bound `signMessage` and a derived-key API, which would close the risk in 2.1.

## 14. Changes from draft 1

- Section 3: the atomic sale is now proven (P2), no longer an open question.
- New 3.1: public listings carry no FVK, note or package. Disclosure goes only to a committed buyer over an encrypted channel (3.2). Reason: a public FVK exposes the item's history and the sale tx (ZecBit whitepaper p.226-227, 418, 421).
- New 3.3: redaction of the buyer's change, provenance output and value randomness, not only FVKs.
- New 3.4: free option, cancel path, default expiry tip + 20.
- New 3.5 and D3: optional fee output, still non-custodial.
- New section 4: FVK-only provenance stored in a self-addressed memo, so a seed restore recovers proofs (whitepaper p.238, 309).
- New section 5: Action padding, anchor-age randomisation, no public sale times (whitepaper p.273, 304, 315).
- New section 6: browser code security, own lightwalletd, range fetches, content moderation.
- 2.2: proceeds go to a per-listing address by default (D2).
- Prerequisites and acceptance expanded (sections 9 and 11). New decisions table (section 10).
- Prior art: the note-chaining primitive was published first by ZecBit's NSCV (2026-09-20). Carrier's distinct contributions are the non-custodial atomic sale and a working implementation.
