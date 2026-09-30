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

## 3. Atomic sale

Right now payment and carrier transfer are separate txs. The buyer trusts the seller. Next test: a PCZT/joint-build flow where one tx contains:
- buyer funds, spent to the seller
- the carrier spent to the buyer vault, same Action

Both parties sign the same tx.

## 4. Wallet UX

Ordinary wallets (Zodl, etc.) will show a zero-value note at best. They may also merge it or shuffle it into an unbound Action, which breaks provenance. A dedicated viewer/wallet page is needed. It would:
- import a vault viewing key
- run R1 to R3 locally
- render the memo artwork
- build bound transfers

## 5. Explorer cross-check

Mined heights come from one lightwalletd operator (`testnet.zec.rocks`). A second independent source has not been confirmed yet.

## 6. Verifier cosmetics

When the issuer OVK and IVK both open the same attack-mint output, `attack-mint` rejection prints twice.

## 7. Successor validity is not yet checked (found 2026-09-29)

In the forge test, the true successor of item #2's carrier was the funding wallet's change note (value > 0, empty memo). That note was later spent in the attack-mint tx without any binding. R2 currently accepts any output in the same Action as the successor. Rule to add: a successor is valid only if `value == 0` and its memo is `HOP` with the same `coll` and item number. Otherwise the item is treated as **burned**, and later hops are ignored. The verifier needs the successor note opened, so the buyer checks its own received note. A third party can check this only with the next vault key.
