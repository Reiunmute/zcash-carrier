# P5: anchor ages of Ironwood transactions (testnet, 2026-10-02)

Marketplace SPEC prerequisite P5, second half: how old is the anchor a transaction proves against, so the sale tx does not stand out. Read-only measurement with `examples/anchorage.rs` against `testnet.zec.rocks`.

Method: rebuild the final Ironwood root of every block from the treestate at the window start plus the compact blocks' note commitments (the rebuilt root at the tip equals the server's treestate, checked), then map each sampled tx's anchor to the first block whose final root it is. Age = the tx's mined height minus that block.

Sample: blocks 4433631 to 4435630 (2,000 blocks), roots from 4431630, 1,341 txs with Ironwood actions.

| Class | Count | Anchor |
|---|---|---|
| 1 Action | 1,098 | all the empty-tree root (output-only, coinbase-like) |
| 2 to 4 Actions | 195 | see below; 4 used the empty-tree root |
| 5+ Actions | 48 | ages 4 to 50, mostly 10 to 19 |

Anchor age of the 191 txs with 2 to 4 Actions and a non-empty anchor:

| Age (blocks) | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 16 to 26 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Txs | 1 | 5 | 12 | 21 | 22 | 20 | 5 | 8 | 7 | 15 | 17 | 16 | 8 | 11 | 7 | 16 |

Reading:
- Two clusters, ages 3 to 6 (75 txs) and 10 to 15 (74 txs). That fits wallets anchoring 3 or 10 confirmations below the tip, plus a block or two before mining (inference, unverified).
- Anchors up to 50 blocks old were mined, so old anchors are accepted in practice. This is an observation, not a reading of the consensus rules.
- Until this change the CLI anchored at the tip, which gives ages 1 to 2: the shape only about 3% of these txs have. The sale txs of 2026-10-02 are among them.

Rule adopted (SPEC 5): the buyer anchors at `tip - d` with `d` uniform in 2 to 11, and never below the heights of the notes it spends. With one or two blocks before mining, ages land in 3 to 13. A just-confirmed deposit forces a younger anchor; the UI should let the deposit age a few blocks first. `ANCHOR_DEPTH` overrides `d` for tests. Mainnet may differ: this measures testnet only.
