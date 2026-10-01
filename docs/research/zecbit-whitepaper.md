# ZecBit whitepaper: analysis and implications for Carrier

Written 2026-10-01. Source: "ZecBit, The Private NFT Market", technical whitepaper V1.0 (cover says August 2026; PDF created 2026-09-20), 467 A4 pages, saved from `zecbit.net/ZecBit-Whitepaper.pdf`. The full text was read in five parts. Page numbers below refer to the PDF.

**Every ZecBit number and claim below is theirs and unverified by us.** Our own claims cite our evidence.

Related: their public repo `Zecbit-nft/nscv` (2026-09-20) implements the formats of their Part VIII construction in Python with fake crypto backends. See section 3.

## 1. Summary

- ZecBit's main product is a ZSA-based NFT marketplace. ZSA runs only on a dedicated QEDIT-fork testnet. It has no mainnet date, the next upgrade excludes transaction-format changes, and ZIP 228 (asset swaps) is a draft with no reference implementation (p.34-35, 115, 121, 355, 379).
- Their testnet trading is custodial escrow: the venue takes deposits and sends two separate transactions, and atomicity is a database property (p.116-118). The hosted beta wallet holds one master seed behind an HSM (p.97-99).
- Their only on-chain evidence is one issuance transaction of 10 assets at height 251 on the ZSA testnet (p.49). No transfer or trade txid is given.
- Part VIII describes a mainnet-today construction (nullifier-sealed client-side validation) that is **the same core idea as Carrier**: one note per item, spend and successor in the same Action, `rho = nf`, one account per item, per-hop full viewing keys (p.184-187, 217, 236). They date its identification to September 2026 (p.380). They show no deployment of it.
- Their mainnet plan for that construction is a **custodial** venue: the venue holds buyer funds between payment and delivery and takes a percentage of seller proceeds (p.246, 404-412).
- They treat two-party co-construction of one transaction as their top open question (Appendix D.1, p.312) and their "one unverified critical claim" (p.431). Carrier answered it on testnet on 2026-09-30.

## 2. The co-construction question, and our answer

Their D.1 (p.312) asks whether two distrusting parties can build one shielded transaction that spends a note from each, without either extracting a spendable object early and without sharing spending keys. They call it the largest improvement available without new cryptography, rate it "Moderate", and expect joint proving to be the blocker. Elsewhere they state that trustless atomic NFT-for-ZEC settlement is not possible in the current transaction format (p.114), that PCZT does not support the v6 format yet (p.98, 100, 102), and list trustless atomic trading as "No, needs ZIP 228" (p.20).

Their §113 (p.386) says that if it works, custody disappears, trades become single atomic transactions, and the venue becomes a matching service holding nothing. §114 (p.387) says that if it fails, the venue holds funds as a permanent structural fact.

Our result: tx `6bdeb823ff6556dce7ef8c254c8ceaa257913cc820c43da271c52dabdc98ba5b`, v6, Ironwood, mined at 4425592, built with pczt 0.9.3 (`evidence/testnet-2026-09-30/`). Answers to their three sub-questions:

| Sub-question (p.312) | Answer from our run |
|---|---|
| 1. Can one party prove an Action whose witness belongs to the other? | Yes. Proving a spend needs the note, its Merkle path and the full viewing key parts, not the spending key. Provenance already discloses the note and vault FVK to the buyer, so the buyer proved the whole bundle (`src/bin/carrier.rs`, `sale_buy`). |
| 2. Can the binding signature be produced jointly? | No joint secret is needed. The binding key is the sum of value-commitment randomness chosen by the party that constructs the bundle. The pczt IO Finalizer computes it and the extractor applies it. |
| 3. Is there a one-sided broadcastable state? | The buyer alone cannot extract a transaction (`MissingSpendAuthSig`, `negative_checks.log`). The seller can only broadcast the agreed transaction. Unverified: that a seller who edits the effects after the buyer signed fails extraction. |

Consequence: the seller double-sell race they leave open (p.241, 257, 416) is closed, because two sales of one carrier spend the same nullifier and only one can be mined.

## 3. Their construction compared with Carrier

| | ZecBit Part VIII / NSCV | Carrier |
|---|---|---|
| Status | Specification and Python formats with fake crypto backends. No transactions. Examples use placeholders (p.443) | Rust on orchard 0.15.5. Mint, transfer, three attacks and an atomic sale mined on testnet. `verify.sh` re-checks from chain data |
| Pool | Ironwood after NU6.3 sealed Orchard on mainnet, 2026-07-28 at height 3,428,143 (p.28, 175) | Ironwood, testnet. The height matches `zcash_protocol` mainnet NU6.3 activation |
| Item note | Locks 1 zatoshi per item (p.444) | Zero value |
| Successor rule | Any output of that Action the next key opens. No value or memo check | Value 0 and a HOP memo, otherwise burned |
| Collection identity | One 38-byte branded anchor in a transparent data output (p.185, 436) | No transparent outputs. Shielded sequence chain to FINAL, public issuer FVK |
| Supply | Fixed manifest. They call the anchor checked by nobody; a second manifest is detectable, not preventable (p.198, 223, 437) | Recountable from the issuer chain. FINAL is verifier-enforced, not a hard seal (OPEN_ISSUES #2) |
| Artwork | Off chain, hash-committed. They concede a single host is a point of failure and list small on-chain content as a fix (p.233, 252) | 386 bytes in the memo, rebuilt byte-identical from chain |
| Provenance storage | Consignment in self-addressed memos, so a seed restore recovers proofs (p.237, 309, 439) | JSON package files. Seed restore does not recover them yet |
| Settlement | Venue custody | Atomic PCZT sale, no custody |

## 4. Where they are ahead (worth adopting)

- Self-addressed memo storage of provenance, which they call mandatory (p.238 failure mode 4, p.309).
- Padding to the network's common Action count and randomising anchor age (p.273, 304, 315). Our sale tx has 3 Actions.
- Not publishing per-listing data that lets anyone find the sale transaction or read an item's history (p.226-227, 418, 421). Our `listing.json` contains the vault FVK and package, so a public order book of these would expose every listed item.
- Network privacy: lightwalletd lookups reveal which item is being verified, and broadcasting reveals IP (p.256, 274-275, 297).
- A full conformance suite and an error taxonomy (p.304-311).
- Browser key-handling discipline: reproducible WASM builds, SRI, origin isolation (p.143). A browser-wallet marketplace drained users in June 2026 by sending private keys to its server (p.176).
- Their ZIP 228 design lets sellers be offline and supports sweeps and collection offers (p.132-134). Our flow needs the seller online, and the seller holds a free option until the PCZT expires (p.128).
- Breadth: a 467-page treatment of threat models, failure modes, formal properties and economics, and a metadata standard (ZMD-1).

## 5. Market signals (their figures)

- About 111,000 inscriptions on Zcash mainnet (Zerdinals). About one in seven were funded from shielded value (p.171-173, §54.3). The transaction spike faded within a month (p.172).
- ZINC: 16 items, zero trading volume. ZAP1 stopped after 29 anchors (p.175).
- Two asset-layer grant applications were declined in 2026 (p.176).
- About 29 percent of ZEC supply and 44 percent of transactions are shielded (p.174).

Reading: demand for public assets with private funding exists. Demand for any Zcash NFT product is thin so far.

## 6. Concerns about ZecBit

- Custody on testnet now and on mainnet in their plan, with a percentage fee and a cross-asset conversion flow with refunds in ZEC (p.246, 404-412).
- Allowlist and quest tooling tied to accounts (p.15, 94), and testnet holdings claimable as mainnet assets later (p.20, 54, 112). These conflict with our rules 5 and 6.
- An open account-security finding, amount not stated in the text (p.289). Nothing audited (p.289, 336).
- Internal inconsistencies (for example hops per memo, p.369 vs p.439) and a revision that roughly tripled the document (p.459).

## 7. Implications for Carrier

1. Our differentiator is settlement plus a working implementation, not the primitive. ZecBit published the same primitive first (2026-09-20; our first record is 2026-09-29). Public copy must not claim we invented it.
2. Once our result is public, ZecBit can follow their §113 path. Speed and execution matter.
3. Before a public marketplace:
   - deliver the provenance package only to a committed buyer, never in a public listing
   - store provenance in self-addressed memos
   - redact the buyer's change and per-Action randomness in the PCZT; pad Action counts; randomise anchor age
   - test that a seller who edits the transaction after the buyer signed is rejected
   - mainnet only after Ryot changes the testnet-only rule
   - browser key handling as in section 4
4. ZSA, if it ships, will not migrate our items automatically (their p.390). Our docs call ZSA a migration target; any migration would be issuer-driven (burn and reissue).

## 8. Not verified

- All ZecBit figures, txids and dates.
- Whether their escrow beta and testnet-to-mainnet claims are active today.
- The nature of their open security finding.
