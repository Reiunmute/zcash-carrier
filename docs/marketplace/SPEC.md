# Marketplace SPEC (draft 0, for approval)

Status: draft. BUILD starts after Ryot approves this document.
Scope: Zcash testnet, local-only (this Mac). Vercel comes later.

## 1. Goal

An OpenSea-style site where people can:
- connect a wallet and see the carrier items it holds
- list an item at a fixed price
- buy it in one atomic transaction (payment and item transfer in the same tx)

## 2. Wallet survey (checked 2026-09-29)

No existing Zcash wallet can hold carriers and trade them for us today. Two capabilities are missing everywhere:

1. **Showing carriers to a website.** Wallet dApp APIs never expose viewing keys or note memos, by design.
2. **Building a same-Action bound transfer and joining a multi-party PCZT from a dApp.**

| Wallet | dApp connection | What it can do for us | Missing |
|---|---|---|---|
| Noir Wallet (RHEA), Chrome extension | `window.noirwallet`, `@noir-wallet/sdk` 0.1.9. A separate testnet build exists. | `sendTransaction({to, amount, memo})`: funding and simple payments | no viewing key/notes, no PCZT, no bound builds |
| Zucchini, Chrome extension | website connect, ZIP-321 single payment + text memo | funding | same as above |
| Zafu (Rotko), Chrome extension | dApp provider mostly Penumbra/Keplr; handles `zcash:` ZIP-321 links | funding. PCZT (ZIP-324) exists only for its own Zigner air-gap signer. | no Zcash dApp PCZT API found |
| MetaMask Snap (ChainSafe WebZjs) | Snap RPC with PCZT signing | none | last commit 2026-04-16 on a pre-Ironwood librustzcash fork. Ironwood support not confirmed. |
| Brave Wallet | built-in | Ironwood send/receive | no Zcash dApp provider found |
| Zallet (zcash) | local JSON-RPC: `pczt_create/sign/prove/combine/extract` | could sign the buyer leg of a PCZT | full-node wallet, not a browser connection |
| Zodl, Zingo, Zkool, Keystone | mobile/desktop/hardware | funding via ZIP-321 QR. Keystone signs PCZTs. | no dApp connection |

WalletConnect for Zcash is a ZCG grant (Noir team, approved 2026-08-31) and is not shipped yet.

**Decision proposed:** ship our own lightweight **vault wallet**. Existing wallets fund it through a ZIP-321 link or QR (Noir, Zucchini, Zafu, Zodl, and so on). Later adapters: WalletConnect once shipped, and Keystone/Zallet PCZT signing.

## 3. Architecture

```
browser UI (OpenSea IA)  <-- http://127.0.0.1:PORT -->  vault companion (Rust, this repo)
                                                          - keys (local only, 0600)
                                                          - lightwalletd client (testnet)
                                                          - verifier R1-R3
                                                          - PCZT builder / prover / signer
                                                          - local order book (JSON)
```

- The companion binds to 127.0.0.1 only. The UI never receives spending keys.
- **"Connect wallet"** means connecting to the local companion and choosing a vault profile. The companion scans its own vault keys and returns items that pass R1 to R3.
- Every item page shows the verifier verdict computed locally: issuance set, provenance hops, and liveness.

## 4. Pages (OpenSea information architecture)

| Route | Content |
|---|---|
| `/` Explore | collection cards: image mosaic, supply (finalized or open), floor, listed count |
| `/collection/:coll` | header stats, item grid with price badges, filter: listed only, sort by price |
| `/item/:coll/:num` | pixel image rendered from on-chain memo, owner (you / not you), price, **Buy** or **List**, provenance timeline (each hop txid + height), verifier badges |
| `/account` | tabs: Collected, Listed, Activity. Deposit button (ZIP-321 link + QR for external wallets). |
| Connect modal | companion status, vault profile picker, testnet badge |
| List modal | price in TAZ, preview of what gets disclosed (vault FVK + carrier note), confirm |
| Buy modal | price + fee, what gets disclosed, confirm, progress (build, prove, sign, broadcast, mined) |

## 5. Atomic sale protocol (PCZT, one transaction)

Actions in the sale tx:
1. seller carrier spend -> carrier successor to the buyer vault, **same Action** (binding)
2. buyer payment spend(s) -> seller payment address, `price`
3. buyer change, ZIP-317 fee paid by the buyer

Flow:
1. **List.** The seller publishes an order `{coll, num, price, seller_pay_addr, provenance package}` to the order book. The package already contains the carrier note and the vault FVK (required for provenance anyway).
2. **Buy.** The buyer companion builds the PCZT. It can construct the seller spend because it holds the carrier note and vault FVK. It retries unproven builds until the carrier pair shares an Action (cheap, no proof). Then it proves, signs its own spends, and redacts its own spend FVKs.
3. **Seller accept.** The seller companion checks that the payment output decrypts to `price` at `seller_pay_addr`, signs the carrier spend, extracts, and broadcasts.
4. Neither side can complete alone. The tx is invalid without both spend authorizations.

Privacy notes:
- An Orchard/Ironwood bundle has one proof. The prover needs every spend's FVK (`orchard::pczt::Spend.fvk`: "required by the Prover"). The buyer proves, so the seller never needs the buyer's FVK. The seller's vault FVK is disclosed by design.
- After redaction the seller still sees the buyer's payment amount and the tx structure.

Must verify during BUILD (not yet proven):
- `pczt` 0.9.3 supports every role for the Ironwood bundle.
- The seller can sign after the buyer redacts spend FVKs.
- The node accepts the extracted tx.

## 6. Prerequisites in the core

- Fix OPEN_ISSUES #7: a successor is valid only if `value == 0` and its memo is `HOP` for the same `coll` and item. Otherwise the item is burned.
- Replace the `OFF` role hack with named vault profiles in the companion.

## 7. MVP acceptance (proof)

1. The companion starts. The UI loads. Connect shows the buyer vault from the 2026-09-29 run holding item #1, with verifier badges.
2. Profile A lists item #1 at 0.01 TAZ.
3. Profile B (funded from a faucet or an external wallet) buys it: **one** testnet tx contains both legs.
4. After mining, profile B shows item #1 in Collected, and profile A shows the payment in Activity. The independent verifier CLI agrees on the new owner.
5. A Playwright run covers every page and modal. Screenshots are stored under `.harness/runs/`.

## 8. Out of scope (MVP)

Vercel/public hosting, mainnet, WalletConnect, WASM in-browser wallet, auctions, offers, royalties, multi-collection launchpad, naming (see below).

## 9. Naming (open)

These are not ERC-721 tokens. Candidates:
- **Heirloom**: passed hand to hand, and the lineage is the product.
- **Relic**: one-of-one, and provenance-first.
- **Noteprint**: literal. Art printed into a shielded note.
- **Veil**: private by default.
- **Carrier**: current internal term. Technical and neutral.
