// Zarrier marketplace prototype. Layout after OpenSea,
// brand tokens (docs/marketplace/DESIGN.md). Routes follow SPEC section 7.
// Pages are built as HTML strings from the constant mock data below (labels go through esc).
// The real app renders chain and order-book data, which is untrusted: build DOM nodes there.
"use strict";

// Artwork payloads (v0: 48-byte palette + 338 bytes of 4-bit pixels, 26 by 26), the same bytes the
// carrier memos hold on chain (art/art1.bin, art/art2.bin).
const ART = ["0e0e14201e2c3a364e605c7cebe8e0c8c4ba96928cd4aa40fad6787848a0aa78d2285a78468caa0a0a0cffffff5a141e2111111111111111111112111111111111111111111111111111111111111113333311121111111111111113333333331111111211111113333333333333111111111111333333333333333111111111133333333333333333111121111333333444443333331111111133333344464453333331111111333334444c445533333111111133334444c6c455533331121113333344444644555333332111122224444446445555222211111222244487464875552222111112222444444644555522221111122224444446445555222211111222244444464455552222111112222444444644555522221111112222444666665552222121111122224444464455522221111111222224444644552222211111111222224446445222221111111bcbcbcbcbcbcbcbcbcbc111111cbcbcbcbcbcbcbcbcbcb111111bcbcbcbcbcbcbcbcbcbc112111cbcbcbcbcbcbcbcbcbcb111", "0e0e14201e2c3a364e605c7cebe8e0c8c4ba96928cd4aa40fad6787848a0aa78d2285a78468caa0a0a0cffffff5a141e1111111111111121111112111111111111111111111111111111111112111113333311111111111111111113333333331111111211111113333333333333111111111111333333333333333112111111133333333333333333111111111333333444443333331111111133333344464453333331111111333334444f445533333111111133334444f6f4555333311111133333444446445553333311111222244444464455552222111112222444a9464a95552222111112222444444644555522221111122224444446445555222211111222244444464455552222111112222444444644555522221211112222444666665552222121111122224444464455522221111111222224444644552222211111111222224446445222221111111bcbcbcbcbcbcbcbcbcbc111111cbcbcbcbcbcbcbcbcbcb111111bcbcbcbcbcbcbcbcbcbc111111cbcbcbcbcbcbcbcbcbcb121"];
const TIP = 4466500; // mock tip (testnet height on 2026-10-05, rounded)
const BLOCK_SECS = 75;

const COLLECTIONS = [
  {
    coll: "5fc17ef389fdae9b23142e4218153b17e74ae859d344667351a32e03d74e881f",
    label: "Masks, sealed",
    issuer: "5fc17ef389fdae9b23142e4218153b17e74ae859d344667351a32e03d74e881f1219c96e7c7f82da78930fc44648d3f9d264df28300f26a2d6816e075deb993e0961fb6f6c0dbcc71713aafd7fcf9ec04a809dd8c797d0501c9fca64ff16253c",
    genesis: "7eab6bd6bee43d321e800d7a4565dc26d8ce9af29c2045f7fc15f9f8716aac49",
    genesisH: 4435127,
    finalTx: "512832b969c897752116be1649d5a982ad2a82e34199bc1d1e9f11bfef9b68f3",
    sealed: true,
    items: [
      // chain: the real P4 sale and the buyer vault's public test view key (evidence/testnet-2026-10-02),
      // standing in for what the disclosure would hand a buyer.
      { num: 1, art: 0, hop: 1, history: [["Issued", 4435127, "7eab6bd6bee4"], ["Sold to a fresh vault", 4435628, "e96845ecfc84"]],
        chain: { walk: "e96845ecfc84008674195d8ee77fd433cdb7cdfc3464711ae92221d58ada3b1d", holder: "11f91055ff81757be737270894d94cc69df4b39e28d3d3d382aafc834bef4e1c6a4d53b6e2bcfb5f1b0a10e7926d20be4ec2f9e8051507eda25cc0ae84f2963ef422c50cf400d6d1d4e338bd94ed7d780b3d14425cf90970399830b8731d4e13" } },
      { num: 2, art: 1, hop: 0, history: [["Issued", 4435129, "512832b969c8"]] },
    ],
  },
  {
    coll: "c7fe2aeadb5a62001b99d7f71b1a8eb9526d6b457d7f31d919ffaa73a31c7321",
    label: "Masks",
    issuer: "c7fe2aeadb5a62001b99d7f71b1a8eb9526d6b457d7f31d919ffaa73a31c7321b75f9497b64feffbc74c74fc0e385a5c930ae9627f6ccfb0f3262de1be2417161ceedd79468d1687c12169898e65b3b7adc8ad144a1ff91ade335233c9cd4d24",
    genesis: "82562648ae6421901993cedbb72ffa7cf1ed4977ec33ccd1dbb8f91985fc1ea5",
    genesisH: 4416732,
    sealed: false,
    items: [
      { num: 1, art: 0, hop: 3, history: [["Issued", 4416732, "82562648ae64"], ["Transferred", 4416737, "45056894d3e1"], ["Sold", 4425592, "6bdeb823ff65"], ["Sold", 4435126, "281788d78f3e"]] },
      { num: 2, art: 1, hop: 0, burned: true, history: [["Issued", 4416735, "b193c439841d"], ["Burned: successor was not a carrier", 4416739, "3cfcf04ed370"]] },
    ],
  },
];


// ---------- demo store: both screens of demo.html share it through localStorage ----------
// Real: the "holds it" checks (R1 to R3 in wasm over testnet chain data, verify.mjs).
// Simulated: payment, deposit, proving, broadcast, mining. Nothing is sent anywhere.
const SEALED = COLLECTIONS[0].coll;
const STORE = "zarrier-demo-v1", SEEN = "zarrier-demo-seller-seen";
const ROLE = new URLSearchParams(location.search).get("as") === "seller" ? "seller" : "buyer";
const FEE = 15000; // network fee for a 3-Action sale, paid by the buyer
document.title = `Zarrier (${ROLE}, testnet demo)`;
const fresh = () => ({ listings: {}, sale: null, held: { seller: [`${SEALED}/1`, `${SEALED}/2`], buyer: [] }, bal: { seller: 0, buyer: 0 }, connected: {}, feed: [] });
const load = () => { try { return JSON.parse(localStorage.getItem(STORE)) || fresh(); } catch { return fresh(); } };
let S = load();
function save(mut) {
  S = load();
  mut(S);
  localStorage.setItem(STORE, JSON.stringify(S));
  changed();
}
function feed(s, who, text) { s.feed.push({ who, text, at: Date.now() }); }
window.addEventListener("storage", (e) => { if (e.key === STORE) { S = load(); changed(); } });
if (ROLE === "seller") { const beat = () => localStorage.setItem(SEEN, String(Date.now())); beat(); setInterval(beat, 1500); }
const sellerTabOpen = () => Date.now() - Number(localStorage.getItem(SEEN) || 0) < 5000;

// ---------- helpers ----------
const $ = (s, r = document) => r.querySelector(s);
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const short = (h, n = 8) => `${h.slice(0, n)}…`;
const taz = (z) => `${(z / 1e8).toFixed(8).replace(/0+$/, "").replace(/\.$/, "")} TAZ`;
const zats = (z) => `${z.toLocaleString("en-US")} zats`;
const left = (h) => { const s = (h - TIP) * BLOCK_SECS; return s <= 0 ? "ended" : s < 3600 ? `${Math.round(s / 60)} min` : `${Math.round(s / 3600)} h`; };
const findColl = (c) => COLLECTIONS.find((x) => x.coll.startsWith(c));
const keyOf = (c, it) => `${c.coll}/${it.num}`;
const listing = (c, it) => S.listings[keyOf(c, it)];
const mine = (c, it) => S.held[ROLE].includes(keyOf(c, it));
const floor = (c) => { const p = c.items.map((i) => listing(c, i)).filter(Boolean).map((l) => l.price); return p.length ? Math.min(...p) : null; };
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const hex = (n) => [...crypto.getRandomValues(new Uint8Array(n))].map((b) => b.toString(16).padStart(2, "0")).join("");

function artCanvas(idx, label) {
  const b = Uint8Array.from(ART[idx].match(/../g), (x) => parseInt(x, 16));
  const cv = document.createElement("canvas");
  cv.width = cv.height = 26;
  cv.className = "art";
  cv.setAttribute("role", "img");
  cv.setAttribute("aria-label", label);
  const ctx = cv.getContext("2d"), img = ctx.createImageData(26, 26);
  for (let p = 0; p < 676; p++) {
    const byte = b[48 + (p >> 1)], k = p & 1 ? byte & 15 : byte >> 4;
    img.data.set([b[k * 3], b[k * 3 + 1], b[k * 3 + 2], 255], p * 4);
  }
  ctx.putImageData(img, 0, 0);
  return cv;
}
const art = (idx, label) => `<i data-art="${idx}" data-label="${esc(label)}"></i>`;
const hydrate = (root) => root.querySelectorAll("i[data-art]").forEach((el) => el.replaceWith(artCanvas(+el.dataset.art, el.dataset.label)));

// Real verification through verify.mjs (wasm). Resolves to the verdict object.
async function chainCheck(c, it, withKey, progress) {
  if (!window.zarrierVerify) await new Promise((r) => window.addEventListener("zarrier-verify-ready", r, { once: true }));
  return window.zarrierVerify({
    collection: { network: "test", coll: c.coll, issuer_fvk: c.issuer, genesis_txid: c.genesis },
    item: it.num,
    pkg: withKey ? null : { item: it.num, label: "issuance only", hops: [] },
    walk: withKey ? it.chain.walk : null,
    holder: withKey ? it.chain.holder : null,
    from: c.genesisH,
  }, progress);
}

function card(c, it) {
  const l = listing(c, it), name = `${c.label} #${it.num}`;
  const status = it.burned ? `<span class="badge bad">Burned</span>` : mine(c, it) ? `<span class="badge">Yours</span>` : l ? `<span class="meta">ends in ${left(l.expiry)}</span>` : `<span class="meta">Not listed</span>`;
  return `<a class="card" href="#/item/${c.coll}/${it.num}">
    ${art(it.art, name)}
    <div class="info"><div class="title"><span>#${it.num}</span>${status}</div>
      <div class="price">${l ? taz(l.price) : " "}</div></div>
    ${l && !mine(c, it) ? `<span class="act" aria-hidden="true">Buy now</span>` : ""}
  </a>`;
}

// ---------- pages ----------
function explore() {
  return `<section class="field"><div class="wrap">
      <h1>Collectibles that live inside shielded notes.</h1>
      <p>Each item is a Zcash note. Buying it moves the note and the payment in one transaction. Your browser checks the whole history against the chain before you pay.</p>
      <div class="cta"><a class="btn primary" href="#/collection/${SEALED}">Explore collections</a></div>
      <div class="art-stack">${art(0, "Mask #1")}${art(1, "Mask #2")}</div>
    </div></section>
    <section class="section"><div class="wrap"><h2>Collections</h2>
      <div class="collections">${COLLECTIONS.map((c) => {
        const listed = c.items.filter((i) => listing(c, i)).length, f = floor(c);
        return `<a class="ccard" href="#/collection/${c.coll}">
          <div class="mosaic">${c.items.map((i) => art(i.art, `${c.label} #${i.num}`)).join("")}</div>
          <div class="body"><div class="name">${esc(c.label)}</div><span class="badge ok">Genuine issuance (R1)</span>
            <dl><div><dt class="meta">Supply</dt><dd>${c.items.length} ${c.sealed ? "sealed" : "final"}</dd></div>
              <div><dt class="meta">Floor</dt><dd>${f ? taz(f) : "–"}</dd></div>
              <div><dt class="meta">Listed</dt><dd>${listed}</dd></div></dl></div></a>`;
      }).join("")}</div></div></section>`;
}

function collection(c, tab, listedOnly, sort) {
  let items = c.items.filter((i) => !listedOnly || listing(c, i));
  const price = (i) => listing(c, i)?.price ?? Infinity;
  items = [...items].sort((a, b) => (sort === "num" ? a.num - b.num : sort === "price-desc" ? (price(b) === Infinity ? -1 : price(a) === Infinity ? 1 : price(b) - price(a)) : price(a) - price(b)));
  const listed = c.items.filter((i) => listing(c, i)).length, f = floor(c);
  const body = tab === "items"
    ? `<div class="browse">
        <aside class="filters" aria-label="Filters"><fieldset><legend class="meta">Status</legend>
          <label><input type="radio" name="st" value="all" ${listedOnly ? "" : "checked"}> All</label>
          <label><input type="radio" name="st" value="listed" ${listedOnly ? "checked" : ""}> Listed</label></fieldset></aside>
        <div><div class="toolbar"><span class="meta">${items.length} item${items.length === 1 ? "" : "s"}</span>
            <select id="sort" aria-label="Sort"><option value="price-asc" ${sort === "price-asc" ? "selected" : ""}>Price low to high</option>
              <option value="price-desc" ${sort === "price-desc" ? "selected" : ""}>Price high to low</option>
              <option value="num" ${sort === "num" ? "selected" : ""}>Item number</option></select></div>
          <div class="grid">${items.map((i) => card(c, i)).join("")}</div></div></div>`
    : `<div class="section" style="max-width:720px"><dl class="rows">
        <div><dt>Collection id</dt><dd>${c.coll}</dd></div>
        <div><dt>Genesis tx</dt><dd>${c.genesis} (block ${c.genesisH})</dd></div>
        <div><dt>Supply</dt><dd>${c.items.length} items. ${c.sealed ? "Sealed: the final note went to an address nobody can spend from, so no more can ever be minted." : "Finalized: the issuer wrote a FINAL note."}</dd></div>
        <div><dt>How R1 is checked</dt><dd>Your browser walks the issuer's sequence notes from the genesis tx to FINAL with the public issuer key.</dd></div></dl></div>`;
  return `<div class="wrap">
    <div class="chead"><div class="logo">${art(c.items[0].art, c.label)}</div>
      <div><span class="meta">Collection · ${short(c.coll, 16)}</span><h1>${esc(c.label)}</h1>
        <span class="badge ok">Genuine issuance (R1)</span> <span class="badge">${c.sealed ? "Sealed supply" : "Final supply"}</span>
        <span class="mute issuer" style="margin-left:8px">Issuer key ${short(c.coll, 12)}</span></div></div>
    <dl class="stats"><div><dt class="meta">Floor</dt><dd>${f ? taz(f) : "–"}</dd></div>
      <div><dt class="meta">Listed</dt><dd>${listed} of ${c.items.length}</dd></div>
      <div><dt class="meta">Items</dt><dd>${c.items.length}</dd></div>
      <div><dt class="meta">Pool</dt><dd>Ironwood (shielded)</dd></div></dl>
    <div class="tabs" role="tablist">
      <button class="tab" role="tab" data-tab="items" aria-selected="${tab === "items"}">Items</button>
      <button class="tab" role="tab" data-tab="about" aria-selected="${tab === "about"}">About</button></div>
    ${body}</div>`;
}

const SALE_STEPS = [
  ["reserved", "Reserved", "Buyer's intent and a 30-minute lease"],
  ["disclosed", "Disclosed", "Vault key and carrier location, signed and encrypted to the buyer"],
  ["verified", "Verified by the buyer", "R2 and R3 from chain, in the buyer's browser"],
  ["offer", "Offer received", "The buyer's signed transaction, encrypted to the seller"],
  ["accepted", "Checked, signed, broadcast", "Price, expiry, shape and carrier checked; seller signs last"],
  ["confirmed", "Confirmed", "Item and payment moved in one transaction"],
];
const ORDER = ["reserved", "disclosed", "verified", "deposited", "proving", "offer", "accepted", "confirmed"];
const reached = (stage, s) => s && ORDER.indexOf(s.stage) >= ORDER.indexOf(stage);

function saleTimeline(s) {
  return `<ol class="steps">${SALE_STEPS.map(([k, title, sub]) => {
    const done = reached(k, s), now = !done && ORDER.indexOf(k) === ORDER.indexOf(s.stage) + 1;
    return `<li class="${done ? "done" : now ? "now" : ""}"><span>${title}<br><span class="state">${sub}</span></span><span></span></li>`;
  }).join("")}</ol>`;
}

function item(c, it) {
  const name = `${c.label} #${it.num}`, key = keyOf(c, it), l = S.listings[key], s = S.sale?.key === key && S.sale.stage !== "confirmed" ? S.sale : null;
  const holder = mine(c, it), buying = ROLE === "buyer" && s && s.stage !== "confirmed";
  const verified = (holder && S.check?.[key]) || (ROLE === "buyer" && s?.verify);
  let action;
  if (it.burned) action = `<div class="in"><span class="badge bad">Burned</span><p class="mute">This item's chain ended in a note that is not a valid carrier. It can never be bought or sold.</p></div>`;
  else if (holder && s && ROLE === "seller") action = `<div class="in"><p class="meta">Sale in progress · your open tab handles it</p>${saleTimeline(s)}</div>`;
  else if (holder) action = `<div class="in"><p class="meta">You hold this item</p>
      ${l ? `<div class="price-big">${taz(l.price)}</div><p class="mute">Listed, ends in ${left(l.expiry)} (block ${l.expiry})</p><button class="btn block" data-do="unlist">Cancel listing</button>`
          : `<button class="btn primary block" data-open="list">List for sale</button>`}</div>`;
  else if (buying) action = `<div class="in"><p class="meta">Your purchase</p><div class="price-big">${taz(s.price)}</div><button class="btn primary block" data-open="buy">Show progress</button></div>`;
  else if (s) action = `<div class="in"><p class="meta">Reserved</p><p class="mute">Another buyer holds the lease on this listing. It reopens if they do not finish.</p></div>`;
  else if (l) action = `<div class="in"><p class="meta">Price</p><div class="price-big">${taz(l.price)}</div>
      <p class="mute">${zats(l.price)} · testnet, no value · ends in ${left(l.expiry)} (block ${l.expiry})</p>
      <button class="btn primary block" data-open="buy">Buy now</button></div>`;
  else action = `<div class="in"><p class="meta">Not listed</p><p class="mute">The holder has not listed this item.</p></div>`;

  const v = verified;
  const prov = v
    ? `<dl class="rows checks">
        <div><dt><span class="badge ok">R1</span></dt><dd>${v.items} items issued, ${v.sealed ? "sealed" : "final"}; genesis ${short(c.genesis, 12)} at block ${c.genesisH}.</dd></div>
        <div><dt><span class="badge ${v.verdict === "HELD" ? "ok" : "bad"}">R2</span></dt><dd>${v.verdict === "HELD" ? `${v.hop} transfer${v.hop === 1 ? "" : "s"}, every successor bound to its spend in one Action.` : esc(v.verdict + (v.error ? ": " + v.error : ""))}</dd></div>
        <div><dt><span class="badge ${v.verdict === "HELD" ? "ok" : "bad"}">R3</span></dt><dd>${v.verdict === "HELD" ? "The vault opens the current carrier; it is unspent." : "not held"}</dd></div>
        <div><dt class="meta">Checked</dt><dd>${v.simulated ? "simulated in this demo (no public test key for this item)" : `in this browser: ${v.blocks.toLocaleString("en-US")} blocks to ${v.tip_checked}, ${v.seconds.toFixed(1)} s, chain data from zcash-testnet.chainsafe.dev`}</dd></div>
      </dl>
      <p class="meta" style="margin-top:16px">History</p>
      <ol class="timeline">${it.history.map(([what, h, tx]) => `<li><span>${esc(what)} <span class="mute">· block ${h}${tx ? ` · tx ${tx}…` : ""}</span></span></li>`).join("")}</ol>`
    : `<dl class="rows checks"><div><dt><span class="badge ok">R1</span></dt><dd>Issued in ${short(c.genesis, 12)} at block ${c.genesisH}; the sequence is ${c.sealed ? "sealed" : "final"}.</dd></div></dl>
      <div class="locked"><span class="badge wait">R2</span> <span class="badge wait">R3</span><br>The transfer chain and the current holder are checked in your browser once the seller discloses this item's vault key to you. That happens when you press Buy, before you pay anything.</div>`;

  const more = c.items.filter((x) => x !== it);
  return `<div class="wrap"><div class="item">
    <div class="media"><div class="frame">${art(it.art, name)}</div><p class="meta" style="margin-top:12px">Art: 386 bytes in the carrier memo, drawn from those bytes</p></div>
    <div class="side">
      <a href="#/collection/${c.coll}">${esc(c.label)}</a> <span class="badge ok">R1</span>
      <h1>${esc(name)}</h1>
      <span class="mute">Holder: ${holder ? "you" : "private (shielded)"}</span>
      <div class="chips"><span class="chip">Ironwood</span><span class="chip">Testnet</span><span class="chip">Item ${it.num} of ${c.items.length}</span><span class="chip">Hop ${it.hop}</span></div>
      <section class="panel" aria-label="Price">${action}</section>
      <section class="panel"><header><span>Provenance</span><span class="meta">${v ? "verified" : "R1 public, R2 and R3 after disclosure"}</span></header><div class="in">${prov}</div></section>
      <section class="panel"><header><span>Details</span></header><div class="in"><dl class="rows">
        <div><dt>Collection id</dt><dd>${c.coll}</dd></div><div><dt>Item</dt><dd>#${it.num}</dd></div>
        <div><dt>Standard</dt><dd>Carrier note v0 (ZCR1 memo), zero value</dd></div><div><dt>Metadata</dt><dd>Fully on chain</dd></div></dl></div></section>
      ${more.length ? `<p class="meta" style="margin-top:32px">More from this collection</p><div class="grid">${more.map((x) => card(c, x)).join("")}</div>` : ""}
    </div></div></div>`;
}

function account(tab) {
  const held = [];
  for (const c of COLLECTIONS) for (const it of c.items) if (mine(c, it)) held.push([c, it]);
  const listed = held.filter(([c, it]) => listing(c, it));
  const body = tab === "collected"
    ? (held.length ? `<div class="grid" style="margin-top:24px">${held.map(([c, it]) => card(c, it)).join("")}</div>` : `<p class="mute" style="margin-top:16px">Nothing yet. <a href="#/collection/${SEALED}">Browse a collection</a>.</p>`)
    : tab === "listed"
    ? `<dl class="rows" style="margin-top:16px">${listed.map(([c, it]) => `<div><dt>${esc(c.label)} #${it.num}</dt><dd>${taz(listing(c, it).price)} · ends block ${listing(c, it).expiry}</dd></div>`).join("") || "<p class='mute'>Nothing listed.</p>"}</dl>`
    : `<p class="mute" style="margin-top:16px">Activity is kept in this browser only. The site publishes no sale events.</p>
       <ol class="timeline">${S.feed.filter((f) => f.who === ROLE).reverse().map((f) => `<li><span>${esc(f.text)} <span class="mute">· ${new Date(f.at).toLocaleTimeString()}</span></span></li>`).join("") || "<li><span class='mute'>Nothing yet.</span></li>"}</ol>`;
  return `<div class="wrap"><div class="acct">
      <section class="panel bal"><header><span>Collectibles account</span><span class="meta">from your wallet signature</span></header><div class="in">
        <div class="price-big">${taz(S.bal[ROLE])}</div>
        <div class="btns"><button class="btn primary" data-do="deposit">Deposit</button><button class="btn" data-do="sweep" ${S.bal[ROLE] ? "" : "disabled"}>Sweep to wallet</button><button class="btn" disabled>Restore</button></div>
        <p class="mute" style="font-size:14px">Keep only what you plan to spend on items here. Your main ZEC stays in your wallet.</p></div></section></div>
    <div class="tabs" role="tablist">${["collected", "listed", "activity"].map((t) => `<button class="tab" role="tab" data-atab="${t}" aria-selected="${tab === t}">${t[0].toUpperCase() + t.slice(1)}</button>`).join("")}</div>
    ${body}<div style="height:96px"></div></div>`;
}

// ---------- modals ----------
let modal = null; // { name, c, it, ... }
const stepList = (rows) => `<ol class="steps">${rows.map(([cls, title, sub, right = ""]) => `<li class="${cls}"><span>${title}<br><span class="state">${sub}</span></span><span class="state">${esc(right)}</span></li>`).join("")}</ol>`;

function connectModal() {
  const done = S.connected[ROLE];
  return `<header><h2>Connect</h2><button class="x" data-close aria-label="Close">×</button></header><div class="in">
    ${stepList([
      ["done", "Wallet found", "Noir 1.0.37, testnet build"],
      [done ? "done" : "now", "Share your shielded address", "Used only as the destination for sweeps"],
      [done ? "done" : "", "Sign the collectibles key message", "Your browser derives your collectibles keys from this signature"],
    ])}
    <div class="notice">Only approve this message on this site. A site that gets it signed can move your carrier items and your collectibles balance. It cannot move your ZEC.</div>
    ${done ? `<button class="btn block" data-close>Done</button>` : `<button class="btn primary block" data-do="connect">Approve in Noir (simulated)</button>`}</div>`;
}

function listModal() {
  const m = modal, name = `${m.c.label} #${m.it.num}`;
  if (m.phase === "form") return `<header><h2>List ${esc(name)}</h2><button class="x" data-close aria-label="Close">×</button></header><div class="in form">
    <label>Price (TAZ)<input type="number" step="0.00001" min="0.00001" value="${m.price / 1e8}" id="lp"></label>
    <label>Listing ends<select id="lttl"><option value="500">in 500 blocks (about 10 h)</option><option value="1000" selected>in 1,000 blocks (about 21 h)</option><option value="2000">in 2,000 blocks (about 42 h)</option></select></label>
    <div class="sum"><div><span>Price</span><span id="lp1">${taz(m.price)}</span></div><div><span>Marketplace fee</span><span>0</span></div><div><span>Network fee</span><span>paid by the buyer</span></div><div class="total"><span>You receive</span><span id="lp2">${taz(m.price)}</span></div></div>
    <div class="notice">The buyer who reserves this listing receives this item's vault viewing key, where its carrier is, and a pay address made for this listing. With the key they can read the item's full history and could republish it. Only that buyer receives it.</div>
    <button class="btn primary block" data-do="list">List item</button></div>`;
  const pub = m.public ? `<pre class="json">${esc(JSON.stringify(m.public, null, 2))}</pre>` : "";
  return `<header><h2>List ${esc(name)}</h2><button class="x" data-close aria-label="Close">×</button></header><div class="in">
    ${stepList([
      [m.phase === "check" ? "now" : "done", "Check that you hold it", "R1 to R3 from chain, in this browser", m.checkNote || ""],
      [m.phase === "publish" ? "now" : m.phase === "done" ? "done" : "", "Publish the listing", "Only these six fields go to the order book"],
    ])}
    ${pub}
    ${m.phase === "done" ? `<button class="btn primary block" data-close>Done</button>` : m.phase === "failed" ? `<p class="mute">${esc(m.error)}</p><button class="btn block" data-close>Close</button>` : ""}</div>`;
}

function buyModal() {
  const m = modal, s = S.sale?.key === keyOf(m.c, m.it) ? S.sale : null, l = listing(m.c, m.it);
  const price = s ? s.price : l ? l.price : 0, total = price + FEE;
  const st = (k) => (reached(k, s) ? "done" : "");
  const next = s ? ORDER[ORDER.indexOf(s.stage) + 1] : "reserved";
  const cls = (k) => (st(k) || (k === next ? "now" : ""));
  const needDeposit = s && s.stage === "verified" && S.bal.buyer < total;
  const head = `<header><h2>Buy ${esc(m.c.label)} #${m.it.num}</h2><button class="x" data-close aria-label="Close">×</button></header>`;
  if (s?.stage === "confirmed") return `${head}<div class="in"><div class="success">${art(m.it.art, "")}<div><p class="meta">Done</p><h2 style="font-weight:400;margin:4px 0 8px">You got it.</h2><p class="mute">${esc(m.c.label)} #${m.it.num} is in a fresh vault of yours. One transaction moved the item and the payment (simulated: block ${s.block}).</p></div></div>
      <div class="btns" style="display:flex;gap:8px;margin-top:16px"><a class="btn primary" href="#/account" data-close>View in Account</a><button class="btn" data-close>Close</button></div></div>`;
  return `${head}<div class="in">
    <div class="sum" style="border-top:0;margin-top:0;padding-top:0"><div><span>Price</span><span>${taz(price)}</span></div><div><span>Network fee</span><span>${taz(FEE)}</span></div><div class="total"><span>Total</span><span>${taz(total)}</span></div></div>
    ${stepList([
      [cls("reserved"), "Reserve", "Your intent and a 30-minute lease on this listing", s ? "lease granted" : ""],
      [cls("disclosed"), "Seller's disclosure", "Vault key and carrier location, signed by the vault", reached("disclosed", s) ? "signature ok" : s ? "waiting for the seller's tab" : ""],
      [cls("verified"), "Verify from chain", "R1 to R3 in this browser before you pay", s?.verifyNote || ""],
      [cls("deposited"), "Deposit", `${taz(total)} to your collectibles account`, reached("deposited", s) ? "confirmed" : needDeposit ? `balance ${taz(S.bal.buyer)}` : ""],
      [cls("proving"), "Build and prove", "3 Actions, proved in this browser (simulated: about 10 s with threads)", s?.stage === "proving" ? `${s.pct || 0}%` : reached("offer", s) ? "done" : ""],
      [cls("offer"), "Send the offer", "Signed by you, encrypted to the seller; the seller signs last", reached("offer", s) ? "sent" : ""],
      [cls("accepted"), "Seller accepts", "The seller's tab checks it, signs and broadcasts", reached("accepted", s) ? "broadcast" : reached("offer", s) ? "waiting" : ""],
      [cls("confirmed"), "Confirmed", "Item and payment move in one transaction", ""],
    ])}
    ${needDeposit ? `<div class="notice">Your collectibles balance is ${taz(S.bal.buyer)}. Send ${taz(total)} from your wallet: a ZIP-321 request to your collectibles address, by Noir or any wallet (QR).</div><button class="btn primary block" data-do="pay">Pay with Noir (simulated)</button>` : ""}
    ${!s ? `<div class="notice">Until the offer expires (about 25 minutes) the seller decides whether to complete it. To cancel, spend your deposit back to yourself; that only counts once mined. Either the whole sale happens or your deposit stays yours.</div>
      ${S.connected.buyer ? `<button class="btn primary block" data-do="reserve">Reserve and verify</button>` : `<button class="btn primary block" data-open="connect">Connect first</button>`}` : ""}
    ${s?.error ? `<p class="mute">${esc(s.error)}</p>` : ""}
  </div>`;
}

function renderModal() {
  if (!modal) return $("#scrim").classList.remove("open");
  const html = { connect: connectModal, list: listModal, buy: buyModal }[modal.name]();
  $("#modal").innerHTML = html;
  hydrate($("#modal"));
  $("#scrim").classList.add("open");
}
function openModal(name) {
  const [, page, a, b] = location.hash.split("/");
  const c = findColl(a || ""), it = c && c.items.find((x) => x.num === +b);
  modal = { name, c, it, phase: "form", price: 10000 };
  renderModal();
  $("#modal").querySelector("button, input")?.focus();
}
function closeModal() { modal = null; renderModal(); }

// ---------- actions ----------
async function doList() {
  const m = modal, price = Math.round(Number($("#lp").value) * 1e8), ttl = Number($("#lttl").value);
  if (!(price > 0)) return;
  m.price = price;
  m.phase = "check";
  renderModal();
  let ok = true;
  if (m.it.chain) {
    const r = await chainCheck(m.c, m.it, true, (msg) => { m.checkNote = msg; renderModal(); });
    ok = r.verdict === "HELD";
    m.checkNote = ok ? `HELD, ${r.blocks.toLocaleString("en-US")} blocks, ${r.seconds.toFixed(1)} s` : r.verdict;
    if (ok) save((s) => { s.check = s.check || {}; s.check[keyOf(m.c, m.it)] = r; });
    else { m.phase = "failed"; m.error = `Not listed: ${r.verdict} ${r.error || ""}`; return renderModal(); }
  } else {
    await wait(900);
    m.checkNote = "simulated (no public test key for this item)";
  }
  m.phase = "publish";
  renderModal();
  await wait(700);
  const expiry = TIP + ttl;
  m.public = { listing_id: hex(32), coll: m.c.coll, num: m.it.num, price, expiry_height: expiry, seller_channel_key: hex(32) };
  m.phase = "done";
  save((s) => { s.listings[keyOf(m.c, m.it)] = { price, expiry, by: ROLE }; feed(s, ROLE, `Listed ${m.c.label} #${m.it.num} for ${taz(price)}`); });
}

function doReserve() {
  const m = modal, l = listing(m.c, m.it);
  if (!l || (S.sale && S.sale.stage !== "confirmed")) return;
  save((s) => { s.sale = { key: keyOf(m.c, m.it), price: l.price, stage: "reserved" }; feed(s, "buyer", `Reserved ${m.c.label} #${m.it.num}`); feed(s, "seller", `A buyer reserved ${m.c.label} #${m.it.num}`); });
}

// Each side reacts to the other's moves. The buyer's screen plays the seller too when no
// seller screen is open (single-tab use).
const running = new Set();
function once(tag, fn) {
  if (running.has(tag)) return;
  running.add(tag);
  fn().catch((e) => save((x) => { if (x.sale) x.sale.error = `Stopped: ${e.message || e}`; }));
}
function automate() {
  const s = S.sale;
  if (!s) return;
  const [coll, num] = [s.key.slice(0, 64), +s.key.slice(65)];
  const c = findColl(coll), it = c.items.find((x) => x.num === num);
  const actAsSeller = ROLE === "seller" || !sellerTabOpen();
  const tag = (k) => `${s.key}:${k}`;
  if (actAsSeller && s.stage === "reserved") once(tag("disclose"), async () => {
    await wait(1200);
    save((x) => { x.sale.stage = "disclosed"; feed(x, "seller", "Lease granted; disclosure signed and sent"); });
  });
  if (ROLE === "buyer" && s.stage === "disclosed") once(tag("verify"), async () => {
    let r;
    if (it.chain) {
      r = await chainCheck(c, it, true, (msg) => save((x) => { x.sale.verifyNote = msg; }));
    } else {
      await wait(1500);
      r = { verdict: "HELD", hop: it.hop, items: c.items.length, sealed: c.sealed, simulated: true };
    }
    save((x) => {
      if (r.verdict === "HELD") { x.sale.stage = "verified"; x.sale.verify = r; x.sale.verifyNote = r.simulated ? "simulated" : `HELD, ${r.seconds.toFixed(1)} s`; feed(x, "buyer", `Verified ${c.label} #${it.num} from chain`); }
      else { x.sale.error = `Stopped: the item did not verify (${r.verdict}${r.error ? ": " + r.error : ""}). Nothing was paid.`; }
    });
  });
  if (ROLE === "buyer" && s.stage === "verified" && S.bal.buyer >= s.price + FEE) once(tag("deposited"), async () => save((x) => { x.sale.stage = "deposited"; }));
  if (ROLE === "buyer" && s.stage === "deposited") once(tag("prove"), async () => {
    save((x) => { x.sale.stage = "proving"; x.sale.pct = 0; });
    for (let p = 10; p <= 100; p += 10) { await wait(950); save((x) => { x.sale.pct = p; }); }
    save((x) => { x.sale.stage = "offer"; feed(x, "buyer", "Offer signed and sent to the seller"); feed(x, "seller", "Offer received"); });
  });
  if (actAsSeller && s.stage === "offer") once(tag("accept"), async () => {
    await wait(1500);
    save((x) => { x.sale.stage = "accepted"; feed(x, "seller", `Offer checked (price ${taz(x.sale.price)}, 3 Actions, expiry, carrier unspent); signed and broadcast`); });
    await wait(3000);
    save((x) => {
      const k = x.sale.key;
      x.sale.stage = "confirmed";
      x.sale.block = TIP + 2;
      x.held.seller = x.held.seller.filter((h) => h !== k);
      x.held.buyer.push(k);
      x.bal.buyer -= x.sale.price + FEE;
      x.bal.seller += x.sale.price;
      delete x.listings[k];
      x.check = x.check || {};
      x.check[k] = x.sale.verify;
      feed(x, "seller", `Sold for ${taz(x.sale.price)}; paid to a fresh address of your collectibles account`);
      feed(x, "buyer", `Bought for ${taz(x.sale.price)}`);
    });
  });
}

// ---------- toasts: the latest events for this role ----------
let toastSeen = Date.now();
function renderToasts() {
  const box = $("#toasts");
  for (const f of S.feed.filter((f) => f.who === ROLE && f.at > toastSeen)) {
    const t = document.createElement("div");
    t.className = "toast";
    t.textContent = f.text;
    box.append(t);
    setTimeout(() => t.remove(), 5000);
  }
  toastSeen = Math.max(toastSeen, ...S.feed.map((f) => f.at));
}

// ---------- router ----------
let ui = { tab: "items", listedOnly: false, sort: "price-asc", atab: "collected" };
function render() {
  const [, page, a, b] = location.hash.split("/");
  const app = $("#app");
  document.querySelectorAll("[data-nav]").forEach((n) => n.removeAttribute("aria-current"));
  const conn = $("#connect");
  conn.textContent = S.connected[ROLE] ? (ROLE === "seller" ? "utest1…a7c2" : "utest1…k3f9") : "Connect";
  conn.classList.toggle("primary", !S.connected[ROLE]);
  $("#who").textContent = ROLE === "seller" ? "Seller" : "Buyer";
  let html;
  if (page === "collection" && findColl(a)) html = collection(findColl(a), ui.tab, ui.listedOnly, ui.sort);
  else if (page === "item" && findColl(a)) {
    const c = findColl(a), it = c.items.find((x) => x.num === +b);
    html = it ? item(c, it) : "<div class='wrap section'>No such item.</div>";
  } else if (page === "account") { html = account(ui.atab); $('[data-nav="account"]').setAttribute("aria-current", "page"); }
  else { html = explore(); $('[data-nav="explore"]').setAttribute("aria-current", "page"); }
  const y = window.scrollY;
  app.innerHTML = html;
  hydrate(app);
  window.scrollTo(0, y);
}
function changed() { render(); if (modal) renderModal(); automate(); renderToasts(); }

document.addEventListener("click", (e) => {
  const t = e.target.closest("button, a[data-close], [data-open]");
  if (!t) return;
  if (t.id === "connect") return openModal("connect");
  if (t.dataset.open) { e.preventDefault(); return openModal(t.dataset.open); }
  if (t.hasAttribute("data-close")) return closeModal();
  const d = t.dataset.do;
  if (d === "connect") return save((s) => { s.connected[ROLE] = true; feed(s, ROLE, "Connected Noir (simulated)"); });
  if (d === "list") return doList();
  if (d === "unlist") { const [, , a, b] = location.hash.split("/"); return save((s) => { delete s.listings[`${findColl(a).coll}/${b}`]; }); }
  if (d === "reserve") return doReserve();
  if (d === "pay") { t.disabled = true; return wait(1500).then(() => save((s) => { s.bal.buyer += s.sale.price + FEE; feed(s, "buyer", `Deposited ${taz(s.sale.price + FEE)} (simulated)`); })); }
  if (d === "deposit") return save((s) => { s.bal[ROLE] += 50000; feed(s, ROLE, `Deposited ${taz(50000)} (simulated)`); });
  if (d === "sweep") return save((s) => { feed(s, ROLE, `Swept ${taz(s.bal[ROLE])} to your wallet (simulated)`); s.bal[ROLE] = 0; });
  if (t.dataset.tab) { ui.tab = t.dataset.tab; return render(); }
  if (t.dataset.atab) { ui.atab = t.dataset.atab; return render(); }
});
document.addEventListener("change", (e) => {
  if (e.target.name === "st") { ui.listedOnly = e.target.value === "listed"; render(); }
  if (e.target.id === "sort") { ui.sort = e.target.value; render(); }
});
document.addEventListener("input", (e) => {
  if (e.target.id === "lp") { const v = `${e.target.value || 0} TAZ`; $("#lp1").textContent = v; $("#lp2").textContent = v; }
});
$("#scrim").addEventListener("click", (e) => { if (e.target.id === "scrim") closeModal(); });
document.addEventListener("keydown", (e) => { if (e.key === "Escape") closeModal(); });
window.addEventListener("hashchange", () => { ui = { ...ui, tab: "items", listedOnly: false }; window.scrollTo(0, 0); render(); });
window.zarrierReset = () => { localStorage.removeItem(STORE); S = load(); changed(); };
changed();
