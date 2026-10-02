# P5: Ironwood Action-count distribution (testnet, 2026-10-02)

Marketplace SPEC prerequisite P5. Read-only measurement from compact blocks served by `testnet.zec.rocks`.

Range: blocks 4414129 to 4434128 (20,000 blocks, about 17 days). 14,720 compact transactions, 14,692 with Ironwood actions.

| Ironwood Actions per tx | Count | Share |
|---|---|---|
| 1 | 11,948 | 81.3% |
| 2 | 1,735 | 11.8% |
| 3 | 387 | 2.6% |
| 4 | 42 | 0.3% |
| 5 | 30 | 0.2% |
| 6 | 46 | 0.3% |
| 7 to 40 | 291 | 2.0% |
| 41 | 213 | 1.4% |
| 42 to 66 | 79 | 0.5% |

Reading (inference, unverified):
- The 1-Action transactions are almost one per block, which fits output-only transactions such as coinbase. The most common user payment has 2 Actions.
- The 41-Action cluster looks like batched payouts (a faucet, for example).
- Mainnet may differ. This measures testnet only.

## Consequence for the sale transaction

SPEC draft 3 section 3 has 2 spends and 5 outputs, so at least 5 Actions. Only 0.2% of testnet transactions have 5. A padded sale would stand out, and padding to the common count (2) is impossible.

Proposal (needs Ryot's approval, because it changes the approved SPEC):
- Carry the vault record (SPEC 4.2) in the memo of the successor carrier itself. Both go to the buyer's new vault, and a carrier memo has 464 bytes of payload space, enough for the 96-byte FVK, the 36-byte pointer and the 32-byte commitment.
- Carry the account index record (SPEC 4.3) in the memo of the buyer's change output. The change already goes to the buyer's collectibles account 0. Keep a change output in every sale (zero-value if the deposit was exact) so the record is always there.
- Result: 2 spends and 3 outputs, so 3 Actions. That is the same shape as the 2026-09-30 sale and the second most common user size (2.6%). A seller-side 2-spend shape cannot reach 2 Actions.
