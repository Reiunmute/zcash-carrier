# Codex re-review, 2026-10-02

Reviewer: Codex CLI 0.144.5, model `gpt-5.6-sol`, reasoning effort `xhigh`, read-only sandbox.
Input: a clean clone at commit `7e42a9d` (no `state/`; the seed was verified absent from every commit). Network use was not allowed.
Scope: the fixes for the first review (`codex-review-2026-10-02.md`) in commits `19d5d1d`, `d8c9d43`, `7e42a9d`.

Kept verbatim below, except that absolute paths were rewritten to repo-relative paths. Follow-ups: code-level items in the commit after `64a31ce`; design-level items as proposed SPEC changes in `docs/marketplace/SPEC.md` section 15.

---

## Overall verdict

The fixes materially improve the PoC, but the claim that all 14 findings are fixed is not supportable. My strict count is **3 fixed and 11 partly fixed**.

The largest remaining correctness issue is the `LWD2` model: it compares only server-supplied block hashes, not compact-block contents or transaction inclusion. A malicious primary lightwalletd can still hide a spend and obtain a HELD verdict while matching an honest second server’s hashes.

Draft 3 is directionally better. HPKE plus a vault-key signature is feasible, and linked records remove quadratic provenance growth. It is not yet implementation-ready: deterministic vault reuse leaks reacquisitions, transcript/record encodings are incomplete, reservation happens too late, recovery authenticity is underspecified, and the claimed two-server agreement is ineffective.

## Previous findings

| # | Status | Reason |
|---:|---|---|
| 1 | Partly fixed | `sale_buy` now requires `HELD` and uses the carrier decrypted from chain ([carrier.rs:508](src/bin/carrier.rs:508), [carrier.rs:561](src/bin/carrier.rs:561)), but the expected collection/item/listing identity is still taken entirely from the unsigned listing file, which has no `listing_id` or immutable listing hash ([carrier.rs:434](src/bin/carrier.rs:434)). |
| 2 | Fixed | Liveness now produces `Spent`, CLI exit 4, including zero-hop packages with a holder key; the gate asserts `unspent=false` and `VERIFY SPENT` ([verifier.rs:245](src/verifier.rs:245), [verify.rs:77](src/bin/verify.rs:77), [verify.sh:47](verify.sh:47)). |
| 3 | Partly fixed | Txids, range linkage, duplicate nullifiers, and compact/full spend consistency were added ([lib.rs:161](src/lib.rs:161), [verifier.rs:43](src/verifier.rs:43)), but `LWD2` compares only hash fields and still does not establish the served contents or inclusion. |
| 4 | Partly fixed | Draft 3 adds HPKE, transcript binding, a vault signature, limits, and a republication warning ([marketplace/SPEC.md:71](docs/marketplace/SPEC.md:71), [marketplace/SPEC.md:76](docs/marketplace/SPEC.md:76)). PoW plus per-listing caps still lets a scraper request one disclosure from every listing, and P10 is unimplemented. |
| 5 | Fixed | The funding spend’s identifying fields, all output plaintext metadata, `rcv`, and dummy keys are cleared, and the successor uses the buyer’s OVK ([carrier.rs:588](src/bin/carrier.rs:588), [carrier.rs:650](src/bin/carrier.rs:650)). The self-check is only a spot-check, but the actual redactor calls cover the identified fields. |
| 6 | Partly fixed | Account index records remove the circular need to guess `2^31` children ([marketplace/SPEC.md:138](docs/marketplace/SPEC.md:138)), but recovery formats and validation remain unimplemented, `M` is only an example, and the vault-index rule reuses a disclosed key on reacquisition. |
| 7 | Partly fixed | Seller checks now cover liveness, listing and PCZT expiry, branch ID, value pools, action cap, and checked payment totals ([carrier.rs:728](src/bin/carrier.rs:728)). There is still no listing hash/version, no exact expected transaction shape, and the successor is explicitly not checked ([marketplace/SPEC.md:97](docs/marketplace/SPEC.md:97)). |
| 8 | Fixed | A seed-free checkout now exercises genuine and invalid final successors, HELD/SPENT/BURNED/UNVERIFIED exit codes, and the payment in the same sale tx using published FVKs ([verify.sh:35](verify.sh:35), [verify.sh:47](verify.sh:47), [verify.sh:62](verify.sh:62), [verify.sh:68](verify.sh:68), [verify.sh:81](verify.sh:81)). |
| 9 | Partly fixed | Raw tx bytes, payment FVK, a redacted PCZT, and additional logs were added. The mined transaction still used one seed, the separate-seed run remains pending, and negative checks remain non-executable transcripts ([testnet-2026-09-30/README.md:3](evidence/testnet-2026-09-30/README.md:3), [testnet-2026-10-02/README.md:37](evidence/testnet-2026-10-02/README.md:37)). |
| 10 | Partly fixed | Linked vault records make new chain data O(1) per sale instead of copying all prior FVKs ([marketplace/SPEC.md:128](docs/marketplace/SPEC.md:128)). Encoding, validation, cycle handling, dust records, recovery, and implementation are still P4. Verification remains O(number of hops), not constant-size. |
| 11 | Partly fixed | The cancellation race, free option, short expiry, and reservation are now disclosed ([marketplace/SPEC.md:110](docs/marketplace/SPEC.md:110)). The stated reservation starts only after a valid PCZT, too late to stop other buyers from already receiving disclosure, depositing, and proving. |
| 12 | Partly fixed | Checked price/change arithmetic, amount validation, PCZT/action caps, and safer argument handling were added. `verify` still loads unbounded files, file caps have a metadata/read race, and server heights still reach unchecked additions such as `tip_h + 1` and `tip_h + 3` ([verify.rs:17](src/bin/verify.rs:17), [carrier.rs:475](src/bin/carrier.rs:475), [carrier.rs:602](src/bin/carrier.rs:602), [carrier.rs:735](src/bin/carrier.rs:735)). |
| 13 | Partly fixed | R1 now enforces zero value, exactly one sequence note, carrier hop 0, and duplicate item rejection; R2 enforces hop increments ([verifier.rs:130](src/verifier.rs:130), [verifier.rs:264](src/verifier.rs:264)). It still does not enforce canonical SEQ/FINAL counters or payloads, or the specified 386-byte CARRIER artwork format. |
| 14 | Partly fixed | The major claims were narrowed and the one-seed, DRY, holder-key, and lightwalletd caveats are now prominent. Several remaining overstatements and stale sections are listed below. |

## New findings

### 1. High: `LWD2` does not cross-check the data that determines a verdict

**Location:** [src/verifier.rs:88-108](src/verifier.rs:88), [src/lib.rs:161-176](src/lib.rs:161), [marketplace/SPEC.md:166-180](docs/marketplace/SPEC.md:166).

**Problem:** The second server is asked only for `CompactBlock.hash`. Compact transactions, nullifiers, commitments, txids, raw transactions, and transaction inclusion are not compared. A dishonest primary can reuse honest block hashes while omitting or fabricating compact contents. Relevant facts at the primary tip are additionally excluded by `filter(|h| *h < ch.to)`.

**Failure scenario:** The primary omits the current carrier’s spend but serves the genuine block hashes. R3 returns HELD. The honest second server returns the same hashes, so `cross_check` reports agreement.

**Suggested fix:** Compare complete relevant compact contents against the second server, require every fetched raw transaction to appear at its claimed height on both servers, compare raw bytes/txids, and include tip-height facts. Until then, report “primary view with hash-field comparison,” not “both servers agree.” Header and chain-work validation or a trusted full node remains the real fix.

**Confidence:** High.

### 2. High: reacquiring an item reuses a vault FVK that was already disclosed

**Location:** [marketplace/SPEC.md:35-37](docs/marketplace/SPEC.md:35), [marketplace/SPEC.md:138-147](docs/marketplace/SPEC.md:138).

**Problem:** `vault_index` is deterministic from `(coll,num)`, and the collision rule advances only if the index belongs to “another item.” After a user sells an item, that vault FVK is disclosed. Reacquiring the same item selects the same vault.

**Failure scenario:** Alice sells item #1 to Bob, disclosing her FVK. Alice later buys #1 again. Bob, and anyone to whom he republished the FVK, can decrypt Alice’s new carrier and vault record and observe its later spend.

**Suggested fix:** Allocate a fresh, never-before-used vault index for every acquisition epoch. Account index records should preserve all historical allocations so recovery can avoid reuse. Add atomic local reservation for simultaneous purchases and define wraparound/exhaustion.

**Confidence:** High.

### 3. Medium: the authenticated-disclosure construction is feasible but not protocol-complete

**Location:** [marketplace/SPEC.md:76-86](docs/marketplace/SPEC.md:76), [marketplace/SPEC.md:220](docs/marketplace/SPEC.md:220).

**Problem:** `H`, field encodings, domain separation, HPKE mode/`info`, nonce ownership and uniqueness, replay state, and message types are unspecified. The vault signature covers only `T`, not a hash of the disclosure payload. PoW and per-listing caps do not meaningfully stop a Sybil scraper from requesting one disclosure per listing.

A RedPallas signature verifiable against the FVK’s `ak` is feasible with the permitted Orchard API, so the construction is not inherently impossible.

**Failure scenario:** Independent implementations encode `T` differently, or replay an old message under the same transcript. Even when authentication works, one cheap intent per listing reconstructs the history of the entire listed inventory.

**Suggested fix:** Define canonical length-prefixed bytes and a domain such as `CarrierDisclosureV1`; sign `domain || T || H(disclosure)`; include direction, message type, version, and sequence in AAD; require single-use nonces and explicit replay state. If disclosure scarcity is a real privacy goal, use proof of funds or a refundable bond, not only PoW.

**Confidence:** High.

### 4. Medium: recovery records need authenticity, filtering, and exact encodings

**Location:** [marketplace/SPEC.md:128-147](docs/marketplace/SPEC.md:128), [marketplace/SPEC.md:39-45](docs/marketplace/SPEC.md:39).

**Problem:** The linked-record concept is sound, but values, exact memo bytes, pointer encoding, creation constraints, duplicate selection, cycle/depth limits, and validation of unsolicited records are not defined. Anyone who learns a collectibles-account address can send it fake account-index memos. `M` is merely “for example,” so seed-only recovery lacks a stable byte-level root.

**Failure scenario:** Recovery encounters dust records naming arbitrary indices or a cyclic pointer chain and spends unbounded work. A domain/message change after deployment derives a different seed and makes “losing browser storage loses nothing” false.

**Suggested fix:** Freeze exact `M` bytes and signature decoding; require zero-value, versioned canonical records; bind each account record to a valid same-transaction carrier successor; reject duplicates/conflicts; use visited sets and hard hop/byte caps; specify reorg behavior and scan start. Complete P4/P8 before accepting the recovery claim.

**Confidence:** High.

### 5. Medium: “block ranges, not single transactions” does not fit memo verification

**Location:** [marketplace/SPEC.md:177](docs/marketplace/SPEC.md:177), [src/lib.rs:161](src/lib.rs:161), [src/verifier.rs:139](src/verifier.rs:139).

**Problem:** Compact Ironwood actions support compact trial decryption but do not carry the complete 512-byte memo. The present verifier necessarily calls `GetTransaction` for genesis and every provenance transaction. Those requests name the exact txids being inspected.

**Failure scenario:** An honest-but-curious lightwalletd operator links a client IP and request timing to an item’s complete provenance despite the claimed range-only lookup privacy.

**Suggested fix:** Specify a full-transaction batching or relay design, fetch all transactions in privacy-sized ranges, or use authenticated full blocks/decoys. State plainly that current direct `GetTransaction` calls leak the inspected txids.

**Confidence:** High.

### 6. Medium: the reservation begins after the work it claims to prevent

**Location:** [marketplace/SPEC.md:81-86](docs/marketplace/SPEC.md:81), [marketplace/SPEC.md:110-116](docs/marketplace/SPEC.md:110).

**Problem:** The seller reserves after receiving the “first valid PCZT,” but a buyer must already receive disclosure, deposit, build, prove, and sign before producing that PCZT. Up to three disclosed buyers can therefore do the expensive work simultaneously.

**Failure scenario:** Three buyers deposit and prove. One PCZT arrives first; the other two have already paid a deposit fee, disclosed interest, and spent proof time.

**Suggested fix:** Establish one seller-authorized reservation when accepting an intent and before disclosure. Give it a short lease, explicit renewal/expiry, and idempotent state. Retain the accurate warning that the last-signing seller still owns a bounded free option. Replace “No funds are at risk” with the narrower atomic-principal guarantee and disclose fees, lockup, and price risk.

**Confidence:** High.

### 7. Medium: the payment/evidence gate does not validate the published raw transaction

**Location:** [verify.rs:21-31](src/bin/verify.rs:21), [verify.sh:81-84](verify.sh:81).

**Problem:** Payment mode validates a transaction returned by one lightwalletd but performs no inclusion or `LWD2` check. `sale_tx.hex` is checked only for a 24,644-character length; any same-length replacement passes.

**Failure scenario:** The fixture is corrupted or replaced while the gate continues to pass using remote bytes. A malicious lightwalletd can also label a known raw transaction as mined without proving inclusion.

**Suggested fix:** Parse `sale_tx.hex` offline, compute and assert its txid, verify payment from those exact bytes, compare them byte-for-byte with the server response, and confirm compact inclusion on both servers.

**Confidence:** High.

### 8. Low: deposit liveness and input-hardening still have gaps

**Location:** [carrier.rs:508-512](src/bin/carrier.rs:508), [carrier.rs:577-580](src/bin/carrier.rs:577), [verify.sh:77-79](verify.sh:77).

**Problem:** `sale_buy` indexes nullifiers only from the collection genesis, then uses that index to decide whether the funding note is unspent. A funding note spent before genesis will be reported live. `verify` input files remain unbounded. The no-holder gate also inherits an externally exported `HOLDER_FVK`, making that assertion environment-sensitive.

**Failure scenario:** The buyer proves and signs an already-spent old funding note; seller extraction can succeed, but node broadcast fails as a duplicate nullifier. A large package can exhaust memory. `HOLDER_FVK=... bash verify.sh` unexpectedly fails its intended no-key test.

**Suggested fix:** Index from `min(genesis_height, funding_height)`, cap every external file after opening/while reading, use checked height arithmetic, and run the no-key test with `env -u HOLDER_FVK`.

**Confidence:** High.

### 9. Low: documentation is improved but still overstates evidence

**Location:** [HANDOFF.md:52](docs/HANDOFF.md:52), [marketplace/SPEC.md:101-108](docs/marketplace/SPEC.md:101), [marketplace/SPEC.md:163](docs/marketplace/SPEC.md:163), [OPEN_ISSUES.md:21](docs/OPEN_ISSUES.md:21), [OPEN_ISSUES.md:58](docs/OPEN_ISSUES.md:58).

**Problem:** `HANDOFF` says all 14 findings are addressed; draft 3 describes record-output redaction as implemented/tested even though the CLI creates only three outputs and P4 is pending; “no panics on untrusted input (as in the CLI)” is too strong. `OPEN_ISSUES` still says atomic payment is separate and that R2 “currently accepts any output,” and points `successor_valid` to the old file.

**Failure scenario:** A future implementer treats untested record redaction, two-server agreement, or recovery as established evidence.

**Suggested fix:** Say “3 fixed, 11 partially addressed”; distinguish current three-output redaction from future record outputs; mark HPKE, records, padding, separate seeds, and second-server content comparison unverified; clean the closed issue text.

**Confidence:** High.

## Could not check

- I did not run `verify.sh`, contact lightwalletd, or verify current testnet state because network access was prohibited.
- I did not run `cargo build` or tests because the workspace is read-only and compilation writes `target/`. There are no repository unit tests for the new verifier helpers.
- I could not independently parse and authenticate `sale_tx.hex` or `example_redacted.pczt` without building tooling. Their claims remain supported by static code and recorded logs, not independently replayed here.
- The separate-seed sale, HPKE channel, linked records, seed-only recovery, padding, and second independent lightwalletd are explicitly unimplemented or unrun.
- Safe offline checks: `git diff --check 5980218..HEAD` and `bash -n verify.sh` passed. `cargo fmt --all -- --check` failed because the current Rust sources are not rustfmt-clean. The worktree remained clean.