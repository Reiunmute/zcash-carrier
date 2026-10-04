// zypher marketplace prototype: static mock data, no network, no keys. Layout after OpenSea,
// brand tokens (docs/marketplace/DESIGN.md). Routes follow SPEC section 7.
// Pages are built as HTML strings from the constant mock data below (labels go through esc).
// The real app renders chain and order-book data, which is untrusted: build DOM nodes there.
"use strict";

// Artwork payloads (v0: 48-byte palette + 338 bytes of 4-bit pixels, 26 by 26), the same bytes the
// carrier memos hold on chain (art/art1.bin, art/art2.bin).
const ART = ["0e0e14201e2c3a364e605c7cebe8e0c8c4ba96928cd4aa40fad6787848a0aa78d2285a78468caa0a0a0cffffff5a141e2111111111111111111112111111111111111111111111111111111111111113333311121111111111111113333333331111111211111113333333333333111111111111333333333333333111111111133333333333333333111121111333333444443333331111111133333344464453333331111111333334444c445533333111111133334444c6c455533331121113333344444644555333332111122224444446445555222211111222244487464875552222111112222444444644555522221111122224444446445555222211111222244444464455552222111112222444444644555522221111112222444666665552222121111122224444464455522221111111222224444644552222211111111222224446445222221111111bcbcbcbcbcbcbcbcbcbc111111cbcbcbcbcbcbcbcbcbcb111111bcbcbcbcbcbcbcbcbcbc112111cbcbcbcbcbcbcbcbcbcb111", "0e0e14201e2c3a364e605c7cebe8e0c8c4ba96928cd4aa40fad6787848a0aa78d2285a78468caa0a0a0cffffff5a141e1111111111111121111112111111111111111111111111111111111112111113333311111111111111111113333333331111111211111113333333333333111111111111333333333333333112111111133333333333333333111111111333333444443333331111111133333344464453333331111111333334444f445533333111111133334444f6f4555333311111133333444446445553333311111222244444464455552222111112222444a9464a95552222111112222444444644555522221111122224444446445555222211111222244444464455552222111112222444444644555522221211112222444666665552222121111122224444464455522221111111222224444644552222211111111222224446445222221111111bcbcbcbcbcbcbcbcbcbc111111cbcbcbcbcbcbcbcbcbcb111111bcbcbcbcbcbcbcbcbcbc111111cbcbcbcbcbcbcbcbcbcb121"];
const TIP = 4435640; // mock tip
const BLOCK_SECS = 75;

const COLLECTIONS = [
  {
    coll: "5fc17ef389fdae9b23142e4218153b17e74ae859d344667351a32e03d74e881f",
    label: "Masks, sealed",
    genesis: "7eab6bd6bee43d321e800d7a4565dc26d8ce9af29c2045f7fc15f9f8716aac49",
    genesisH: 4435127,
    finalTx: "512832b969c897752116be1649d5a982ad2a82e34199bc1d1e9f11bfef9b68f3",
    sealed: true,
    items: [
      { num: 1, art: 0, hop: 1, listed: { price: 10000, expiry: 4436636 }, history: [["Issued", 4435127, "7eab6bd6bee4"], ["Sold to a fresh vault", 4435628, "e96845ecfc84"]] },
      { num: 2, art: 1, hop: 0, listed: null, history: [["Issued", 4435129, "512832b969c8"]] },
    ],
  },
  {
    coll: "c7fe2aeadb5a62001b99d7f71b1a8eb9526d6b457d7f31d919ffaa73a31c7321",
    label: "Masks",
    genesis: "82562648ae6421901993cedbb72ffa7cf1ed4977ec33ccd1dbb8f91985fc1ea5",
    genesisH: 4416690,
    sealed: false,
    items: [
      { num: 1, art: 0, hop: 3, listed: { price: 25000, expiry: 4436100 }, history: [["Issued", 4416690, "82562648ae64"], ["Transferred", 4416716, "45056894d3e1"], ["Sold", 4425653, "6bdeb823ff65"], ["Sold", 4435126, "281788d78f3e"]] },
      { num: 2, art: 1, hop: 0, burned: true, listed: null, history: [["Issued", 4416701, "b193c439841d"], ["Burned: successor was not a carrier", 4416717, ""]] },
    ],
  },
];

const state = { connected: false, view: "visitor", held: new Set(["5fc17ef389fdae9b23142e4218153b17e74ae859d344667351a32e03d74e881f/1"]) };

// ---------- helpers ----------
const $ = (s, r = document) => r.querySelector(s);
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const short = (h, n = 8) => `${h.slice(0, n)}…`;
const taz = (z) => `${(z / 1e8).toFixed(8).replace(/0+$/, "").replace(/\.$/, "")} TAZ`;
const zats = (z) => `${z.toLocaleString("en-US")} zats`;
const left = (h) => {
  const s = (h - TIP) * BLOCK_SECS;
  return s <= 0 ? "ended" : s < 3600 ? `${Math.round(s / 60)} min` : `${Math.round(s / 3600)} h`;
};
const findColl = (c) => COLLECTIONS.find((x) => x.coll.startsWith(c));
const floor = (c) => Math.min(...c.items.filter((i) => i.listed).map((i) => i.listed.price));

function artCanvas(idx, label) {
  const hex = ART[idx];
  const b = Uint8Array.from(hex.match(/../g), (x) => parseInt(x, 16));
  const cv = document.createElement("canvas");
  cv.width = cv.height = 26;
  cv.className = "art";
  cv.setAttribute("role", "img");
  cv.setAttribute("aria-label", label);
  const ctx = cv.getContext("2d");
  const img = ctx.createImageData(26, 26);
  for (let p = 0; p < 676; p++) {
    const byte = b[48 + (p >> 1)];
    const k = p & 1 ? byte & 15 : byte >> 4;
    img.data.set([b[k * 3], b[k * 3 + 1], b[k * 3 + 2], 255], p * 4);
  }
  ctx.putImageData(img, 0, 0);
  return cv;
}
// Placeholders <i data-art="0" data-label=".."></i> become canvases after render.
const art = (idx, label) => `<i data-art="${idx}" data-label="${esc(label)}"></i>`;
function hydrate(root) {
  root.querySelectorAll("i[data-art]").forEach((el) => el.replaceWith(artCanvas(+el.dataset.art, el.dataset.label)));
}

function card(c, it) {
  const name = `${c.label} #${it.num}`;
  const status = it.burned ? `<span class="badge bad">Burned</span>` : it.listed ? `<span class="meta">ends in ${left(it.listed.expiry)}</span>` : `<span class="meta">Not listed</span>`;
  return `<a class="card" href="#/item/${c.coll}/${it.num}">
    ${art(it.art, name)}
    <div class="info">
      <div class="title"><span>#${it.num}</span>${status}</div>
      <div class="price">${it.listed ? taz(it.listed.price) : " "}</div>
    </div>
    ${it.listed ? `<span class="act" aria-hidden="true">Buy now</span>` : ""}
  </a>`;
}

// ---------- pages ----------
function explore() {
  return `<section class="field"><div class="wrap">
      <h1>Collectibles that live inside shielded notes.</h1>
      <p>Each item is a Zcash note. Buying it moves the note and the payment in one transaction. Your browser checks the whole history against the chain before you pay.</p>
      <div class="cta"><a class="btn primary" href="#/collection/${COLLECTIONS[0].coll}">Explore collections</a></div>
      <div class="art-stack">${art(0, "Mask #1")}${art(1, "Mask #2")}</div>
    </div></section>
    <section class="section"><div class="wrap">
      <h2>Collections</h2>
      <div class="collections">${COLLECTIONS.map((c) => {
        const listed = c.items.filter((i) => i.listed).length;
        return `<a class="ccard" href="#/collection/${c.coll}">
          <div class="mosaic">${c.items.map((i) => art(i.art, `${c.label} #${i.num}`)).join("")}</div>
          <div class="body">
            <div class="name">${esc(c.label)}</div>
            <span class="badge ok">Genuine issuance (R1)</span>
            <dl>
              <div><dt class="meta">Supply</dt><dd>${c.items.length} ${c.sealed ? "sealed" : "final"}</dd></div>
              <div><dt class="meta">Floor</dt><dd>${listed ? taz(floor(c)) : "–"}</dd></div>
              <div><dt class="meta">Listed</dt><dd>${listed}</dd></div>
            </dl>
          </div></a>`;
      }).join("")}</div>
    </div></section>`;
}

function collection(c, tab = "items", listedOnly = false, sort = "price-asc") {
  let items = c.items.filter((i) => !listedOnly || i.listed);
  items = [...items].sort((a, b) => {
    const pa = a.listed ? a.listed.price : Infinity, pb = b.listed ? b.listed.price : Infinity;
    return sort === "price-asc" ? pa - pb : sort === "price-desc" ? (pb === Infinity ? -1 : pa === Infinity ? 1 : pb - pa) : a.num - b.num;
  });
  const listed = c.items.filter((i) => i.listed).length;
  const body = tab === "items"
    ? `<div class="browse">
        <aside class="filters" aria-label="Filters">
          <fieldset><legend class="meta">Status</legend>
            <label><input type="radio" name="st" value="all" ${listedOnly ? "" : "checked"}> All</label>
            <label><input type="radio" name="st" value="listed" ${listedOnly ? "checked" : ""}> Listed</label>
          </fieldset>
        </aside>
        <div>
          <div class="toolbar"><span class="meta">${items.length} item${items.length === 1 ? "" : "s"}</span>
            <select id="sort" aria-label="Sort">
              <option value="price-asc" ${sort === "price-asc" ? "selected" : ""}>Price low to high</option>
              <option value="price-desc" ${sort === "price-desc" ? "selected" : ""}>Price high to low</option>
              <option value="num" ${sort === "num" ? "selected" : ""}>Item number</option>
            </select></div>
          <div class="grid">${items.map((i) => card(c, i)).join("")}</div>
        </div></div>`
    : `<div class="section" style="max-width:720px">
        <dl class="rows">
          <div><dt>Collection id</dt><dd>${c.coll}</dd></div>
          <div><dt>Genesis tx</dt><dd>${c.genesis} (block ${c.genesisH})</dd></div>
          <div><dt>Supply</dt><dd>${c.items.length} items. ${c.sealed ? "Sealed: the final note went to an address nobody can spend from, so no more can ever be minted." : "Finalized: the issuer wrote a FINAL note."}</dd></div>
          <div><dt>How R1 is checked</dt><dd>Your browser walks the issuer's sequence notes from the genesis tx to FINAL with the public issuer key. Nothing the server says is trusted.</dd></div>
        </dl></div>`;
  return `<div class="wrap">
    <div class="chead">
      <div class="logo">${art(c.items[0].art, c.label)}</div>
      <div><span class="meta">Collection · ${short(c.coll, 16)}</span>
        <h1>${esc(c.label)}</h1>
        <span class="badge ok">Genuine issuance (R1)</span> <span class="badge">${c.sealed ? "Sealed supply" : "Final supply"}</span>
        <span class="mute issuer" style="margin-left:8px">Issuer key ${short(c.coll, 12)}</span></div>
    </div>
    <dl class="stats">
      <div><dt class="meta">Floor</dt><dd>${listed ? taz(floor(c)) : "–"}</dd></div>
      <div><dt class="meta">Listed</dt><dd>${listed} of ${c.items.length}</dd></div>
      <div><dt class="meta">Items</dt><dd>${c.items.length}</dd></div>
      <div><dt class="meta">Pool</dt><dd>Ironwood (shielded)</dd></div>
    </dl>
    <div class="tabs" role="tablist">
      <button class="tab" role="tab" data-tab="items" aria-selected="${tab === "items"}">Items</button>
      <button class="tab" role="tab" data-tab="about" aria-selected="${tab === "about"}">About</button>
    </div>
    ${body}
  </div>`;
}

function item(c, it) {
  const name = `${c.label} #${it.num}`;
  const key = `${c.coll}/${it.num}`;
  const holder = state.view === "holder";
  const disclosed = state.view === "disclosed";
  let action;
  if (it.burned) action = `<div class="in"><span class="badge bad">Burned</span><p class="mute">This item's chain ended in a note that is not a valid carrier. It can never be bought or sold.</p></div>`;
  else if (holder) action = `<div class="in"><p class="meta">You hold this item</p>
      ${it.listed ? `<div class="price-big">${taz(it.listed.price)}</div><p class="mute">Listed, ends in ${left(it.listed.expiry)} (block ${it.listed.expiry})</p><button class="btn block">Cancel listing</button>` : `<button class="btn primary block" data-open="list">List for sale</button>`}</div>`;
  else if (it.listed) action = `<div class="in"><p class="meta">Price</p><div class="price-big">${taz(it.listed.price)}</div>
      <p class="mute">${zats(it.listed.price)} · testnet, no value · listing ends in ${left(it.listed.expiry)} (block ${it.listed.expiry})</p>
      <button class="btn primary block" data-open="buy">Buy now</button></div>`;
  else action = `<div class="in"><p class="meta">Not listed</p><p class="mute">The holder has not listed this item.</p></div>`;

  const r2r3 = disclosed || holder
    ? `<dl class="rows checks">
        <div><dt><span class="badge ok">R1</span></dt><dd>Issued in ${short(c.genesis, 12)} at block ${c.genesisH}; the sequence is ${c.sealed ? "sealed" : "final"}.</dd></div>
        <div><dt><span class="badge ok">R2</span></dt><dd>${it.hop} transfer${it.hop === 1 ? "" : "s"}; every successor was made in the same Action as the spend, with a valid carrier memo.</dd></div>
        <div><dt><span class="badge ok">R3</span></dt><dd>The ${holder ? "your" : "disclosed"} vault opens the current carrier and it is unspent.</dd></div>
        <div><dt class="meta">Trust</dt><dd>Chain data from two lightwalletd servers, identical in every block used. No proof-of-work check yet.</dd></div>
      </dl>
      <p class="meta" style="margin-top:16px">History</p>
      <ol class="timeline">${it.history.map(([what, h, tx]) => `<li><span>${esc(what)} <span class="mute">· block ${h}${tx ? ` · tx ${tx}…` : ""}</span></span></li>`).join("")}</ol>`
    : `<dl class="rows checks"><div><dt><span class="badge ok">R1</span></dt><dd>Issued in ${short(c.genesis, 12)} at block ${c.genesisH}; the sequence is ${c.sealed ? "sealed" : "final"}.</dd></div></dl>
      <div class="locked"><span class="badge wait">R2</span> <span class="badge wait">R3</span><br>The transfer chain and the current holder are checked in your browser once the seller discloses this item's vault key to you. That starts when you press Buy, before you pay anything.</div>`;

  const more = c.items.filter((x) => x !== it);
  return `<div class="wrap"><div class="item">
    <div class="media"><div class="frame">${art(it.art, name)}</div>
      <p class="meta" style="margin-top:12px">Art: 386 bytes in the carrier memo, drawn from those bytes</p></div>
    <div class="side">
      <a href="#/collection/${c.coll}">${esc(c.label)}</a> <span class="badge ok">R1</span>
      <h1>${esc(name)}</h1>
      <span class="mute">Holder: private (shielded)</span>
      <div class="chips"><span class="chip">Ironwood</span><span class="chip">Testnet</span><span class="chip">Item ${it.num} of ${c.items.length}</span><span class="chip">Hop ${it.hop}</span></div>
      <section class="panel" aria-label="Price">${action}</section>
      <section class="panel"><header><span>Provenance</span><span class="meta">${disclosed || holder ? "verified in this browser" : "R1 public, R2 and R3 after disclosure"}</span></header><div class="in">${r2r3}</div></section>
      <section class="panel"><header><span>Details</span></header><div class="in"><dl class="rows">
        <div><dt>Collection id</dt><dd>${c.coll}</dd></div>
        <div><dt>Item</dt><dd>#${it.num}</dd></div>
        <div><dt>Issuer key</dt><dd>${short(c.coll, 24)} (public)</dd></div>
        <div><dt>Standard</dt><dd>Carrier note v0 (ZCR1 memo), zero value</dd></div>
        <div><dt>Metadata</dt><dd>Fully on chain</dd></div>
      </dl></div></section>
      ${more.length ? `<p class="meta" style="margin-top:32px">More from this collection</p><div class="grid">${more.map((x) => card(c, x)).join("")}</div>` : ""}
    </div></div></div>`;
}

function account(tab = "collected") {
  const mine = [];
  for (const c of COLLECTIONS) for (const it of c.items) if (state.held.has(`${c.coll}/${it.num}`)) mine.push([c, it]);
  const body = tab === "collected"
    ? `<div class="grid" style="margin-top:24px">${mine.map(([c, it]) => card(c, it)).join("")}</div>`
    : tab === "listed"
    ? `<dl class="rows" style="margin-top:16px">${mine.filter(([, it]) => it.listed).map(([c, it]) => `<div><dt>${esc(c.label)} #${it.num}</dt><dd>${taz(it.listed.price)} · ends block ${it.listed.expiry} <button class="btn" style="margin-left:12px">Cancel</button></dd></div>`).join("") || "<p class='mute'>Nothing listed.</p>"}</dl>`
    : `<p class="mute" style="margin-top:16px">Activity is kept in this browser only. The site publishes no sale events.</p>
       <ol class="timeline"><li><span>Bought Masks, sealed #1 <span class="mute">· block 4435628</span></span></li><li><span>Deposited 0.0003 TAZ <span class="mute">· block 4435125</span></span></li></ol>`;
  return `<div class="wrap">
    <div class="acct">
      <section class="panel bal"><header><span>Collectibles account</span><span class="meta">from your wallet signature</span></header><div class="in">
        <div class="price-big">0.0025 TAZ</div>
        <div class="btns"><button class="btn primary">Deposit</button><button class="btn">Sweep to wallet</button><button class="btn">Restore</button></div>
        <p class="mute" style="font-size:14px">Keep only what you plan to spend on items here. Your main ZEC stays in your wallet.</p>
      </div></section>
    </div>
    <div class="tabs" role="tablist">
      ${["collected", "listed", "activity"].map((t) => `<button class="tab" role="tab" data-atab="${t}" aria-selected="${tab === t}">${t[0].toUpperCase() + t.slice(1)}</button>`).join("")}
    </div>
    ${body}
    <div style="height:96px"></div>
  </div>`;
}

// ---------- modals ----------
const MODALS = {
  connect: () => `<header><h2>Connect</h2><button class="x" data-close aria-label="Close">×</button></header><div class="in">
    <ol class="steps">
      <li class="done"><span>Wallet found<br><span class="state">Noir 1.0.37, testnet build</span></span><span></span></li>
      <li class="now"><span>Share your shielded address<br><span class="state">Used only as the destination for sweeps</span></span><span></span></li>
      <li><span>Sign the collectibles key message<br><span class="state">Your browser derives your collectibles keys from this signature</span></span><span></span></li>
    </ol>
    <div class="notice">Only approve this message on this site. A site that gets it signed can move your carrier items and your collectibles balance. It cannot move your ZEC.</div>
    <button class="btn primary block" data-do="connect">Approve in Noir</button></div>`,
  list: () => `<header><h2>List for sale</h2><button class="x" data-close aria-label="Close">×</button></header><div class="in form">
    <label>Price (TAZ)<input type="number" step="0.00001" min="0.00001" value="0.0001" id="lp"></label>
    <label>Listing ends<select><option>in 500 blocks (about 10 h)</option><option selected>in 1,000 blocks (about 21 h)</option><option>in 2,000 blocks (about 42 h)</option></select></label>
    <div class="sum"><div><span>Price</span><span id="lp1">0.0001 TAZ</span></div><div><span>Marketplace fee</span><span>0</span></div><div><span>Network fee</span><span>paid by the buyer</span></div><div class="total"><span>You receive</span><span id="lp2">0.0001 TAZ</span></div></div>
    <div class="notice">The buyer who reserves this listing receives this item's vault viewing key, where its carrier is, and a pay address made for this listing. With the key they can read the item's full history and could republish it. Only that buyer receives it.</div>
    <button class="btn primary block" data-close>List item</button></div>`,
  buy: () => `<header><h2>Buy</h2><button class="x" data-close aria-label="Close">×</button></header><div class="in">
    <div class="sum" style="border-top:0;margin-top:0;padding-top:0"><div><span>Price</span><span>0.0001 TAZ</span></div><div><span>Network fee</span><span>0.00015 TAZ</span></div><div class="total"><span>Total</span><span>0.00025 TAZ</span></div></div>
    <ol class="steps" id="buysteps">
      <li><span>Reserve<br><span class="state">Intent and a 30-minute lease</span></span><span class="state"></span></li>
      <li><span>Verify<br><span class="state">Seller's signed disclosure; R2 and R3 from chain</span></span><span class="state"></span></li>
      <li><span>Deposit<br><span class="state">0.00025 TAZ to your collectibles account, from your wallet</span></span><span class="state"></span></li>
      <li><span>Build and prove<br><span class="state">3 Actions, in this browser</span></span><span class="state"></span></li>
      <li><span>Sign and send to the seller<br><span class="state">Encrypted; the seller signs last</span></span><span class="state"></span></li>
      <li><span>Seller accepts<br><span class="state">Item and payment move in one transaction</span></span><span class="state"></span></li>
    </ol>
    <div class="notice">Until the offer expires (about 25 minutes) the seller decides whether to complete it. To cancel, spend your deposit back to yourself; that only counts once mined. Your funds are never at risk: either the whole sale happens or your deposit stays yours.</div>
    <button class="btn primary block" data-do="buy">Start</button></div>`,
};

function openModal(name) {
  $("#modal").innerHTML = MODALS[name]();
  $("#scrim").classList.add("open");
  $("#modal").querySelector("button, input")?.focus();
}
function closeModal() {
  $("#scrim").classList.remove("open");
}

function runBuy(btn) {
  btn.disabled = true;
  const steps = [...document.querySelectorAll("#buysteps li")];
  const times = ["0.3 s", "1.2 s", "confirmed", "3.9 s", "0.4 s", "mined"];
  let i = 0;
  const tick = () => {
    if (i > 0) { steps[i - 1].className = "done"; steps[i - 1].lastElementChild.textContent = times[i - 1]; }
    if (i === steps.length) { btn.textContent = "Done: the item is in Collected"; return; }
    steps[i].className = "now";
    i++;
    setTimeout(tick, 650);
  };
  tick();
}

// ---------- router ----------
let ui = { tab: "items", listedOnly: false, sort: "price-asc", atab: "collected" };
function render() {
  const [, page, a, b] = location.hash.split("/");
  const app = $("#app");
  let html;
  document.querySelectorAll("[data-nav]").forEach((n) => n.removeAttribute("aria-current"));
  $("#proto").hidden = page !== "item";
  if (page === "collection" && findColl(a)) html = collection(findColl(a), ui.tab, ui.listedOnly, ui.sort);
  else if (page === "item" && findColl(a)) {
    const c = findColl(a), it = c.items.find((x) => x.num === +b);
    html = it ? item(c, it) : "<div class='wrap section'>No such item.</div>";
    $("#proto").innerHTML = `<span class="meta">Prototype view</span>${["visitor", "disclosed", "holder"].map((v) => `<button data-view="${v}" aria-pressed="${state.view === v}">${v}</button>`).join("")}`;
  } else if (page === "account") { html = account(ui.atab); $('[data-nav="account"]').setAttribute("aria-current", "page"); }
  else { html = explore(); $('[data-nav="explore"]').setAttribute("aria-current", "page"); }
  app.innerHTML = html;
  hydrate(app);
}

document.addEventListener("click", (e) => {
  const t = e.target.closest("button, [data-open], [data-close]");
  if (!t) return;
  if (t.id === "connect") return openModal("connect");
  if (t.dataset.open) return openModal(t.dataset.open);
  if (t.hasAttribute("data-close")) return closeModal();
  if (t.dataset.do === "connect") { state.connected = true; $("#connect").textContent = "utest1…k3f9"; $("#connect").classList.remove("primary"); return closeModal(); }
  if (t.dataset.do === "buy") return runBuy(t);
  if (t.dataset.tab) { ui.tab = t.dataset.tab; return render(); }
  if (t.dataset.atab) { ui.atab = t.dataset.atab; return render(); }
  if (t.dataset.view) { state.view = t.dataset.view; return render(); }
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
render();
