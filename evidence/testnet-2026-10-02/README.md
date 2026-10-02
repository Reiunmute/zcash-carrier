# Testnet checks 2026-10-02: hardened sale flow (DRY, nothing broadcast)

Follow-up to the Codex review (`docs/research/codex-review-2026-10-02.md`). Every run here used `DRY=1` or a test knob that never broadcasts. No transaction was sent.

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

- Buyer and seller still derive from one local seed. The code now supports separate seeds (`STATE` per party, `SALE_DIR` for the exchanged files), but a separate-seed run needs funding on a second seed and therefore a broadcast. Pending approval.
- Chain data came from one lightwalletd (`testnet.zec.rocks`). `LWD2` cross-checking is implemented, but no independent native-gRPC testnet server was reachable on 2026-10-02 (ECC's endpoint timed out; ChainSafe serves grpc-web only).
