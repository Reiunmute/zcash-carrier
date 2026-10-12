# Marketplace design direction (draft 1, 2026-10-02)

Ryot's direction (D6), 2026-10-02: follow OpenSea's structure closely, with our own colors and type. Colors were updated to the Zarrier brand on 2026-10-12. This draft maps the two onto the pages of SPEC section 7. A prototype lives in `web/prototype/`: one marketplace site for everyone (since 2026-10-12, an arcade look: starfield, Press Start 2P, IBM Plex Mono). Visitors connect a wallet account, deposit, list and buy; a second tab with another account plays the other side, and tabs share the market through local storage. A sale runs list, reserve, disclosure, verify, deposit, prove, offer, accept, confirmed. The "holds it" checks run the real wasm verifier on testnet data; wallet, deposit, proving, broadcast and mining are simulated and labelled so. Run it with `python3 scripts/serve_web.py` (no caching) and open http://127.0.0.1:8790/. `web/prototype/e2e_demo.mjs` (Playwright) clicks through a whole sale between two players and checks the layout at 375, 512, 800 and 1280 px.

## 1. What we take from OpenSea

Benchmarked on 2026-10-02 from opensea.io pages, bundles and the OpenSea Help Center.

| OpenSea pattern | Ours |
|---|---|
| Top bar: search, "Sign in", account menu | Top bar: wordmark, search, Explore and Account links, network badge, **Connect** |
| Left icon rail with 11 sections | Dropped: we have four routes |
| Collection header: logo, name, verified badge, creator, supply, created, fee | Same, with the **R1 badge** (genuine issuance) in place of "verified", the issuer key prefix in place of the creator, and "sealed" or "open" supply |
| Stats row: floor, top offer, volume, listed, owners | Floor, listed, items, supply state. No volume or owners: the site publishes no sale events (SPEC 5) and owners are private |
| Tabs: Explore, Items, Offers, Holders, Traits, Activity, Analytics, About | Items and About only. No public activity by design |
| Filter sidebar (status, price, traits) and sort, default "price low to high" | Status (All, Listed) and sort by price. No traits yet |
| Card: square media, name, price, last sale or countdown, hover "Buy now" bar | Square pixel art, `#n`, price or "Not listed", listing expiry; hover bar on pointer devices, always-visible action on touch |
| Item page: sticky media on 5/12 columns, info scrolls on the right | Same split |
| Title block with bordered chips (standard, chain, token id) | Chips: Ironwood, Testnet, item n of N |
| Action panel: "Buy now", "Make offer"; owner sees Sell, Cancel, Edit | **Buy** for visitors, **List** or Cancel for the holder. No offers in Stage 1 |
| "Blockchain details": chain, contract, token id, metadata storage ("fully onchain") | **Provenance** panel: R1, then R2 and R3 after the disclosure, the trust basis (two servers agree), and "art: fully on chain (memo bytes)" |
| Activity table as the item's history | Provenance timeline, shown only to the buyer who received the disclosure |
| Listing form: price, duration, fee summary ending in proceeds, low-price confirm | Price, expiry in blocks and time, fee line, "you receive", the disclosure and republication warning (SPEC 3.1) |
| Action-timeline modal with a status per step, then toast and success modal | Buy modal with the steps intent, verify, build, prove, sign, post; the free-option notice (SPEC 3.4) |
| Profile tabs: Items, Listings, Activity; listings table with Cancel | Account tabs: Collected, Listed, Activity (local only), plus balance, Deposit, Sweep, Restore |

Not taken: OpenSea's dark default, its blue accent, its fonts (GT America), shopping cart, sweeping, offers, analytics, rewards.

## 2. Visual tokens

| Token | Value | Use here |
|---|---|---|
| Paper | `#F4EFE2` | page surface, light-first |
| Ink | `#231F20` (Zcash black) | text, 1 px structural rules, secondary buttons |
| Deep | `#231F20` | primary action (Connect, Buy, List) with Paper text |
| Mid | `#F4B728` (Zcash yellow) | edge-connected hero field, selected tab and filter, Ink text |
| Light | `#FBE3A6` (yellow tint) | quiet bands: badges, notices, network badge, Ink text |
| Type | Departure Mono (one weight), NeoDunggeunmo Code for Korean | all interface text; hierarchy from size, space and rules, never synthetic bold |
| Grid | 12 columns, 24 px gutters; 4 columns, 16 px on mobile; 8 px unit | page grid |
| Radius | 0 for fields, 8 px for controls and cards, pill for compact primary actions | |
| Rules | 1 px Ink | card borders, table rows, panel separators (OpenSea also separates with 1 px borders, not shadows) |

Contrast: Paper text only on Deep; Ink on Mid and Light.

The marketplace shows its name as plain Departure Mono text. The Zarrier mark lives in `brand/`.

## 3. Decisions in this draft

- **Light-first.** We chose light over OpenSea's dark default. Dark mode is out of scope for now.
- **Pixel art is drawn from the memo bytes** in the browser (26 by 26, `image-rendering: pixelated`), never from an uploaded image.
- **Verification badges carry the rule name** (R1, R2, R3) and a plain label, so "verified" never means "the server says so".
- **Testnet is visible everywhere**: a network badge in the top bar and "TAZ, no value" next to prices.
- Open questions for Ryot: Korean UI at launch or later.
