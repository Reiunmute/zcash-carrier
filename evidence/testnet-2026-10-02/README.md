# Testnet run 2026-10-02: hardened sale flow, separate-seed sale, FINAL hard seal

Follow-up to the Codex review (`docs/research/codex-review-2026-10-02.md`). The sections up to "Re-run after the Codex re-review" are DRY runs (nothing broadcast). The section "Broadcasts" records four transactions Ryot approved and ran with `scripts/run_p9_p12.sh`.

Roles (`STATE=state OFF=10`, one local test seed): seller vault acct6 (holds item #1 since the 2026-09-30 sale), seller pay key acct5, buyer funding acct0 (change note of `6bdeb823…`), buyer new vault acct7.

## What changed in the flow

- **The buyer verifies before paying.** `sale-buy` runs R1 to R3 from chain data with the listed vault key and refuses anything that is not HELD. It uses the note it decrypted from chain, never the note in the listing.
- **Full redaction.** The buyer clears its funding spend's FVK, witness, ZIP-32 path, recipient, value, rho, rseed and alpha; the plaintext of the successor, payment and change outputs; and every Action's value randomness and dummy keys. A self-check searches the PCZT bytes for the buyer's FVK, funding-note rho and rseed, change address and new vault address, and refuses to write the file if any is present.
- **The successor uses the buyer's own OVK**, so the seller cannot recover the buyer's new vault address from chain.
- **Seller checks** now also cover: carrier still unspent, listing not expired, PCZT expiry no later than the listing expiry or tip + 40, Ironwood only, at most 16 Actions, checked payment sum.
- **Per-listing pay address**: a fresh diversified address of the pay key for each listing.

## Results

`happy_path.log`:
- A former holder (acct3) cannot list item #1: `REFUSED: the carrier is already spent`.
- Seller acct6 lists; buyer verifies (HELD), builds, proves, signs, redacts. The redaction check passes and the redacted PCZT still parses for the seller's signer. The seller's checks pass and extraction succeeds (proof and all signatures verified). DRY txid `6b3c3425…`, not broadcast.

`negative_checks.log`:

| Case | Result |
|---|---|
| C. Seller flips one byte of the payment ciphertext after signing | extraction refused: `SighashMismatch` |
| D. Buyer pays 9,000 against a 10,000 listing | seller refuses |
| E. Expired PCZT | seller refuses |
| F. PCZT valid for 100 blocks | seller refuses (cap is tip + 40) |
| G. Listing expired, buyer side | buyer refuses |
| G2. Listing expired, seller side | seller refuses |
| H. Listing names a different carrier commitment | buyer refuses |
| H2. Listing names the former holder's key | buyer refuses (holder cannot open current carrier) |
| I. A different seller key tries to accept | refused |

Case C answers the item left unverified on 2026-09-30: a transaction edited after the buyer signed does not extract.

## Files

- `happy_path.log`, `negative_checks.log`: command output. The scratch exchange directory is shown as `<SALE_DIR>`.
- `example_redacted.pczt`: the buyer's PCZT from the happy path, after redaction. It carries only the buyer's signature, so it cannot become a transaction without the seller's key, and it expired at height 4433741. The fully signed DRY transaction is deliberately not published, because a signed transaction stays broadcastable until its expiry.

## Limits

- The DRY runs above derive buyer and seller from one local seed. The broadcast below uses separate seeds.
- Chain data came from one lightwalletd (`testnet.zec.rocks`). `LWD2` cross-checking is implemented, but no independent native-gRPC testnet server was reachable on 2026-10-02 (ECC's endpoint timed out; ChainSafe serves grpc-web only).

## Re-run after the Codex re-review (`rerun_after_rereview.log`)

The re-review (`docs/research/codex-review-2026-10-02-rereview.md`) counted 3 of the 14 first-review findings fixed and 11 partly fixed. Code-level follow-ups made the same day, all re-tested here (DRY):
- **Listing version binding**: the buyer writes a listing id (BLAKE2b-256 of the listing) into the PCZT's global proprietary field, and the seller refuses any other value. Case J: a listing changed after the buyer built is refused.
- **Exact shape**: the seller accepts exactly 3 Actions, the CLI sale shape.
- **Funding liveness**: the buyer's nullifier scan starts no later than the funding note's height.
- **Bounded input and checked heights** on the sale and verify paths (the older issuance subcommands are unchanged).
- **`LWD2` compares content**: every compact block in the scanned range (txids and Ironwood actions) and every fetched transaction must be identical on both servers. Case K exercises this code path with the same host as both servers, which is not an independent check.
- **R1 canonical memos**: empty sequence payloads, a step counter, FINAL naming the item count, 386-byte v0 artwork, item numbers from 1.

All of cases A to K behave as expected.

## Broadcasts (approved by Ryot, run with `scripts/run_p9_p12.sh`)

| Step | Tx | Height | What |
|---|---|---|---|
| A | `7b89f969b2bc531d65257f48b4f60b5f47f9e904101a1caf4fa8a09b332ab33f` | 4435125 | 300,000 zats from the main seed's acct0 to a separately held buyer seed (`state/buyer`) |
| B | `281788d78f3e8ab39b208373c9810d0e0383c2cf54beb1b342dfe5bb03c4bf98` | 4435126 | **P12**: sale of item #1 from the main seed's acct6 to the buyer seed's acct6, 10,000 zats to a per-listing address of the main seed's acct5, one tx, 3 Actions |
| C1 | `7eab6bd6bee43d321e800d7a4565dc26d8ce9af29c2045f7fc15f9f8716aac49` | 4435127 | genesis of a new test collection `5fc17ef389fdae9b` (issuer acct31, item #1 to acct32) |
| C2 | `512832b969c897752116be1649d5a982ad2a82e34199bc1d1e9f11bfef9b68f3` | 4435129 | **P9**: FINAL sent to the collection's seal address, item #2 to acct34 |

P12, a sale between two parties whose keys never shared a seed: the buyer process (`STATE=state/buyer`) and the seller process (`STATE=state`) exchanged only the listing and the PCZT through `<SALE_DIR>`. Both still ran on one machine.
- `broadcast/B_buy.log`: the buyer verified item #1 from chain (HELD) before paying; the redaction check passed.
- `broadcast/B_accept.log`: all seller checks passed (carrier unspent, listing open, expiry, branch, 3 Actions, payment 10,000).
- `broadcast/V_holder.log`: the buyer seed's acct6 holds item #1 (HELD, hop 2 successor valid). `V_oldholder.log`: the seller's acct6 cannot open it. `V_payment.log`: 10,000 zats to the seller's pay key.

P9, FINAL hard seal: `broadcast/V_sealed.log` shows `R1 issuance set: 2 item(s), finalized=true, sealed=true`. The FINAL note's address is `seal_address(coll)`, whose transmission key is a hash-to-curve point, so no one knows a key that can spend it, and the sequence chain can never continue.

Files: `broadcast/*.log` (command output, exchange directory shown as `<SALE_DIR>`), `broadcast/txids.txt`, `package_item1_hop3.json` (provenance for the new holder), `collection_sealed.json` and `final_sealed.json` (the sealed collection's public record), `sale_tx.hex` (raw bytes of tx B), `viewing_keys.json` (the buyer seed's viewing keys, view only).

`verify.sh` re-checks all of this: the new holder (HELD), the former holder (SPENT or cannot open), the payment from `sale_tx.hex`, and `sealed=true`.

## P4 DRY: vault records and account index records (`p4_dry_records.log`)

Marketplace SPEC draft 4, sections 4.2 and 4.3. A DRY sale of item #1 of the sealed collection `5fc17ef389fdae9b`: seller `STATE=state OFF=30` acct2 (index 32), buyer the separate seed's fresh vault acct7 (index 17). Nothing was broadcast.
- The successor's HOP memo carries a 135-byte v1 vault record (the seller's vault FVK, pointer kind issuance, the genesis tx and Action). The buyer's change memo (250,000 zats) has kind `ACCOUNT` (5) with vault index 17. Decoded with `examples/memos.rs`.
- `verify ... walk:1:<txid>` with the buyer's vault key and `WALK_TX_FILE` rebuilds item #1's provenance from the record alone: one hop to issuance, equal to the listed vault key and carrier note. It then fails at R2 (`hop 0: carrier not spent on chain`) because the DRY tx is not mined. The full verdict needs the broadcast in `scripts/run_p4.sh`.
- Refusals: the seller's key opens no carrier in the DRY tx; a walk from the earlier separate-seed sale stops at once (`hop 3: no vault record`), because records did not exist yet.
- `carrier recover` on the buyer seed finds no account index records on chain yet (`next vault index: 1`), as expected before the broadcast.

## P8 DRY: automatic vault index (`p8_auto_vault_dry.log`)

`sale-buy` with `BUYER_VAULT_ACCT=auto` on the buyer seed (`BIRTHDAY=4435100`): the first purchase takes vault index 1 (no account index records yet, and index 1 never received a note) and reserves it; a second purchase takes index 2, and its change memo records 2. Separately, with the reservation set to 16, allocation skips 16 (it holds item #1 since tx `281788d7…`) and returns 17. Nothing was broadcast. The local reservation file was deleted after the test, because these DRY PCZTs are discarded.

## P10 DRY: the sale over the disclosure channel (`p10_channel_dry.log`)

The CLI sale now follows SPEC 3.1 and 3.2. `listing.json` holds only `{listing_id, coll, num, price, expiry_height, seller_channel_key}`; the vault key, carrier location, pay address and package travel in a vault-signed disclosure encrypted to the lease holder, and the PCZT travels encrypted to the seller's channel key. Seller `STATE=state OFF=30` acct2, buyer the separate seed's acct7; nothing was broadcast.

| Case | Result |
|---|---|
| A. List, intent, disclose, buy, accept (DRY) | extraction succeeds |
| B. A second buyer's intent while the lease is held | `RESERVED` until the lease ends |
| C. Buyer 1's intent replayed | refused: nonce already used |
| D. One byte of the encrypted PCZT altered | refused: does not open |
| E. Accept at the lease end (`NOW=until`) | refused: lease expired |
| F. Seller edits the tx after signing | extraction refused (`SighashMismatch`) |
| G. Relay lowers the price; with the id left as is, then with the id recomputed (independently, Python `hashlib`) | buyer refuses the first; seller refuses the second's intent |
| H. Relay replaces the buyer's intent with its own key | seller discloses to the relay's transcript; the buyer cannot open it |
| I. Disclosure without a package (`RECORDS_ONLY=1`) | buyer verifies from chain alone and the sale extracts (DRY) |

Case I is an issuance carrier, so no vault record was needed; walking records past issuance is covered by `verify ... walk:` (`p4_dry_records.log`). A relay that decrypts and re-encrypts a disclosure under the buyer's transcript fails the vault signature check; that case is a unit test (`carrier-core::channel`).

## P4 broadcast: the first sale with vault records, over the disclosure channel (`broadcast_p4/`)

Approved by Ryot and run with `scripts/run_p4.sh`. One tx: `e96845ecfc84008674195d8ee77fd433cdb7cdfc3464711ae92221d58ada3b1d`, height 4435628. Item #1 of the sealed collection `5fc17ef389fdae9b` moved from the main seed's vault (`OFF=30` acct2, index 32) to the buyer seed's fresh vault 17, and 10,000 zats went to a per-listing address of `OFF=30` acct5. Buyer and seller used the channel flow: public listing, intent, lease and signed disclosure, encrypted PCZT.

- `V_walk.log`: `verify ... walk:1:e96845ec…` with the new holder's key rebuilds the provenance from the vault record in the successor's memo alone (one hop, pointer kind issuance) and returns HELD. No package was used.
- `V_package.log`: the package the seller saved gives the same verdict (`package_sealed_item1_hop1.json`).
- `V_oldholder.log`: the former vault's key opens no carrier in the tx (exit 2).
- `V_payment.log`: 10,000 zats to the seller's pay key; `sale_p4_tx.hex` holds the raw bytes.
- `R_recover.log`: `carrier recover` on the buyer seed, from height 4435100, finds the account index record in the change output: item #1, hop 1, vault index 17, unspent; next vault index 18.

New view-only keys in `viewing_keys.json`: `buyer_acct7`, `sealed_acct2`, `sealed_acct5`. `verify.sh` re-checks the walk, the package, the former holder, the payment from the published bytes, and (with the buyer seed present) the recovery.
