# Open issues

## 1. Two layers of enforcement

| Attack | Stopped by | Strength |
|---|---|---|
| Spend the same carrier twice | Zcash consensus (nullifier set) | protocol-level |
| Give a buyer a fake successor | Verifier rule R2 | cryptographic. It holds for anyone who runs the verifier before paying. |
| Mint beyond the FINAL note | Verifier rule R1 | social/indexer-level. The chain does not stop the tx. Everyone agrees to ignore it. |

On-chain collections on other chains share this model. A contract can have a mint function, and indexers still decide which contract is "the collection". The weak point here is narrower: the issuer can still *create* look-alike notes after FINAL, and a careless buyer who skips the verifier could be fooled.

## 2. FINAL is not yet a hard seal

In the PoC the FINAL note is held by the issuer. The issuer can still spend it and create a successor. Verifiers ignore any successor of FINAL, so R1 already rejects extra mints. Still, "the key physically cannot continue the chain" would be a stronger public story.

To test:
- Send FINAL to an address derived from a nothing-up-my-sleeve point with no known spending key. Show that no one can spend it.
- Or publish the proof that the FINAL note's nullifier has appeared: spend it into nothing, so no unspent successor can exist.

> Status: **closed 2026-10-02 on testnet**. Collection `5fc17ef389fdae9b`, FINAL in tx `512832b969c8…` (height 4435129); `verify.sh` checks `sealed=true`. Implementation: `seal_address(coll)` in `src/lib.rs` is an address whose transmission key is a hash-to-curve point of the collection id, so nobody knows its spending key. `carrier final` with `SEAL=1` sends FINAL there; R1 finds it through the issuer OVK and reports `sealed=true`. The testnet run is step C of `scripts/run_p9_p12.sh` (marketplace SPEC P9).

## 3. Atomic sale

Originally (2026-09-29) payment and carrier transfer were separate txs, and the buyer had to trust the seller. The test was a PCZT/joint-build flow where one tx contains:
- buyer funds, spent to the seller
- the carrier spent to the buyer vault, same Action

Both parties sign the same tx.

> Status: proven on testnet 2026-09-30 with the CLI (`carrier sale-*`, pczt 0.9.3). Tx `6bdeb823ff65…` (height 4425592) has the carrier pair in one Action and the payment to the seller. The buyer alone cannot extract it. The seller refuses a tampered amount and an expired PCZT. Evidence: `evidence/testnet-2026-09-30/`. Buyer and seller keys came from one local seed; a run with separate seeds is pending (marketplace SPEC P12). Hardened and DRY-tested 2026-10-02 (`evidence/testnet-2026-10-02/`). The browser version is still open (HANDOFF tasks 3 and 5).

## 4. Wallet UX

Ordinary wallets (Zodl, etc.) will show a zero-value note at best. They may also merge it or shuffle it into an unbound Action, which breaks provenance. A dedicated viewer/wallet page is needed. It would:
- import a vault viewing key
- run R1 to R3 locally
- render the memo artwork
- build bound transfers

## 5. lightwalletd trust (was: explorer cross-check)

Every chain fact the verifier uses comes from one lightwalletd operator (`testnet.zec.rocks`): transactions, compact blocks, heights, nullifiers and tree states. A malicious or compromised server could hide a spend or serve fabricated data. "Verified in your own browser" therefore means verified against that server's view.

Done 2026-10-02 (`src/lib.rs`, `src/verifier.rs`):
- a returned transaction must hash to the requested txid
- a block range must be contiguous and hash-linked
- a full spend tx must match the compact block it was indexed from (height, nullifier, cmx)
- duplicate nullifiers in served data are refused
- `LWD2=<url>` requires every compact block in the scanned range (txids and Ironwood actions) and every fetched transaction to be identical on a second server (strengthened after the Codex re-review: comparing block hashes alone could not detect a server hiding a spend)

Still open:
- No independent native-gRPC testnet lightwalletd was reachable on 2026-10-02 (ECC's endpoint timed out; ChainSafe serves grpc-web only). Run our own node as the second source.
- None of the above validates proof of work. A light-client header check, or a full node we trust, is the real fix (marketplace SPEC section 6.2).

## 6. Verifier cosmetics

When the issuer OVK and IVK both open the same attack-mint output, `attack-mint` rejection prints twice.

## 7. Successor validity is not yet checked (found 2026-09-29)

> Status: closed 2026-09-30. `successor_valid` (now in `src/verifier.rs`, since 2026-10-02 also checking the hop counter) enforces the rule (exit 3, `VERIFY BURNED`). `verify.sh` is green on Ryot's Mac: the genuine item #1 successor is valid, and the forged item #2, opened with the funding key, reports `successor invalid (value 9920000 != 0)` and is burned at hop 0.

In the forge test, the true successor of item #2's carrier was the funding wallet's change note (value > 0, empty memo). That note was later spent in the attack-mint tx without any binding. Before the fix, R2 accepted any output in the same Action as the successor. The rule now enforced: a successor is valid only if `value == 0` and its memo is `HOP` with the same `coll` and item number. Otherwise the item is treated as **burned**, and later hops are ignored. The verifier needs the successor note opened, so the buyer checks its own received note. A third party can check this only with the next vault key.
