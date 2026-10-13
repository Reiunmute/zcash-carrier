# Carrier: overview for readers with no context

Written 2026-09-30. Status: research proof of concept on **Zcash testnet only**. Not audited. The product is called **Zarrier** (2026-10-12); "Carrier" is the protocol and the code.

## 1. In one paragraph

Carrier puts collectibles on Zcash as **shielded notes**. One item is one zero-value private note. Its memo field holds the artwork. When the item changes hands, the old note is spent and its successor is created in the same "Action". Zcash's zero-knowledge circuit then forces the successor to be cryptographically linked to the note it replaces. Anyone holding the right viewing keys can check the whole chain of owners from blockchain data, without trusting a website or a database (today the verifier still trusts the lightwalletd server it reads from; section 7). On 2026-09-29 and 2026-09-30 we proved the full life cycle on testnet: mint, transfer, supply cap, three attacks, and a **one-transaction atomic sale** between a buyer and a seller.

## 2. Why this project exists

The goal is an OpenSea-style public website where people connect a wallet, see their items, and buy and sell them, on Zcash.

Two existing approaches fall short of that goal:

- **zkSNARKs**, the Zcash collection sold on Zilkroad, keeps ownership in the marketplace operator's database. The shielded payment is real cryptography. The transfer of title is a database write ([Onchain Diary analysis](https://paragraph.com/@onchaindiary/zksnarks-auction-what-bidders-bought-1)). If the operator disappears or makes a mistake, the record of who owns what goes with it.
- **Zcash Shielded Assets (ZSA, ZIP 226/227)**, the native token standard, has no mainnet date. We treat it as a future migration target, not something to build on today.

Carrier tries to get "real on-chain ownership with zero-knowledge enforcement" from Zcash **exactly as it is deployed today**.

## 3. Background in five terms

You need only these to follow the rest.

| Term | Meaning |
|---|---|
| **Shielded note** | A private "coin" in Zcash. The chain stores only an encrypted note and a commitment (`cmx`) to it. Outsiders cannot see the amount, the owner, or the memo. |
| **Nullifier (`nf`)** | A unique tag revealed when a note is spent. The node rejects a second appearance of the same nullifier. That is how Zcash stops double spends. |
| **Action** | The unit of a shielded transaction in the Orchard-style pools: one spend plus one output, proven together by one zero-knowledge circuit. |
| **Memo** | 512 bytes of encrypted data attached to every note. Only someone who can decrypt the note can read it. |
| **Viewing keys** | The full viewing key (FVK) lets you decrypt and recognise a key's notes, but not spend them. The spending key stays with the owner. |

All of this happens in the **Ironwood** pool, the newest shielded pool (active on testnet since height 4,134,000, network upgrade NU6.3).

## 4. How Carrier works

### 4.1 The item is a note

- One item is one **zero-value** shielded note, called the **carrier**.
- Its memo starts with a tag (`ZCR1`), the collection id, the item number, and a hop counter. Then come up to 464 bytes of payload. Our artwork is 26x26 pixels in 16 colours, which fits in 386 bytes.
- Each item lives in its own **vault**, a key used for that item only. This matters for privacy (section 4.4).

### 4.2 A transfer is a bound spend

The key idea is **same-Action binding**:

- The Action circuit enforces `rho(new note) = nf(old note)` inside the proof (orchard 0.15.5 `circuit.rs`, `rho_new = nf_old`).
- If the old carrier is spent and the new carrier is created **in the same Action**, the new note is cryptographically chained to the old one.
- A nullifier can appear on chain only once. So each carrier has **exactly one** possible successor. Anyone who knows the old note can find the Action that revealed its nullifier and read the successor's commitment from that Action.

Standard Zcash builders shuffle spends and outputs randomly. Our tool rebuilds the transaction until the shuffle puts the pair in the same Action. With 2 Actions that succeeds with probability 1/2 per build, so it takes a few tries at most.

A successor counts only if it has value 0 and a `HOP` memo for the same collection and item. Anything else **burns** the item (implemented 2026-09-30).

### 4.3 Supply cap

- The issuer keeps a chain of **sequence notes**, bound Action to Action in the same way, ending in a `FINAL` note.
- Every mint output is encrypted so that the **issuer's outgoing viewing key** can recover it, and the issuer's FVK is published.
- So anyone can walk the sequence chain from the genesis transaction to `FINAL` and list exactly which items exist. A carrier minted outside that chain, or after `FINAL`, is invalid.

### 4.4 Verification without a website

The `verify` program uses only chain data, the public collection record, a **provenance package** that the seller gives the buyer, and the holder's viewing key for the last hop. The package contains the earlier carrier notes and their vault FVKs. It checks three rules:

| Rule | What it checks |
|---|---|
| **R1 issuance set** | The item is in the issuer's sequence chain, up to `FINAL`. |
| **R2 transfer hops** | Each hop: the disclosed note matches the current commitment, its nullifier is on chain, and the successor is the output of the same Action and is valid (value 0, `HOP` memo, next hop number). |
| **R3 holder** | The claimed owner can decrypt the current carrier, and its nullifier has not appeared yet (it is unspent). |

The verdict is HELD, SPENT (the chain is valid but that key no longer holds the item), BURNED, UNVERIFIED (no holder key) or FAIL. The link between notes is enforced by the circuit. Which note counts as a valid successor is the verifier's rule.

Privacy follows from this. The public sees that Ironwood transactions happened, not which item moved or to whom. A buyer sees the history of the item it buys, because it receives the earlier vault FVKs. Per-item vaults keep that disclosure limited to one item.

### 4.5 Atomic sale (proven 2026-09-30)

A sale is **one transaction** with both legs:

1. the carrier spend and its successor to the buyer's new vault, in the same Action
2. the buyer's funds spent, with `price` paid to the seller
3. the buyer's change and the fee

It uses a **PCZT** (partially created Zcash transaction):

- **Buyer:** first verifies the item from chain (R1 to R3). Then it builds the transaction, proves it, signs only its own funding spend, clears its private fields from the file (since 2026-10-02 every buyer field the seller does not need), and hands the file to the seller.
- **Seller:** checks the file. The payment must decrypt to the listed price at the seller's address, the carrier spend must be present and still unspent, and the file must not be expired or valid for too long. The seller then signs the carrier spend and broadcasts.
- In the 2026-09-30 run both parties' keys came from one local test seed, separated in code. A run with separate seeds is pending.
- Neither party can finish alone. Without both signatures the transaction is invalid. Without the seller's signature the buyer cannot even extract a transaction.

## 5. How this differs from zkSNARKs and Bitcoin Ordinals

### 5.1 Summary table

| Question | zkSNARKs (Zilkroad) | Bitcoin Ordinals | Carrier |
|---|---|---|---|
| Where is ownership recorded? | The operator's database | On chain, as "which UTXO holds the inscribed satoshi" | On chain, as "which shielded note is the current carrier" |
| What makes a transfer happen? | The operator updates a row | Any Bitcoin transaction that moves that satoshi | A transaction that spends the carrier and creates its successor in the same Action |
| What links old owner to new owner? | Trust in the operator | The ordinal theory "first in, first out" rule, computed by an indexer. Bitcoin consensus does not know about it. | `rho_new = nf_old`, enforced inside the zero-knowledge proof that the node verifies |
| Who stops a double sale? | The operator | Bitcoin consensus (a UTXO can be spent once) | Zcash consensus (a nullifier can appear once) |
| What defines the collection and its supply cap? | The operator's list | Off-chain lists or parent/child inscriptions, interpreted by indexers and marketplaces | Issuer sequence chain ending in `FINAL`, checked by the verifier from chain data |
| Where is the artwork? | Off chain | On chain, public, in transaction witness data (can be large) | On chain, encrypted, in the 512-byte note memo (small) |
| Who can see owners and history? | The operator | Everyone: addresses, amounts, full history | Only holders of the relevant viewing keys |
| How does a buyer check an item? | Ask the operator | Run or trust an Ordinals indexer, which scans Bitcoin history | Run `verify` on a provenance package plus public chain data |
| Atomic trade? | No. The payment is shielded, the title change is a database write | Yes, with PSBTs (seller pre-signs its input and the payment it wants) | Yes, with PCZTs (buyer signs first, seller signs last and broadcasts) |
| If the company disappears | The ownership record is gone | Nothing changes | Nothing changes |

### 5.2 Compared with zkSNARKs

zkSNARKs and Carrier are both Zcash collections, but they put the trust in different places.

- In zkSNARKs the **payment** is private and cryptographic. The **ownership** is not. The marketplace's database decides who owns an item. Buyers rely on the operator to record the sale, keep the record, and not change it.
- In Carrier the ownership itself is a note on chain. No company holds the record. A sale is one transaction in which the carrier and the payment move together, or nothing moves.
- Carrier also keeps the zero-knowledge part where it matters. The proof that the new note continues the old one is checked by every Zcash node, in the same circuit that protects ordinary Zcash payments.

### 5.3 Compared with Bitcoin Ordinals

Ordinals is the closest relative, so the comparison is worth doing carefully, including where the two are alike.

**How Ordinals works, briefly.** Ordinal theory numbers every satoshi in the order it was mined and follows it through transactions with a "first in, first out" rule. An inscription puts content (an image, text) into the witness of a Taproot transaction and assigns it to one satoshi. Whoever controls the UTXO containing that satoshi "owns" the inscription. Bitcoin itself does not know any of this. The `ord` indexer and the marketplaces apply the rules.

**Where they are alike.**

- Double spends are stopped by the base chain in both: one UTXO spend in Bitcoin, one nullifier in Zcash.
- Both rely on a convention layer above consensus. In Ordinals, the indexer decides which satoshi carries which inscription and what a "collection" is. In Carrier, the verifier decides which output is the successor and which notes are in the issuance set. We say this openly: a forged successor or a mint after `FINAL` can still be mined. The verifier rejects it, and the chain does not.
- Both are at risk from wallets that do not understand them. An ordinary Bitcoin wallet can spend an inscribed satoshi as a fee. An ordinary Zcash wallet might spend a zero-value carrier as an input in an unbound Action, which breaks the chain.

**Where they differ.**

1. **What links owners.** In Ordinals the link is a numbering rule that an indexer computes. In Carrier the link is part of the zero-knowledge proof. `rho_new = nf_old` is a constraint in the Action circuit, and every node checks it. The verifier only has to find the right Action. It does not have to trust anyone's computation of the link.
2. **What you must process to verify.** To be sure of an inscription's owner you need an Ordinals index, which in practice means scanning Bitcoin history or trusting someone who did. A Carrier buyer checks only the transactions of one collection and one item: the issuance chain plus the hops in the package.
3. **Privacy.** Ordinals is fully public. Anyone can see which address holds which inscription, what was paid, and every previous owner. Carrier is shielded. Outsiders see only that Ironwood transactions happened. The buyer learns the item's history, because provenance requires it, and nothing about the owner's other items.
4. **Artwork.** Inscriptions can hold large files (up to the block size) in public witness data. Carrier holds at most 464 bytes of payload per note, encrypted. That is why our art is 26x26 pixels. It is visible to the issuer's auditors and to holders, not to the whole internet by default.
5. **Supply.** Ordinals collections are usually defined by off-chain lists or by parent/child inscriptions. Carrier defines the issuance set by a chain of bound sequence notes that ends in `FINAL`. Anyone with the public issuer key can recount it from chain data.

## 6. What has been done

### 6.1 Testnet run, 2026-09-29 (`evidence/testnet-2026-09-29/`)

Collection `c7fe2aea…`, two items, funded with 0.1 TAZ from a faucet.

| Step | Transaction | Result |
|---|---|---|
| Mint #1 and open the sequence | `82562648ae64…` | mined |
| Mint #2 and close with `FINAL` | `b193c439841d…` | mined |
| Transfer #1 to a buyer vault | `45056894d3e1…` | mined at 4416737 |
| Attack: forged successor for #2 | `3cfcf04ed370…` | mined, **the verifier rejects it** |
| Attack: spend #1 twice | not broadcastable | **the node rejects it** (duplicate nullifier) |
| Attack: mint #3 after `FINAL` | `78a9a662f825…` | mined, **the verifier rejects it** |

Both artworks were rebuilt from the on-chain memo bytes and are byte-identical to the source files.

### 6.2 Browser feasibility, 2026-09-30 (`research/stage1-checks/RESULTS.md`)

- A wallet signature (Noir Wallet, `derived` mode) was deterministic 3 out of 3 times, so it can seed a collectibles key. Risk: the signature is not bound to the website's origin.
- Browser proving (multithreaded WASM, Apple M4 Pro): 1.9 s for 2 Actions, 3.6 s for 4 Actions.
- A browser can reach testnet lightwalletd through ChainSafe grpc-web or a small relay. `zec.rocks` has no CORS.

### 6.3 Successor validity rule, 2026-09-30 (OPEN_ISSUES #7)

The verifier now burns an item whose successor has a value other than 0 or a memo that is not `HOP`. In the 2026-09-29 forgery, the real successor of item #2 was a funding change note of 9,920,000 zats. The verifier now reports item #2 as burned.

### 6.4 Atomic sale, 2026-09-30 (`evidence/testnet-2026-09-30/`)

| Item | Value |
|---|---|
| Transaction | `6bdeb823ff6556dce7ef8c254c8ceaa257913cc820c43da271c52dabdc98ba5b`, mined at 4425592 |
| Carrier | item #1 from the seller's vault to the buyer's new vault, bound in action 2 |
| Payment | 10,000 zats to the seller |
| Buyer change | 9,885,000 zats (9,910,000 minus price 10,000 minus fee 15,000) |
| Verifier | the buyer's new vault is the holder. The seller's old vault no longer opens the carrier. |
| Negative checks | the buyer alone cannot produce a transaction; the seller refuses a wrong amount; the seller refuses an expired PCZT |

All payments were between our own test keys. Testnet assets are never sold or offered.

## 7. Known limits

- **Two enforcement layers.** Double spends are stopped by consensus. Forged successors and mints after `FINAL` are stopped only by the verifier. A buyer who skips the verifier can be fooled.
- **`FINAL` is not a hard seal yet.** The issuer can still spend it. Verifiers ignore any successor, but "no one can continue the chain" would be stronger (OPEN_ISSUES #2).
- **Wallets.** No existing Zcash wallet shows carriers, builds bound transfers, or joins a PCZT from a website. The marketplace plan keeps carrier keys in the user's browser and uses normal wallets only for funding and payouts.
- **One data source.** All chain data comes from one lightwalletd operator. The verifier checks txids, hash links and internal consistency, and can cross-check a second server, but none was reachable yet, and proof of work is not validated (OPEN_ISSUES #5).
- **Buyer privacy.** A buyer sees the previous vaults' history of that item.
- **Disclosure.** Any prospective buyer who receives an item's provenance can republish it. The marketplace limits who receives it, but cannot prevent this (marketplace SPEC section 3).

## 8. Next steps

1. Browser network: streaming block ranges with Ironwood data over grpc-web.
2. `FINAL` hard seal.
3. A shared core crate that compiles for native and WebAssembly.
4. The public web app, only after the marketplace spec (`docs/marketplace/SPEC.md`, draft 3) is approved.

## 9. Check it yourself

```bash
cargo build --release
bash verify.sh      # re-checks the R1 to R3 verdicts, the sale payment and the artwork from chain data; prints VERIFY.SH OK
```

More detail: `docs/SPEC.md` (format and rules), `docs/OPEN_ISSUES.md`, `docs/marketplace/SPEC.md`, `docs/HANDOFF.md` (state and next tasks).
