# Carrier

**Private collectibles on Zcash that you actually own, sold in one atomic transaction.**

Carrier turns a shielded Zcash note into a collectible. The note is the item, its encrypted memo holds the artwork, and Zcash's own zero-knowledge circuit links each owner to the next. A sale moves the item and the payment in the same transaction, so no marketplace ever holds anyone's funds or art. It works on Zcash as deployed today: no smart contracts, no new token standard, no protocol change.

> **Status:** working proof of concept on Zcash **testnet** (Ironwood pool, NU6.3). Not audited. No real funds. Built for the Crypto World's Fair hackathon, Zcash track.

![Two items, rebuilt from on-chain memo bytes](evidence/testnet-2026-09-29/onchain_items.png)

*These two test items were drawn from bytes read back out of testnet memos. They are byte-identical to the source files.*

## The problem

Zcash makes payments private, but collectibles on Zcash have not really belonged to their owners.

- **zkSNARKs**, the Zcash collection sold on Zilkroad, keeps the list of owners in the marketplace's database. The payment is shielded; the change of title is a database write ([Onchain Diary analysis](https://paragraph.com/@onchaindiary/zksnarks-auction-what-bidders-bought-1)). If the operator errs or disappears, so does the record.
- **Escrow marketplaces** hold the buyer's money between payment and delivery. One hack loses everyone's funds.
- **Zcash Shielded Assets (ZSA)**, the native token standard, is not on mainnet and has no date.

## What Carrier does

- **Ownership is on chain.** Holding an item means holding one zero-value shielded note. No company list is involved.
- **The zk proof enforces the chain of owners.** A transfer spends the old note and creates the new one in the same Action, and the Action circuit forces `rho_new = nf_old`. Each item therefore has exactly one valid successor.
- **The art lives on chain**, encrypted, in the note's 512-byte memo.
- **A sale is one transaction.** The item goes to the buyer and the price to the seller, built jointly as a PCZT (partially created Zcash transaction). Either both legs land or nothing moves.
- **Anyone can check an item** from chain data, with the `verify` tool or in the browser. No website or database is needed.
- **Holdings stay private.** Outsiders see only that shielded transactions happened. A buyer learns the history of the one item it buys and nothing else.

## What already works (testnet evidence)

Every row is a real testnet transaction or a recorded run. `bash verify.sh` re-checks the verdicts, the sale payments and the artwork from public chain data.

| Date | What happened | Evidence |
|---|---|---|
| 2026-09-29 | Minted 2 items, capped the supply with a `FINAL` note, transferred item #1 | txs `82562648ae64…`, `b193c439841d…`, `45056894d3e1…` |
| 2026-09-29 | Three attacks. Double spend: refused by the node. Forged successor and mint after `FINAL`: mined, rejected by the verifier | `3cfcf04ed370…`, `78a9a662f825…` |
| 2026-09-30 | **First atomic sale**: item #1 to the buyer and 10,000 zats to the seller in one tx (both key sets from one seed, separated in code) | `6bdeb823ff65…` at height 4425592 |
| 2026-10-02 | **Atomic sale between two separately held seeds** | `281788d78f3e…` at height 4435126 |
| 2026-10-02 | **Sealed supply**: `FINAL` sent to an address whose spending key nobody knows | collection `5fc17ef389fdae9b`, tx `512832b969c8…` |
| 2026-10-02 | Sale with on-chain vault records; provenance rebuilt from the records alone, and the item recovered from the buyer's seed alone | `e96845ecfc84…` at height 4435628 |
| 2026-10-02 | Nine cheating attempts refused, including a seller who edits the tx after the buyer signed (DRY runs, nothing broadcast) | `evidence/testnet-2026-10-02/` |
| 2026-10-04 | The whole sale (build, prove, sign) runs in a browser: 9.5 to 13.4 s in Chromium with 14 threads | `research/stage1-checks/RESULTS.md` |
| 2026-10-04 | The verifier runs in the browser (WebAssembly over grpc-web) and agrees with the native verifier on all 7 verdict cases | same file |
| 2026-10-12 | Two-party sale through the order book server, end to end (DRY) | `scripts/orderbook_e2e.sh` |

Full txids and logs are in [`evidence/`](evidence/).

## How it works

```
 mint                       transfer or sale                    transfer or sale
 issuer ──► carrier (hop 0) ──spend──► carrier (hop 1) ──spend──► carrier (hop 2)
                             same Action               same Action
                             rho_1 = nf_0              rho_2 = nf_1
```

1. **Carrier note.** One item is one zero-value note in the Ironwood shielded pool. The memo starts with a tag, the collection id, the item number and a hop counter, followed by the artwork: 26x26 pixels, 16 colours, 386 bytes.
2. **Same-Action binding.** Spending a carrier and creating its successor in the same Action makes the circuit chain them. A nullifier can appear on chain only once, so there is one successor, and the node itself stops double sales.
3. **Supply cap.** The issuer keeps its own chain of bound sequence notes that ends in a `FINAL` note. The issuer's viewing key is public, so anyone can count the items. Anything minted outside that chain does not count.
4. **Verification rules.** R1: the item is in the issuer's chain. R2: every hop is a bound spend with a valid successor (value 0, correct memo, next hop number). R3: the claimed holder can open the current note and it is unspent. Verdicts: HELD, SPENT, BURNED, UNVERIFIED or FAIL.
5. **Atomic sale.** The buyer verifies the item, builds one 3-Action transaction (carrier to a fresh buyer vault, price to the seller, change back), proves it, signs only its own spend and strips its private fields. The seller checks price and expiry, signs the carrier spend last and broadcasts. Neither side can finish alone.
6. **Marketplace, Stage 1.** Keys, proofs and verification stay in the user's browser. The server holds only public listings and relays encrypted messages between buyer and seller. It never learns who owns what.

Specs: [`docs/SPEC.md`](docs/SPEC.md) (format and rules), [`docs/marketplace/SPEC.md`](docs/marketplace/SPEC.md) (marketplace protocol, draft 4). Longer explanation for newcomers: [`docs/OVERVIEW.md`](docs/OVERVIEW.md). Product goals: [`docs/VISION.md`](docs/VISION.md).

## Try it

**Re-check the testnet evidence.** Needs Rust stable and network access, no keys:

```bash
cargo build --release
bash verify.sh        # ends with VERIFY.SH OK
```

It uses the published test viewing keys. On an Apple M4 Pro it took 166 s on 2026-10-12, most of it the WebAssembly sale self-test. It does not re-run the node's double-spend rejection or the DRY negative checks; those are logged in the evidence folders.

**Watch one sale from both sides** in the browser prototype:

```bash
bash scripts/build_web.sh       # needs nightly Rust, wasm-bindgen-cli 0.2.129, wasi-sdk
python3 scripts/serve_web.py    # then open http://127.0.0.1:8790/demo.html
```

Seller and buyer screens sit side by side: list, reserve, disclosure, verify, deposit, prove, offer, accept. The "holds it" checks run the real verifier on testnet data. Wallet, deposit, proving, broadcast and mining are simulated on this page and labelled as such.

**Drive the CLI** (needs a funded testnet key in `state/`; the first run creates `state/seed.hex`):

```bash
mkdir -p state out && chmod 700 state
export STATE=state OFF=10            # OFF shifts role accounts 1-4 to a fresh key range; acct0 = funding
./target/release/carrier info        # role addresses; fund acct0 from a testnet faucet
./target/release/carrier genesis     # then: final, transfer, sale-list, sale-buy, sale-accept, recover
```

Environment: `LWD` (lightwalletd endpoint, default `https://testnet.zec.rocks:443`), `LWD2` (second server for cross-checks), `DRY=1` (build and prove without broadcasting), `HOLDER_FVK` (enables the R3 holder check in `verify`).

## Similar projects and how Carrier differs

| | zkSNARKs (Zilkroad) | Bitcoin Ordinals | ZecBit (whitepaper) | Carrier |
|---|---|---|---|---|
| Ownership record | operator database | on chain, public | ZSA testnet; a note-chain design for mainnet on paper | on chain, shielded |
| Owner-to-owner link | trust the operator | indexer rule | `rho = nf`, specified, not deployed | `rho = nf`, checked by every node |
| Atomic trade | no | yes (PSBT) | custodial escrow; trustless trade is their top open question | yes (PCZT), mined on testnet |
| Who sees holdings | the operator | everyone | shielded | shielded |

What Carrier has that the others do not:
- **On-chain ownership on Zcash today**, without waiting for ZSA.
- **No custody at any step.** The sale is one transaction that buyer and seller build together.
- **Mined testnet transactions for every step**, including the sale between two separately held wallets.
- **A sealed supply** and **a verifier that runs in the browser**.

Our notes on ZecBit's design: [`docs/research/zecbit-whitepaper.md`](docs/research/zecbit-whitepaper.md).

## Security model and known limits

- **Two enforcement layers.** Zcash consensus stops double spends. Forged successors and mints after `FINAL` can still be mined; the verifier rejects them. A buyer who skips verification can be fooled.
- **Chain data source.** The verifier trusts the lightwalletd servers it reads from. It checks txids, hash links, block inclusion and agreement between two servers (`LWD2`), but not proof of work. The browser verifier still reads from one server.
- **Wallets.** No existing Zcash wallet shows carriers or builds bound transfers. The marketplace keeps carrier keys in the browser and uses normal wallets only to fund purchases and receive payouts.
- **Disclosure.** A buyer sees the history of the item it buys, and can republish it.
- **Small art.** At most 464 bytes of payload per note.
- **Not audited.** Three review rounds with Codex found issues. Fixes and the items still open are tracked in [`docs/research/`](docs/research/) and [`docs/OPEN_ISSUES.md`](docs/OPEN_ISSUES.md).

Privacy of the evidence files: `evidence/*/package_*.json` hold spent carrier notes and their vault viewing keys on purpose, because that is the provenance a seller hands a buyer. The issuer viewing key is public by design. Spending keys and the test seed stay in `state/`, which is git-ignored.

## Roadmap

1. Public testnet beta of the marketplace: our own Zebra and lightwalletd node as a second data source (`deploy/`), two-server checks in the browser, wallet connection through Noir Wallet signatures.
2. External security review.
3. Mainnet, as a separate decision after 1 and 2. Ironwood is already active on mainnet, so no protocol change is needed.
4. A migration path to ZSA once it reaches mainnet.

## Repository layout

```
crates/carrier-core/   formats, keys, decryption, rules R1 to R3, disclosure channel; native and wasm32
crates/carrier-sale/   PCZT atomic sale: buyer build, prove, sign, redact; seller check, sign, extract
crates/carrier-wasm/   wasm-bindgen exports for the browser, with parity checks against native
crates/orderbook/      order book server (axum): public listings, encrypted buyer-seller messages
src/bin/carrier.rs     issuer and holder CLI: genesis, final, transfer, sale-*, recover, attack tests
src/bin/verify.rs      independent verifier CLI
web/prototype/         marketplace UI prototype and the two-sided sale demo
deploy/                Zebra, lightwalletd, Envoy, Caddy and order book deployment configs
evidence/              public testnet artifacts (no spending keys)
research/              browser feasibility measurements
docs/                  specs, overview, open issues, reviews
verify.sh              repository gate
```

## Built during the hackathon

- First commit: 2026-09-29, inside the contest period (2026-09-14 to 2026-10-12). All code in this repository was written in that window; `git log` has the full timeline.
- Before the first commit (from August 2026) we only researched the Zcash collectibles market and the zkSNARKs/Zilkroad model. No code came from that period.
- Built with heavy use of AI coding agents (Claude, Codex) under the founder's direction. Security reviews were run with Codex and are kept in `docs/research/`.
- Open-source dependencies: the Zcash Rust crates (`orchard`, `zcash_primitives`, `zcash_client_backend`, `pczt`) and the others listed in `Cargo.toml`.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option, the same terms as the Zcash Rust crates it builds on.
