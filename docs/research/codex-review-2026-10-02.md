# Codex review, 2026-10-02

Reviewer: Codex CLI 0.144.5, model `gpt-5.6-sol`, reasoning effort `xhigh`, read-only sandbox.
Input: a clean clone of this repo at commit `5980218` (no `state/`; the seed was verified absent from every commit). Network use was not allowed.
Scope: commits `99f6fee`, `b37dd77`, `85df8e6`, `5980218`, and the docs' claims.

The review is kept verbatim below, except that absolute paths were rewritten to repo-relative paths. Fixes are tracked in `docs/HANDOFF.md` and in the commits that follow `5980218`.

---

## Overall verdict

The PCZT transaction is atomic at the Zcash-effects level: once the buyer has signed, the seller cannot redirect the buyer’s change or remove/alter the payment without invalidating the signature, and extraction verifies the proof and signatures. However, the current buyer path does not authenticate the collectible, the verifier accepts spent notes as current holdings, and verification trusts lightwalletd without validating the returned chain data. A malicious seller can therefore sell an invalid item, while a malicious buyer cannot underpay in a valid transaction but can bypass listing expiry, create an invalid successor, or cause denial of service. The successor-validity predicate itself is correctly implemented when the successor key is available, but `verify.sh` often skips that exact check.

## Findings

### 1. High: the buyer can pay for a counterfeit or unrelated note

- **Location:** [src/bin/carrier.rs:499](src/bin/carrier.rs:499), especially lines 505–529; marketplace SPEC §3.2 step 3.
- **What is wrong:** `sale_buy` checks only that the disclosed note matches its supplied `cmx`. It never runs R1–R3, validates the package, confirms the FVK owns the note, checks the note memo, or checks that the note is the live carrier for `(coll,item)`.
- **Failure scenario:** A seller edits `listing.json` to contain a live zero-value note controlled by the seller and labels it as a valuable item. The buyer proves and signs a payment transaction. `sale_accept` derives the matching nullifier, sees the exact payment, and signs. The transaction mines, but the received “carrier” cannot connect to the real genesis commitment, so the buyer paid for an invalid item.
- **Suggested fix:** Make sale construction consume a verified `VerifiedCurrentCarrier` object produced only by a complete R1–R3 check. Bind the collection, item, current commitment, note, FVK, price, pay address, listing ID, and expiry into one immutable listing object and revalidate immediately before signing.
- **Confidence:** High.

### 2. High: R3 reports spent notes as valid holdings and exits successfully

- **Location:** [src/bin/verify.rs:188](src/bin/verify.rs:188), particularly lines 196–198; [verify.sh:43](verify.sh:43).
- **What is wrong:** The verifier computes `live`, prints `unspent=false`, but still returns exit 0. `verify.sh` deliberately accepts the former seller’s already-spent carrier by matching only `R3 holder opens ... yes`. A package with zero hops also performs no holder or liveness check because `prev` is `None`.
- **Failure scenario:** A former owner presents a stale package and its old vault FVK. `verify` prints that the holder opens the carrier and exits successfully even though the carrier has already been sold.
- **Suggested fix:** Fail R3 when `live == false`, and make the gate require the exact string `unspent=true`. Represent the current note explicitly so an untransferred, zero-hop item also receives ownership and liveness checks. If historical verification is wanted, expose a separate R1/R2-only mode rather than weakening R3.
- **Confidence:** High.

### 3. High: “independent verification” trusts an unauthenticated lightwalletd view

- **Location:** [src/lib.rs:23](src/lib.rs:23), [src/lib.rs:146](src/lib.rs:146), `src/bin/verify.rs:31–47`; marketplace SPEC §§1 and 6.2.
- **What is wrong:** Returned transactions, compact blocks, heights, nullifiers, and tree states are trusted. `get_tx` does not even verify that the returned transaction’s computed txid equals the requested txid. The witness root is cross-checked against another response from the same server, not an independently validated header chain.
- **Failure scenario:** A malicious or compromised marketplace relay omits the current carrier’s spend and reports it as unspent, or substitutes fabricated transaction bytes and matching compact data. The browser displays a locally computed but server-controlled verdict.
- **Suggested fix:** At minimum verify requested txids and use independent servers with consistency checks. For the stated “do not trust our server” property, validate a header chain and block commitments or use a locally trusted full node. Document the lightwalletd trust assumption until then.
- **Confidence:** High.

### 4. High: private disclosure is neither meaningfully gated nor authenticated

- **Location:** [docs/marketplace/SPEC.md:63](docs/marketplace/SPEC.md:63), §3.1–3.2.
- **What is wrong:** A “committed buyer” is merely anyone posting an ephemeral X25519 key. An attacker or the server can submit intents for every listing and receive every provenance package and vault FVK. The protocol also does not specify authenticated key exchange or transcript binding, so a server can substitute buyer keys, decrypt a disclosure, and re-encrypt it.
- **Failure scenario:** A scraper posts one intent per listing and reconstructs the history of the whole listed inventory. Alternatively, the order-book server replaces the buyer’s ephemeral key before relaying the intent and silently reads the supposedly private disclosure.
- **Suggested fix:** Define what commitment means, such as a refundable bond or proof of a suitably funded deposit, and rate-limit identities rather than bare listings. Use authenticated HPKE/Noise-style messages bound to `listing_id`, both channel keys, price, collection, item, expiry, and nonces. Explicitly acknowledge that any prospective buyer who receives provenance can republish it.
- **Confidence:** High.

### 5. High: draft-2 redaction still exposes the buyer’s deposit and account linkages

- **Location:** [src/bin/carrier.rs:577](src/bin/carrier.rs:577); [docs/marketplace/SPEC.md:82](docs/marketplace/SPEC.md:82), §3.3.
- **What is wrong:** The current code clears only the funding FVK and witness. Draft 2 adds output redaction and `rcv`, but still leaves the buyer spend’s recipient, value, `rho`, `rseed`, and `alpha`. These fields reconstruct the spent note commitment, locating the deposit transaction; `alpha` plus public `rk` reveals the stable spend validating key. The successor also uses the seller vault’s OVK at line 523, allowing the seller to recover the buyer’s successor plaintext from chain.
- **Failure scenario:** The seller learns the buyer’s collectibles address, deposit value and transaction, and can link later PCZTs using the recovered `ak`, despite the claimed private flow.
- **Suggested fix:** After proof and buyer signature, clear every unnecessary buyer-spend field: recipient, value, `rho`, `rseed`, FVK, witness, `alpha`, ZIP-32 derivation, and proprietary metadata. Clear all private output metadata, including ZIP-32/user-address fields, and use no OVK or a buyer-controlled OVK for buyer-private outputs. Complete P3 with extraction and privacy regression tests.
- **Confidence:** High.

### 6. High: seed-only recovery is circular with hardened per-item keys

- **Location:** marketplace SPEC §2.1 step 5 and [§4.2](docs/marketplace/SPEC.md:108).
- **What is wrong:** Restoring the master seed does not reveal which hardened child index `hash(coll,num) mod 2^31` should be derived. Yet `(coll,num)` is inside a memo encrypted to that same undiscovered child key.
- **Failure scenario:** A user clears IndexedDB and restores the signature-derived seed. The wallet cannot know which of up to \(2^{31}\) item keys to derive, so it cannot decrypt the memo that would identify the item. “Losing local storage loses nothing” is therefore not established.
- **Suggested fix:** Store an encrypted item manifest under a discoverable account-level key, use a recoverable sequential index with authenticated metadata, or require and document a complete public collection registry that can be enumerated. Specify hash domain separation, encoding, and collision handling.
- **Confidence:** High.

### 7. Medium: seller checks protect payment but do not bind the PCZT to the complete listing

- **Location:** [src/bin/carrier.rs:602](src/bin/carrier.rs:602), lines 603–630.
- **What is wrong:** The carrier nullifier check is correct, the current-branch check is reasonable, and authenticated trial decryption to the exact seller address correctly checks payment effects. But `l.expiry` is never checked, the PCZT expiry has no upper bound, and the same-action output is not checked for value 0 or the expected HOP memo/item. Unexpected bundles/actions are unrestricted.
- **Failure scenario:** A buyer obtains disclosure before listing expiry, waits until the price is stale, then submits a PCZT with a far-future expiry. An auto-accepting seller signs it after the listing expired. A buyer can also make the carrier action output garbage, burning the item while paying the seller.
- **Suggested fix:** Require `tip + safety_margin <= tx_expiry <= listing.expiry`, recheck carrier liveness, cap PCZT size/action count, reject unexpected value pools, and validate the same-action successor semantics if preservation of the item is part of acceptance. Bind a listing hash/version into the local acceptance state.
- **Confidence:** High.

A malicious buyer still cannot produce a valid underpayment or redirect the checked payment: the seller signs only the matching action, and the buyer’s shielded signature commits all effects. A malicious seller likewise cannot edit effects after the buyer signs; extraction should reject the altered signature hash.

### 8. Medium: `verify.sh` does not test the successor rule or atomic payment in a fresh checkout

- **Location:** [verify.sh:19](verify.sh:19), lines 51–88.
- **What is wrong:** Without the private local seed, the forged package exits successfully as “successor validity unverified”, and the script treats that as success. The sale package similarly skips its final successor check. The “atomic sale” section checks the carrier’s txid but not the payment, amount, signatures, or PCZT.
- **Failure scenario:** CI prints `VERIFY.SH OK` after a regression that breaks last-successor validation or payment verification, because no public holder key or payment evidence is exercised.
- **Suggested fix:** Add offline unit tests for every `successor_valid` branch and full fixture tests with dedicated public test FVKs. Make verification modes explicit and do not call an unverified final successor a passing full verification. Add a raw-transaction/payment evidence checker if the sale claim is part of the gate.
- **Confidence:** High.

### 9. Medium: the sale evidence does not independently establish adversarial two-party operation

- **Location:** [evidence/testnet-2026-09-30/README.md:3](evidence/testnet-2026-09-30/README.md:3), lines 35–52; `src/bin/carrier.rs:32–56`.
- **What is wrong:** Buyer and seller accounts derive from the same local master seed. The PCZT and raw transaction are omitted, payment verification is described but not recorded in a reproducible artifact, and negative checks are a text transcript rather than executable fixtures.
- **Failure scenario:** A reviewer can confirm only that the code paths chose different accounts and that the recorded carrier chain names the sale tx. They cannot reproduce the PCZT checks or independently decrypt the recorded payment from repository contents.
- **Suggested fix:** Repeat with separately held seeds/processes; publish a sanitized fully redacted PCZT, raw tx, deterministic inspection script, and dedicated test viewing material where privacy permits. Describe the present result as “code-path role separation using one local seed,” not a hostile-party test.
- **Confidence:** High.

### 10. Medium: provenance growth conflicts with fixed padding and eventual feasibility

- **Location:** marketplace SPEC §§4.2 and 5.
- **What is wrong:** Copying every 96-byte historical FVK into approximately four-per-memo outputs adds \(O(n)\) Actions to the nth sale and \(O(n^2)\) cumulative chain data. A single fixed Action count cannot accommodate an unbounded history without either leaking history length or imposing a hard cap. Chunk numbering, total count, authentication, conflict handling, and association with the current carrier are also unspecified.
- **Failure scenario:** After 100 hops, roughly 25 provenance outputs are needed, before carrier, payment, change, funding and padding Actions. The stated browser timings and common-count padding no longer apply.
- **Suggested fix:** Use an incremental authenticated record or hash-linked encrypted chunks rather than copying the full history each time. Specify chunk indices/count/root and bind records to the current carrier commitment. Define size limits and privacy buckets before claiming a fixed transaction shape.
- **Confidence:** High.

### 11. Medium: cancellation is a race and the seller retains a real free option

- **Location:** marketplace SPEC §3.4.
- **What is wrong:** Spending the deposit to oneself does not cancel an already signed sale until the conflicting cancellation confirms. The seller can respond by immediately broadcasting the sale. Multiple buyers can also deposit and prove transactions while only one sale can mine.
- **Failure scenario:** The buyer broadcasts a cancellation after a price change. The seller sees or anticipates it, broadcasts the signed PCZT, and the sale wins the nullifier race. Other buyers lose fees, time, and privacy even though their principal is not mined.
- **Suggested fix:** State that cancellation is effective only after confirmation, use shorter bounded expiries and one active reservation, and disclose the economic option plainly. If removing the seller option matters, redesign signing order or accept that a symmetric two-party protocol generally gives the last signer an option.
- **Confidence:** High.

### 12. Medium: untrusted listings and PCZTs reach unchecked arithmetic and unwraps

- **Location:** `src/bin/carrier.rs:472, 513, 519, 523, 535, 553, 623`; [src/lib.rs:191](src/lib.rs:191); `src/bin/verify.rs:105, 175–180`.
- **What is wrong:** Examples include `pay_value + fee(3)`, `hop + 1`, `tip_h - 100`, `note_h - 1`, `Zatoshis::from_u64(...).unwrap()`, unchecked JSON `as_str().unwrap()`, CLI argument indexing, bundle unwraps, and `sum::<u64>()` over attacker-controlled decryptable payment outputs. `fs::read` and PCZT action counts are also unbounded.
- **Failure scenario:** A malicious listing with an extreme price/hop/height crashes the buyer. A crafted PCZT with many or huge decryptable seller outputs can panic in checked builds or wrap the sum in release; extraction should later reject an invalid transaction, so this is primarily denial of service rather than theft.
- **Suggested fix:** Use checked addition/subtraction and fallible amount conversions, validate all listing fields, replace input-dependent unwraps, cap PCZT/file/action sizes, and accumulate payment into `u128` or with `checked_add`.
- **Confidence:** High.

### 13. Low: R1 does not enforce the carrier and sequence-note definitions

- **Location:** [src/bin/verify.rs:59](src/bin/verify.rs:59), especially lines 68–85; `docs/SPEC.md:5–8`.
- **What is wrong:** R1 accepts any issuer-OVK output with a matching `CARRIER` memo regardless of its note value. Sequence and FINAL values are likewise unchecked, and exactly one sequence successor is not enforced. R2 also ignores the hop counter, although the stated successor rule does not currently require it.
- **Failure scenario:** An issuer places a value-bearing `CARRIER` in the legitimate issuance transaction and `verify` includes it despite the core definition requiring zero value.
- **Suggested fix:** Validate zero values and canonical memo fields for CARRIER, SEQ and FINAL notes; reject zero or multiple matching sequence notes. Decide explicitly whether `hop` must increment and enforce it if it has protocol meaning.
- **Confidence:** High.

### 14. Medium: several documentation claims exceed the evidence

- **Location:** `README.md:18–29,53`; `docs/OVERVIEW.md:9,84–86,189–194`; `docs/VISION.md:9,22–26,42,50–54`; `docs/research/zecbit-whitepaper.md:16,30–32,38`; `docs/HANDOFF.md:45–52`.
- **What is wrong:** “Every transfer is enforced by the zk proof” conflates the circuit-enforced `rho` link with verifier-enforced memo/value validity. “Verify checks everything” ignores its optional holder key, fail-open R3, and lightwalletd trust. “Removes its private data” is false for the current PCZT. “Re-check all of this” does not cover the double-spend rejection, payment, negative PCZT cases, or hostile-party isolation.
- **Failure scenario:** A reader treats `VERIFY.SH OK` or the mined happy-path transaction as evidence that the marketplace protocol, privacy, adversarial sale flow, and all three attacks were independently reproduced.
- **Suggested fix:** Use narrower language: “demonstrated one mined happy-path transaction with code-separated roles sharing a local test seed”; distinguish consensus-enforced linking from verifier policy; list exactly which assertions require private FVKs or trusted lightwalletd; call privacy/redaction/recovery unproven prerequisites.
- **Confidence:** High.

## Things done well

- The successor predicate itself checks value 0, HOP kind, collection and item, and returns immediately with a burned verdict so later hops are ignored.
- Same-Action selection is based on the action revealing the real nullifier, not a seller-provided successor commitment.
- PCZT signing uses the shielded sighash over transaction effects; the seller key is checked against the action’s randomized verification key, and extraction verifies proof and signatures.
- Seller payment checking uses authenticated trial decryption and compares both the exact recipient and exact aggregate amount.
- The repository repeatedly labels the project testnet-only and unaudited, and draft 2 explicitly records several unverified prerequisites such as redaction, padding, anchor age and FINAL sealing.

## Could not check

- I did not run `verify.sh` or access testnet lightwalletd because network use was prohibited.
- I did not run `cargo build` because the review was read-only and compilation would write build artifacts. `bash -n verify.sh` and `git diff --check 99f6fee^..5980218` passed.
- The PCZT, raw sale transaction, seller payment FVK, and local seed are not public evidence, so I could not replay the sale or independently decrypt its payment.
- The original ZecBit whitepaper is not in the workspace, so its quoted page claims could not be checked; the repository itself labels them unverified.
- I inspected the relevant behavior in the permitted `orchard-0.15.5`, `zcash_primitives-0.30.1`, and `pczt-0.9.3` sources.