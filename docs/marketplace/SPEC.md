# Marketplace SPEC (draft 3, Stage 1: public, browser-only)

Status: **approved by Ryot on 2026-10-02**, with the draft 3 defaults for decisions D2 to D8 (section 10). Changes to this SPEC need his approval again. D6: ask for design direction before any UI work.
Draft 3 (2026-10-02) replaces draft 2 (2026-10-01) after the Codex review (`docs/research/codex-review-2026-10-02.md`). Changes are listed in section 14. Draft 0, a local companion on one Mac, was dropped because it could not serve the public.
Evidence for measured claims: `research/stage1-checks/RESULTS.md` (browser), `evidence/testnet-2026-09-30/` (atomic sale), `evidence/testnet-2026-10-02/` (hardened sale, DRY), `docs/research/zecbit-whitepaper.md` (lessons from the competing design).

## 1. Goal

An OpenSea-style public website where anyone with a Zcash wallet can:
- connect and see the carrier items they hold, each with a verifier verdict computed in their own browser
- list an item at a fixed price
- buy it in **one** atomic transaction, with payment and item transfer in the same tx

Product principles (see `docs/VISION.md`):
- **No custody.** The site never holds funds, items, or keys. The server keeps only public listings and relays encrypted messages.
- **Private ownership.** Nothing the server stores or publishes reveals who holds an item or an item's history.
- **Check, don't trust.** Verdicts come from chain data in the user's browser, never from our server. Section 6.2 states what the browser still trusts about chain data.

## 2. Stage 1 model

| Piece | Where it runs | Holds |
|---|---|---|
| UI, verifier, scanner, prover, signer | user's browser (WASM, Web Workers) | the collectibles seed, in memory only. The IndexedDB cache holds FVKs and notes, never a spending key. |
| order book | our server | public listings (3.1) and encrypted, authenticated messages between buyer and seller (3.2). Never a vault FVK, a note, a provenance record, or a user key. |
| chain access | two independent lightwalletd servers behind grpc-web (6.2) | nothing. Stateless. |
| payments in and out | the user's normal wallet (Noir first) | the user's main funds |

### 2.1 Key derivation

1. Detect `window.noirwallet` (Noir 1.0.37 or later, testnet build for this phase).
2. Call `zcash_requestAccounts`. We get the shielded address, used as the sweep destination.
3. Call `zcash_signMessage(M, {signingMode: "derived"})`. Measured as deterministic (3/3 identical).
4. Compute `seed = HKDF-SHA256(ikm = signature, salt = "carrier/v1", info = M)`.
5. ZIP-32 Orchard account 0 of `seed` is the **collectibles account**. It funds purchases and receives the account index records (4.3).
6. Each item gets its own **vault**: ZIP-32 Orchard account `vault_index` of the same `seed`, where
   `vault_index = 1 + (LE32(BLAKE2b-256(personal = "CarrierVaultIdx1", coll || LE32(num))[0..4]) mod (2^31 - 1))`.
   If that index is already used by another item of this seed, take the next free index. The index actually used is stored in the account index record, so recovery never recomputes it (4.3). Only vault FVKs are ever disclosed.

`M` is fixed and versioned, for example:
`Carrier collectibles key v1. Only approve on https://<domain>. Approving elsewhere lets that site move your carrier items. It cannot move your ZEC.`

Security properties:
- Measured: Noir signs without binding the origin. Any site that gets the same `M` approved gets the same seed. Blast radius: carrier items and the collectibles account's balance. The main wallet is out of reach.
- Mitigations: warning text in `M`; a small collectibles balance; no auto-connect; the browser security rules in 6.1.
- Fallback: a random collectibles seed with a 24-word backup, for wallets that are not deterministic.

### 2.2 Money flow

- **Deposit (buyer):** a ZIP-321 request to the buyer's collectibles address for `price + fee`, paid with Noir `zcash_sendTransaction` or any wallet by QR. Needed because no wallet can join a multi-party PCZT from a website yet. Costs one extra tx fee.
- **Proceeds (seller):** the sale pays a **fresh diversified address of the seller's collectibles account, one per listing**, so one seller's listings are not linkable through the pay address. The UI then offers a sweep to the main wallet. Decision D2 covers paying the main wallet directly instead. (The CLI does this since 2026-10-02.)

## 3. Atomic sale

Mined on testnet with the CLI on 2026-09-30 (tx `6bdeb823ff65…`), with buyer and seller keys from one local seed. Hardened and DRY-tested on 2026-10-02: the buyer verifies before paying, full redaction, and nine negative checks including a seller who edits the tx after the buyer signed (`evidence/testnet-2026-10-02/README.md`).

Outputs of the sale tx, all created by the buyer:
1. the carrier successor to the buyer's new vault, in the **same Action** as the seller's carrier spend
2. the payment of `price` to the listing's pay address
3. the buyer's change; the buyer pays the network fee
4. the buyer's **vault record** (4.2), to the buyer's new vault
5. the buyer's **account index record** (4.3), to the buyer's collectibles account

Plus dummy Actions up to the padding target (5). The output count is the same for every sale.

### 3.1 Public listing

The order book publishes only:

`{listing_id, coll, num, price, expiry_height, seller_channel_key}`

- No vault FVK, carrier note, record pointer, nullifier, or pay address appears in public. A public FVK would let anyone read the item's whole history and spot the sale tx.
- The listing page shows the artwork and an **R1 badge** (genuine issuance), both derivable from the public issuer FVK. R2 and R3 badges appear only for a buyer who received the disclosure (3.2).
- `seller_channel_key` is an ephemeral X25519 key held by the seller's browser for this listing.
- The List modal warns the seller: every buyer who receives the disclosure learns the item's full history and can republish it. The marketplace limits who receives it (3.2) but cannot prevent republication.

### 3.2 Buy flow

Messages between buyer and seller use HPKE (X25519, HKDF-SHA256, ChaCha20-Poly1305). Every message's associated data is the transcript
`T = H(listing_id, coll, num, price, expiry_height, seller_channel_key, buyer_channel_key, nonce)`.

1. **Intent.** The buyer's browser posts an intent for `listing_id` with its own ephemeral X25519 key and a proof-of-work stamp over `T`. The server enforces per-listing caps: at most 3 disclosures in flight, at most 10 per day. The seller's policy can tighten this (D8).
2. **Disclosure.** The seller's browser (an open tab auto-responds) encrypts `{vault FVK, record pointer, carrier note, pay address}` to the buyer's key. It also signs `T` with a RedPallas spend-authorization signature under the vault key, so the signature verifies against the `ak` inside the disclosed FVK. The server relays ciphertext only.
3. **Verify.** The buyer's browser checks the signature over `T` against the disclosed FVK and checks that `T` contains its own channel key. A relay that substituted either key is detected here, so the buyer aborts and reports. The relay may have read that one disclosure; it cannot alter it, because the next step checks everything against chain data. The buyer then reconstructs the provenance from the record pointer (4.2) and runs R1 to R3 against chain data (6.2). Only HELD continues. The note used from here on is the one decrypted from chain, not the disclosed copy.
4. **Build.** The buyer's browser checks that its deposit note is unspent, builds the PCZT, retries unproven builds until the carrier pair shares an Action, proves all Actions (the seller's Action needs only the disclosed note, its Merkle path and the vault FVK), signs the buyer spends, and redacts (3.3). It refuses to post if any redacted field is still present.
5. **Post.** The PCZT is encrypted to `seller_channel_key` with AAD `T` and posted.
6. **Accept.** The seller's browser runs the accept checks below, signs the carrier spend, extracts (this verifies the proof and every signature) and broadcasts.

Accept checks (as implemented in `carrier sale-accept` on 2026-10-02):
- the PCZT file is at most 1 MiB
- the carrier is still unspent, and the listing has not expired
- PCZT `expiry_height` is at least tip + 3, at most the listing expiry, and at most tip + 40
- the consensus branch id is the current one
- the PCZT carries the listing id of this exact listing version in its global proprietary field (added after the Codex re-review)
- the PCZT has only an Ironwood bundle, with exactly the sale shape's Action count (3 in the CLI, which has no record outputs yet)
- the carrier nullifier is spent in exactly one Action
- outputs to the listing's pay address, decrypted from the effects, sum exactly to the price (checked addition)

The seller does not check the buyer's successor. The buyer built it and verifies it; a buyer who burns its own purchase harms only itself.

The seller must be online for steps 2 and 6. An open tab handles both automatically.

### 3.3 Redaction before posting

As implemented in `carrier sale-buy` on 2026-10-02, the buyer clears:
- on its funding spends: FVK, witness, ZIP-32 derivation, recipient, value, rho, rseed, alpha (with alpha and the public `rk`, anyone could recover the buyer's `ak` and link its PCZTs), proprietary fields
- on every output it created (successor, payment, change; and the record outputs once P4 adds them): recipient, value, rseed, `ock`, ZIP-32 derivation, user address, proprietary fields
- on every Action: value randomness (`rcv`) and dummy spend keys

The successor and the records use the buyer's own OVK, so the seller cannot recover them from chain. Extraction still succeeds after this redaction (tested 2026-10-02 on the three-output CLI sale). A self-check searches the serialized PCZT for the buyer's FVK, funding-note rho and rseed, and the buyer's addresses, and refuses to post if any is found.

### 3.4 Race, reservation, expiry and the free option

- Two sales of one carrier spend the same nullifier, so at most one can be mined. A double sale is impossible by construction.
- **One active reservation per listing.** The seller's tab accepts the first valid PCZT and does not disclose to further buyers until it is mined or has expired. Later intents are told the listing is reserved. This avoids buyers depositing and proving for a sale that cannot happen.
- PCZTs carry `expiry_height`. Default: tip + 20 (about 25 minutes); the seller refuses more than tip + 40. The order book drops expired PCZTs.
- **The free option is real.** Until expiry, the seller alone decides whether to broadcast. The buyer can try to cancel by spending its deposit note to itself, but a cancellation takes effect only once it is mined, and the seller can still broadcast first and win the nullifier race. The UI says this plainly. It is inherent to a protocol where one party signs last. Short expiries bound it.
- Griefing (abandoned intents, spam PCZTs) is bounded by proof of work, per-listing caps, reservation and expiry. The buyer's principal is never at risk: either the sale completes atomically or the deposit stays the buyer's. Fees, deposit lockup time and price risk remain.

### 3.5 Marketplace fee (option, untested)

If the marketplace charges a fee, it is an extra output in the same sale tx to the marketplace's address. The order book relays a PCZT only if its own key decrypts a fee output of the right amount. The marketplace still never holds user funds. Decision D3.

## 4. Provenance

### 4.1 What a verifier needs

Per hop: the vault FVK. The verifier decrypts each note from chain with that FVK. Plus the public collection record and the item number.

### 4.2 Vault records: constant size per sale

Each sale adds one **vault record**, an output to the buyer's new vault whose memo holds:
- format tag and version; `coll`, `num`, the new hop number
- the seller's vault FVK (96 bytes)
- a pointer to the seller's own vault record: `(txid, action index)`, or the issuance marker `(genesis txid, item)` when the seller's carrier came from issuance
- the successor commitment `cmx`, which the verifier checks against the carrier Action of the same tx

Because the record sits at the buyer's vault address, the next buyer can decrypt it with the FVK it is given. Walking the pointers back, each record yields the FVK that decrypts the previous one, so one disclosed vault FVK plus one pointer reconstructs the whole history. The record has a fixed size, so the sale tx has the same number of outputs at every hop.

### 4.3 Account index records: seed-only recovery

Each purchase also adds an **account index record**, an output to the buyer's collectibles account (account 0) whose memo holds `{coll, num, vault_index}`.

Recovery from the wallet signature alone:
1. Re-derive `seed`, then account 0, and scan the chain with its one incoming viewing key.
2. Each account index record names an owned item and the vault index actually used. There is no guessing over `2^31` indices.
3. Derive each vault key, find its carrier and vault record, and walk the record chain (4.2).

Losing browser storage loses nothing, provided the chain and a lightwalletd are reachable.

## 5. Transaction shape

- **Padding.** Every sale tx is padded with dummy Actions to one fixed count. Because 3 and 4 fix the output count at 5, the count does not depend on history length. The target is chosen from the measured Ironwood Action-count distribution (P5).
- **Anchor age.** The anchor is a random recent height, not always the tip, so the anchor does not reveal when the tx was built. The allowed range is to be confirmed against consensus rules (P5).
- **Timing.** The site publishes no sale times. Listings are removed without a public "sold" event.

## 6. Security and privacy of the service

### 6.1 Browser code is the wallet

- Reproducible builds; the release hash is published and listed in the repo.
- Subresource Integrity on every script; strict CSP; no third-party scripts or analytics.
- COOP `same-origin` and COEP `require-corp` on every page (also needed for multithreaded WASM).
- Keys live in memory only. A CI test asserts that no request body ever contains key material.
- Everything received from the other party or the server is size-capped and parsed with checked arithmetic. The 2026-10-02 CLI does this on the sale and verify paths; the browser code must do it everywhere.
- Precedent: in June 2026 a browser-wallet marketplace sent users' private keys to its server and wallets were drained (ZecBit whitepaper p.176, unverified).

### 6.2 Chain data: what is trusted

Every verdict rests on chain data served by lightwalletd. The verifier (since 2026-10-02, `src/verifier.rs`):
- requires a returned transaction to hash to the requested txid
- requires block ranges to be contiguous and hash-linked
- requires a full spend transaction to match the compact block it was indexed from
- refuses duplicate nullifiers in served data
- with a second server (`LWD2`), requires every compact block in the scanned range and every fetched transaction to be identical on both (strengthened after the Codex re-review; comparing block hashes alone could not detect a hidden spend)

None of this validates proof of work. A colluding pair of servers could still serve a false view. Stage 1 therefore:
- uses two independent servers, ours and one third-party operator; a "verified" badge requires both to agree, and the UI shows the trust basis
- fetches block ranges, not single transactions, so lookups do not name the item being verified
- broadcasts through our relay so the node operator does not see user IPs; optional Tor guidance for users who want to hide their IP from us

Later: light-client header validation in the browser (proof-of-work and chain-work checks on headers), which removes server trust except for availability.

### 6.3 Content

- Artwork on chain cannot be deleted. The site can hide an item or a collection from its own pages and runs a report flow.
- A moderation policy is required before the public beta (D5).

## 7. Pages

| Route | Content |
|---|---|
| `/` Explore | collection cards: mosaic, supply (finalized or open), floor, listed count |
| `/collection/:coll` | stats header; grid with price badges; filters: listed only, sort by price |
| `/item/:coll/:num` | image from the on-chain memo; R1 badge; price; **Buy** or **List**. After the disclosure: R2 and R3 badges, the trust basis (6.2) and the provenance timeline, shown to that buyer only |
| `/account` | Collected, Listed, Activity (local only); collectibles balance; Deposit and Sweep; Restore |
| Connect modal | wallet detection, signature explainer, network badge |
| List modal | price, expiry, what will be disclosed and the republication warning (3.1) |
| Buy modal | price + fees, deposit step, progress (intent, verify, build, prove, sign, post), the free-option and cancellation notice (3.4) |
| Accept | automatic in an open tab; manual review screen otherwise |

## 8. Runtime requirements (measured)

- Multithreaded WASM: nightly Rust, `+atomics,+bulk-memory`, `wasm-bindgen-rayon`.
  - Measured on M4 Pro, 14 threads: ProvingKey 3.9 s once, 2 Actions 1.9 s, 4 Actions 3.6 s. Single-thread: 11.9 s and 21.8 s.
  - The padded sale (5 outputs plus spends, then padding) will be larger than 4 Actions. Measure after P5.
- Not yet verified: streaming `GetBlockRange` over grpc-web, and Ironwood compact actions on ChainSafe (HANDOFF task 3).

## 9. Prerequisites before BUILD

| ID | Item | State |
|---|---|---|
| P1 | Successor validity rule (OPEN_ISSUES #7), with the hop counter | done 2026-09-30, hop counter 2026-10-02 |
| P2 | CLI two-party PCZT sale on testnet | done 2026-09-30, tx `6bdeb823ff65…` (one local seed) |
| P3 | Redaction with extraction still succeeding; seller edit after buyer signature rejected; buyer verifies before paying | done 2026-10-02, DRY (`evidence/testnet-2026-10-02/`) |
| P4 | Vault records and account index records (4.2, 4.3); verifier walking record pointers; seed-only recovery test | to do |
| P5 | Measure the Ironwood Action-count distribution; padding and anchor-age rules | to do |
| P6 | Browser network: grpc-web streaming and Ironwood compact data (HANDOFF task 3) | to do |
| P7 | `carrier-core` crate for native and wasm32 (HANDOFF task 5) | done 2026-10-02 for formats, keys, decryption and rules R1 to R3 (bundle-based, no `zcash_primitives`); `verify.sh` builds it for wasm32. The PCZT sale path stays native until a wasm-capable C compiler is available for `secp256k1-sys` (pulled in by `zcash_primitives`) |
| P8 | Key derivation of 2.1 (collectibles account, vault index, collision rule) replacing the PoC `OFF` offsets | partly: `collectibles_seed` (HKDF, step 4) in `carrier-core::keys`; vault index allocation waits for proposal X2 |
| P9 | FINAL hard seal (OPEN_ISSUES #2) | to do, needs a new test collection and broadcast approval |
| P10 | Authenticated disclosure channel (3.2): HPKE messages bound to `T`, vault signature over `T` | partly: `vault_sign` / `vault_verify` (RedPallas, domain-separated) in `carrier-core::keys`; the encoding of `T` and HPKE wait for proposal X3 |
| P11 | A second, independent lightwalletd (our own node) for 6.2 | to do |
| P12 | Repeat the CLI sale with separately held seeds (`STATE` per party, `SALE_DIR` between them) | code ready 2026-10-02; run needs broadcast approval |

## 10. Decisions for Ryot

| ID | Decision | Draft 3 default |
|---|---|---|
| D1 | Approve this SPEC | approved 2026-10-02 |
| D2 | Sale proceeds to a per-listing collectibles address (unlinkable, then sweep) or directly to the main wallet (simpler, linkable) | per-listing address |
| D3 | Marketplace fee: none, or a fee output in the sale tx (3.5) | none for the testnet beta |
| D4 | Testnet beta prices: does a TAZ price count as "offering testnet assets" (CLAUDE.md rule 5)? | free transfers only, no TAZ prices, until decided |
| D5 | Content moderation policy | required before the public beta |
| D6 | Visual design direction | ask before any UI work |
| D7 | Mainnet, and legal review before any real value | out of scope for Stage 1 |
| D8 | Disclosure gating: proof of work and per-listing caps (3.2), or something stronger such as an off-chain proof of funds | proof of work + caps |

## 11. Acceptance (MVP, testnet)

1. Two Chromium profiles with the Noir testnet build (fresh test seeds) connect.
2. A lists item #1. The public listing JSON contains no FVK, note, pointer, nullifier or pay address (automated check).
3. B posts an intent, receives the disclosure, verifies A's signature over `T`, and sees R1 to R3 pass with both lightwalletd servers agreeing.
4. A relay test that swaps B's channel key is detected by B (automated check).
5. B deposits and clicks Buy; the PCZT is posted encrypted. The PCZT contains no buyer FVK, funding-note fields, buyer addresses or output plaintext (automated check).
6. A's tab accepts. **One** testnet tx contains both legs, the vault record, the account index record, and exactly the padding-target number of Actions.
7. After mining: B shows #1 in Collected. The CLI verifier agrees on the new owner.
8. Restore: B clears all browser storage, reconnects, and recovers #1 with its full provenance from chain alone (4.3).
9. Negative checks: B alone cannot broadcast; a tampered amount is refused by A; an expired PCZT is refused; a PCZT valid past the cap is refused; a tx A edits after B signed fails extraction; a stale listing is refused by both sides.
10. Playwright covers every page and modal. Screenshots go under `.harness/runs/`.

## 12. Out of scope (Stage 1)

Mainnet, WalletConnect, connect with wallets other than Noir (any wallet can still deposit by QR), auctions, offers, royalties, launchpad, naming.

## 13. Later stages

- **Stage 2:** propose carrier support to Vizor (Chainapsis). Keys stay inside the wallet.
- **Stage 3:** a carrier method set in Zcash WalletConnect (Noir team grant, approved 2026-08-31, not shipped).
- **Ask Noir:** origin-bound `signMessage` and a derived-key API, which would close the risk in 2.1.

## 14. Changes

### Draft 3 (2026-10-02), after the Codex review

- 3.2: disclosure is authenticated. HPKE messages bound to transcript `T`, a vault signature over `T`, and detection of relay key substitution. Intents need proof of work and are capped per listing (D8). Accept checks are listed as implemented.
- 3.1: the List modal warns that recipients can republish the history.
- 3.3: the redaction list now covers every buyer field (recipient, value, rho, rseed, alpha on spends; all output plaintext), as implemented and tested.
- 3.4: one active reservation per listing; cancellation is effective only when mined, and the seller's free option is stated plainly.
- 4.2 and 4.3: provenance is now a linked chain of constant-size vault records plus account index records, replacing the full FVK copy that grew with history. This also resolves the circular seed-recovery problem.
- 2.1: the vault index derivation, domain separation and collision rule are specified.
- 5: padding works because the output count is constant.
- 6.1: input caps and checked arithmetic. 6.2: the chain-data trust model, two independent servers, light-client headers later.
- 9: P3 done; P10 to P12 added. 10: D8 added. 11: acceptance updated.

### Draft 2 (2026-10-01)

- Public listings without FVKs, with disclosure only to a committed buyer; buyer-side redaction; FVK-only provenance in self-addressed memos; Action padding, anchor-age randomisation, no public sale times; browser code security, own lightwalletd, content moderation; per-listing pay address; prerequisites, decisions and acceptance tables.
- Prior art: the note-chaining primitive was published first by ZecBit's NSCV (2026-09-20). Carrier's distinct contributions are the non-custodial atomic sale and a working implementation.

## 15. Proposed changes for draft 4 (NOT approved)

From the P5 measurement (`research/p5-action-counts-2026-10-02.md`) and the Codex re-review (`docs/research/codex-review-2026-10-02-rereview.md`). Draft 3 stays the approved baseline until Ryot approves these.

| ID | Proposal | Why |
|---|---|---|
| X1 | 3-Action sale: put the vault record (4.2) in the successor carrier's memo and the account index record (4.3) in the change output's memo; keep a change output in every sale | 5 outputs means at least 5 Actions, which only 0.2% of testnet txs have; 3 Actions matches 2.6% and the CLI shape (P5) |
| X2 | Fresh vault per acquisition: allocate the next never-used vault index (a counter recovered from account index records), not `hash(coll, num)` | the hash rule reuses a vault whose FVK was disclosed when the item was sold, so a re-bought item would be visible to the previous buyer |
| X3 | Disclosure encoding: canonical length-prefixed `T` with domain `CarrierDisclosureV1`; sign `domain || T || H(payload)`; AAD carries version, message type, direction and a sequence number; single-use nonces with replay state | draft 3 leaves encodings, payload binding and replay open |
| X4 | Disclosure gating: offer an optional proof of funds (the buyer shows its confirmed deposit note to the seller with a payment-disclosure proof) in addition to proof of work and caps | proof of work and per-listing caps still let a scraper take one disclosure from every listing |
| X5 | Records: exact byte formats, zero value, versioned; accept an account index record only if the same tx moves a valid carrier to the named vault; reject duplicates and conflicts; cycle detection, hop and byte caps; recovery scans from a recorded birthday height and re-scans the last 100 blocks on reorg; freeze the exact bytes of `M` | prevents fake or cyclic records and makes seed recovery stable |
| X6 | Reservation at intent: the seller grants one short lease (for example 30 minutes) before disclosing, renewable and idempotent; others wait | draft 3 reserved only after a valid PCZT, after other buyers had already deposited and proved |
| X7 | Transaction lookups: memos need full transactions, which compact blocks do not carry. Fetch full transactions in privacy-sized batches through our relay, with decoys, and state the remaining leak until then | direct `GetTransaction` calls tell the server which item is being verified |
| X8 | Trust: keep the content comparison of 6.2 across two independent servers, and add light-client header validation before any real value | the content check needs a second independent server, and neither check validates proof of work |
