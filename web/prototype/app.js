// Zarrier marketplace, testnet demo. One site for everyone: connect a wallet account, deposit,
// list, buy. Layout after OpenSea, arcade look after the Zarrier brand (docs/marketplace/DESIGN.md).
// Real: the "holds it" checks (rules R1 to R3 in wasm over Zcash testnet data, verify.mjs).
// Simulated: the wallet, deposits, proving, broadcast and mining. Nothing is sent anywhere.
// Pages are HTML strings built from the constant data below; labels go through esc.
"use strict";

// Artwork payloads (v0: 48-byte palette + 338 bytes of 4-bit pixels, 26 by 26), the same bytes the
// carrier memos hold on chain (art/art1.bin, art/art2.bin).
const ART = ["0e0e14201e2c3a364e605c7cebe8e0c8c4ba96928cd4aa40fad6787848a0aa78d2285a78468caa0a0a0cffffff5a141e2111111111111111111112111111111111111111111111111111111111111113333311121111111111111113333333331111111211111113333333333333111111111111333333333333333111111111133333333333333333111121111333333444443333331111111133333344464453333331111111333334444c445533333111111133334444c6c455533331121113333344444644555333332111122224444446445555222211111222244487464875552222111112222444444644555522221111122224444446445555222211111222244444464455552222111112222444444644555522221111112222444666665552222121111122224444464455522221111111222224444644552222211111111222224446445222221111111bcbcbcbcbcbcbcbcbcbc111111cbcbcbcbcbcbcbcbcbcb111111bcbcbcbcbcbcbcbcbcbc112111cbcbcbcbcbcbcbcbcbcb111", "0e0e14201e2c3a364e605c7cebe8e0c8c4ba96928cd4aa40fad6787848a0aa78d2285a78468caa0a0a0cffffff5a141e1111111111111121111112111111111111111111111111111111111112111113333311111111111111111113333333331111111211111113333333333333111111111111333333333333333112111111133333333333333333111111111333333444443333331111111133333344464453333331111111333334444f445533333111111133334444f6f4555333311111133333444446445553333311111222244444464455552222111112222444a9464a95552222111112222444444644555522221111122224444446445555222211111222244444464455552222111112222444444644555522221211112222444666665552222121111122224444464455522221111111222224444644552222211111111222224446445222221111111bcbcbcbcbcbcbcbcbcbc111111cbcbcbcbcbcbcbcbcbcb111111bcbcbcbcbcbcbcbcbcbc111111cbcbcbcbcbcbcbcbcbcb121"];
let TIP = 4494000; // replaced on load by the live testnet height (window.zarrierTip, verify.mjs)
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
const SEALED = COLLECTIONS[0].coll;
const FEE = 15000; // network fee for a 3-Action sale, paid by the buyer

const SALE_STEPS = [
  ["reserved", "Reserve", "Your intent and a 30-minute lease on this listing"],
  ["disclosed", "Seller's disclosure", "Vault key and carrier location, signed and encrypted to you"],
  ["verified", "Verify from chain", "R1 to R3 in this browser before you pay"],
  ["deposited", "Funds ready", "Price and network fee in your collectibles account"],
  ["proving", "Build and prove", "3 Actions, proved in this browser"],
  ["offer", "Send the offer", "Signed by you, encrypted to the seller"],
  ["accepted", "Seller signs and broadcasts", "Price, expiry and carrier checked; the seller signs last"],
  ["confirmed", "Confirmed", "Item and payment moved in one transaction"],
];
const ORDER = SALE_STEPS.map(([k]) => k);

// ---------- shared market state (localStorage: every tab of this browser sees the same market) ----------
// The active wallet account is per tab (sessionStorage), so two windows can be two different people.
const STORE = "zarrier-market-v2", ACTIVE = "zarrier-active-account";
// The tab that drives a purchase. Kept in sessionStorage so a reload keeps driving it.
const TAB = sessionStorage.getItem("zarrier-tab") || Math.random().toString(36).slice(2);
sessionStorage.setItem("zarrier-tab", TAB);
const LEASE_MS = 30 * 60 * 1000; // a reservation lapses after 30 minutes
const newAccount = (n) => ({ id: `p${n}`, n, addr: `utest1${hex(3)}…${hex(2)}`, wallet: `utest1${hex(3)}…${hex(2)}`, bal: 0 });
const WALLETS = { p1: "utest1nr8…4d2e", p2: "utest1nw3…9b7c" }; // each player's own Noir wallet (where withdrawals go)
const walletOf = (a) => a.wallet || WALLETS[a.id] || "utest1nq0…0000";
const WITHDRAW_FEE = 10000; // ZIP-317 fee for a 2-Action shielded send
function fresh() {
  const p1 = { id: "p1", n: 1, addr: "utest1qz7…a7c2", bal: 0 }, p2 = { id: "p2", n: 2, addr: "utest1xk4…k3f9", bal: 0 };
  return { accounts: [p1, p2], held: { p1: [`${SEALED}/1`, `${SEALED}/2`], p2: [] }, listings: {}, sale: null, check: {}, feed: [] };
}
const load = () => {
  let s;
  try { s = JSON.parse(localStorage.getItem(STORE)) || fresh(); } catch { s = fresh(); }
  if (s.sale && s.sale.stage !== "confirmed" && ORDER.indexOf(s.sale.stage) < ORDER.indexOf("offer") && Date.now() - (s.sale.at || 0) > LEASE_MS) s.sale = null;
  return s;
};
let S = load();
function save(mut) { S = load(); mut(S); localStorage.setItem(STORE, JSON.stringify(S)); changed(); }
function feed(s, who, text, kind = "") { s.feed.push({ who, text, kind, at: Date.now() }); }
window.addEventListener("storage", (e) => { if (e.key === STORE) { S = load(); changed(); } });
const me = () => S.accounts.find((a) => a.id === sessionStorage.getItem(ACTIVE)) || null;
const acct = (id) => S.accounts.find((a) => a.id === id);
const pname = (a) => `PLAYER ${a.n}`;

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
const holderOf = (k) => Object.keys(S.held).find((id) => S.held[id].includes(k)) || null;
const mine = (c, it) => !!me() && holderOf(keyOf(c, it)) === me().id;
const floor = (c) => { const p = c.items.map((i) => listing(c, i)).filter(Boolean).map((l) => l.price); return p.length ? Math.min(...p) : null; };
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
function hex(n) { return [...crypto.getRandomValues(new Uint8Array(n))].map((b) => b.toString(16).padStart(2, "0")).join(""); }
const fx = matchMedia("(prefers-reduced-motion: reduce)").matches ? null : true;

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
async function chainCheck(c, it, progress) {
  if (!window.zarrierVerify) await new Promise((r) => window.addEventListener("zarrier-verify-ready", r, { once: true }));
  return window.zarrierVerify({
    collection: { network: "test", coll: c.coll, issuer_fvk: c.issuer, genesis_txid: c.genesis },
    item: it.num, pkg: null, walk: it.chain.walk, holder: it.chain.holder, from: c.genesisH,
  }, progress);
}
// Errors go to the screen and, as a request line, to the local server's log (no data beyond the message).
function report(where, e) {
  const msg = `${where}: ${e && (e.message || e)}`.slice(0, 300);
  fetch(`/__client_error?${encodeURIComponent(msg)}&ua=${encodeURIComponent(navigator.userAgent)}`).catch(() => {});
  return msg;
}
window.addEventListener("error", (e) => report("page", e.message));
window.addEventListener("unhandledrejection", (e) => report("promise", e.reason));
const timeout = (p, ms, what) => Promise.race([p, new Promise((_, no) => setTimeout(() => no(new Error(`${what} took over ${ms / 1000} s`)), ms))]);
async function holdsIt(c, it, progress) {
  if (it.chain) {
    progress("starting the check");
    if (!window.zarrierVerify) await timeout(new Promise((r) => window.addEventListener("zarrier-verify-ready", r, { once: true })), 20000, "loading verify.mjs");
    return timeout(chainCheck(c, it, progress), 120000, "the chain check");
  }
  await wait(1400);
  return { verdict: "HELD", hop: it.hop, items: c.items.length, sealed: c.sealed, simulated: true };
}

// ---------- arcade effects: 8-bit sound (off by default), banners, pixel bursts ----------
let audio = null, soundOn = localStorage.getItem("zarrier-sound") === "on";
function beep(kind) {
  if (!soundOn) return;
  audio ||= new AudioContext();
  const seq = { click: [[660, 0.04]], ok: [[523, 0.07], [784, 0.09]], coin: [[988, 0.06], [1319, 0.18]],
    win: [[523, 0.09], [659, 0.09], [784, 0.09], [1047, 0.25]], bad: [[220, 0.2]] }[kind] || [[440, 0.05]];
  let t = audio.currentTime;
  for (const [f, d] of seq) {
    const o = audio.createOscillator(), g = audio.createGain();
    o.type = "square"; o.frequency.value = f; g.gain.setValueAtTime(0.06, t); g.gain.exponentialRampToValueAtTime(0.0001, t + d);
    o.connect(g).connect(audio.destination); o.start(t); o.stop(t + d); t += d;
  }
}
function banner(text, sub = "") {
  if (modal) return; // an open dialog already shows the state; the strip would cross it
  const b = $("#banner");
  b.innerHTML = `<div class="b1">${esc(text)}</div>${sub ? `<div class="b2">${esc(sub)}</div>` : ""}`;
  b.classList.remove("show"); void b.offsetWidth; b.classList.add("show");
}
function burst(x = innerWidth / 2, y = innerHeight / 2) {
  if (!fx) return;
  const cv = $("#burst"), ctx = cv.getContext("2d");
  // With a dialog open the particles fly behind it, so they never cover its text.
  if (modal) $("#scrim").prepend(cv); else document.body.append(cv);
  cv.width = innerWidth; cv.height = innerHeight;
  const cols = ["#F4B728", "#F4EFE2", "#F4B728", "#E8432E", "#5BC0EB"];
  const ps = Array.from({ length: 90 }, () => { const a = Math.random() * Math.PI * 2, v = 2 + Math.random() * 7;
    return { x, y, vx: Math.cos(a) * v, vy: Math.sin(a) * v, s: 4 + (Math.random() * 3 | 0) * 2, c: cols[Math.random() * cols.length | 0], life: 50 + Math.random() * 40 }; });
  let f = 0;
  (function tick() {
    ctx.clearRect(0, 0, cv.width, cv.height);
    for (const p of ps) { if (f > p.life) continue; p.x += p.vx; p.y += p.vy; p.vy += 0.12; p.vx *= 0.985; ctx.fillStyle = p.c; ctx.fillRect(p.x | 0, p.y | 0, p.s, p.s); }
    if (++f < 95) requestAnimationFrame(tick); else ctx.clearRect(0, 0, cv.width, cv.height);
  })();
}

// Starfield: three layers drifting down, like the attract screen of an old shooter.
(function stars() {
  const cv = $("#stars"), ctx = cv.getContext("2d"), cols = ["#F4EFE2", "#F4B728", "#9A928C", "#F4EFE2", "#E8432E", "#5BC0EB"];
  let W, H, st;
  const resize = () => { W = cv.width = innerWidth; H = cv.height = innerHeight;
    st = Array.from({ length: Math.round(W * H / 6000) }, () => ({ x: Math.random() * W, y: Math.random() * H, z: 1 + (Math.random() * 3 | 0), c: cols[Math.random() * cols.length | 0], t: Math.random() * 100 })); };
  resize(); addEventListener("resize", resize);
  let last = 0;
  (function frame(ts) {
    const dt = Math.min(50, ts - last || 16); last = ts;
    ctx.clearRect(0, 0, W, H);
    for (const s of st) {
      if (fx) { s.y += s.z * dt * 0.02; s.t += dt * 0.01; if (s.y > H) { s.y = -2; s.x = Math.random() * W; } }
      if (((s.t | 0) % 7) === 0 && s.z === 1) continue; // twinkle
      ctx.fillStyle = s.c; ctx.fillRect(s.x | 0, s.y | 0, s.z === 3 ? 3 : 2, s.z === 3 ? 3 : 2);
    }
    requestAnimationFrame(frame);
  })(0);
})();

// ---------- cards and pages ----------
function card(c, it) {
  const l = listing(c, it), name = `${c.label} #${it.num}`, own = mine(c, it);
  const status = it.burned ? `<span class="badge bad">BURNED</span>` : own ? `<span class="badge you">YOURS</span>` : l ? `<span class="meta">ends in ${left(l.expiry)}</span>` : `<span class="meta">not listed</span>`;
  return `<a class="card" href="#/item/${c.coll}/${it.num}">
    <div class="art-wrap">${art(it.art, name)}</div>
    <div class="info"><div class="title"><span>#${it.num}</span>${status}</div>
      <div class="price">${l ? taz(l.price) : "&nbsp;"}</div></div>
    ${l && !own ? `<span class="act" aria-hidden="true">BUY NOW</span>` : own && !l && !it.burned ? `<span class="act" aria-hidden="true">SELL</span>` : ""}
  </a>`;
}

function explore() {
  const all = COLLECTIONS.flatMap((c) => c.items.map((i) => [c, i]));
  const listed = all.filter(([c, i]) => listing(c, i));
  return `<section class="hero"><div class="wrap">
      <div class="copy">
        <p class="kicker">ZCASH TESTNET · SHIELDED POOL</p>
        <h1>COLLECTIBLES THAT LIVE INSIDE SHIELDED NOTES.</h1>
        <p class="lead">Every item is a private Zcash note. Buy it and the item and the payment move in one transaction. Your browser checks the item against the chain before you pay.</p>
        <div class="cta"><a class="btn primary big" href="#/collection/${SEALED}">▶ EXPLORE COLLECTIONS</a>
          ${me() ? "" : `<button class="btn ghost big" data-open="connect">CONNECT WALLET</button>`}</div>
      </div>
      <div class="formation" aria-hidden="true">
        <div class="row">${art(0, "")}${art(1, "")}${art(0, "")}</div>
        <div class="row">${art(1, "")}${art(0, "")}</div>
        <img class="ship" src="mark.svg" alt="">
      </div>
    </div></section>
    <section class="score"><div class="wrap">
      <div><span class="k">ITEMS</span><span class="v">${all.length}</span></div>
      <div><span class="k">LISTED</span><span class="v">${listed.length}</span></div>
      <div><span class="k">FLOOR</span><span class="v">${floor(COLLECTIONS[0]) ? taz(floor(COLLECTIONS[0])) : "–"}</span></div>
      <div><span class="k">OWNERS SHOWN</span><span class="v">0</span></div>
    </div></section>
    ${listed.length ? `<section class="section"><div class="wrap"><h2>ON SALE NOW</h2><div class="grid">${listed.map(([c, i]) => card(c, i)).join("")}</div></div></section>` : ""}
    <section class="section"><div class="wrap"><h2>COLLECTIONS</h2>
      <div class="collections">${COLLECTIONS.map((c) => {
        const n = c.items.filter((i) => listing(c, i)).length, f = floor(c);
        return `<a class="ccard" href="#/collection/${c.coll}">
          <div class="mosaic">${c.items.map((i) => art(i.art, `${c.label} #${i.num}`)).join("")}</div>
          <div class="body"><div class="name">${esc(c.label)}</div><span class="badge ok">GENUINE ISSUANCE (R1)</span>
            <dl><div><dt class="meta">Supply</dt><dd>${c.items.length} ${c.sealed ? "sealed" : "final"}</dd></div>
              <div><dt class="meta">Floor</dt><dd>${f ? taz(f) : "–"}</dd></div>
              <div><dt class="meta">Listed</dt><dd>${n}</dd></div></dl></div></a>`;
      }).join("")}</div></div></section>
    <section class="section how"><div class="wrap"><h2>HOW TO PLAY</h2><ol class="howto">
      <li><b>1</b><span>Connect wallet: pick a wallet account.</span></li>
      <li><b>2</b><span>Deposit test TAZ to your collectibles account.</span></li>
      <li><b>3</b><span>Buy an item. Your browser verifies it from chain first.</span></li>
      <li><b>4</b><span>Or list one you hold. Item and payment swap in one transaction.</span></li></ol></div></section>`;
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
          <label><input type="radio" name="st" value="listed" ${listedOnly ? "checked" : ""}> On sale</label></fieldset></aside>
        <div><div class="toolbar"><span class="meta">${items.length} item${items.length === 1 ? "" : "s"}</span>
            <select id="sort" aria-label="Sort"><option value="price-asc" ${sort === "price-asc" ? "selected" : ""}>Price low to high</option>
              <option value="price-desc" ${sort === "price-desc" ? "selected" : ""}>Price high to low</option>
              <option value="num" ${sort === "num" ? "selected" : ""}>Item number</option></select></div>
          <div class="grid">${items.map((i) => card(c, i)).join("")}</div></div></div>`
    : `<div class="section" style="max-width:760px"><dl class="rows">
        <div><dt>Collection id</dt><dd>${c.coll}</dd></div>
        <div><dt>Genesis tx</dt><dd>${c.genesis} (block ${c.genesisH})</dd></div>
        <div><dt>Supply</dt><dd>${c.items.length} items. ${c.sealed ? "Sealed: the final note went to an address nobody can spend from, so no more can ever be minted." : "Finalized: the issuer wrote a FINAL note."}</dd></div>
        <div><dt>How R1 is checked</dt><dd>Your browser walks the issuer's sequence notes from the genesis tx to FINAL with the public issuer key.</dd></div></dl></div>`;
  return `<div class="wrap">
    <div class="chead"><div class="logo">${art(c.items[0].art, c.label)}</div>
      <div><span class="meta">Collection · ${short(c.coll, 16)}</span><h1>${esc(c.label)}</h1>
        <span class="badge ok">GENUINE ISSUANCE (R1)</span> <span class="badge">${c.sealed ? "SEALED SUPPLY" : "FINAL SUPPLY"}</span></div></div>
    <dl class="stats"><div><dt class="meta">Floor</dt><dd>${f ? taz(f) : "–"}</dd></div>
      <div><dt class="meta">On sale</dt><dd>${listed} of ${c.items.length}</dd></div>
      <div><dt class="meta">Items</dt><dd>${c.items.length}</dd></div>
      <div><dt class="meta">Pool</dt><dd>Ironwood (shielded)</dd></div></dl>
    <div class="tabs" role="tablist">
      <button class="tab" role="tab" data-tab="items" aria-selected="${tab === "items"}">ITEMS</button>
      <button class="tab" role="tab" data-tab="about" aria-selected="${tab === "about"}">ABOUT</button></div>
    ${body}</div>`;
}

const reached = (stage, s) => s && ORDER.indexOf(s.stage) >= ORDER.indexOf(stage);

function provenance(c, it, v) {
  if (!v) return `<dl class="rows checks"><div><dt><span class="badge ok">R1</span></dt><dd>Issued in ${short(c.genesis, 12)} at block ${c.genesisH}; the sequence is ${c.sealed ? "sealed" : "final"}.</dd></div></dl>
      <div class="locked"><span class="badge wait">R2</span> <span class="badge wait">R3</span><br>The transfer chain and the current holder are checked in your browser once the seller discloses this item's vault key to you. That happens when you press Buy, before you pay anything.</div>`;
  return `<dl class="rows checks">
      <div><dt><span class="badge ok">R1</span></dt><dd>${v.items} items issued, ${v.sealed ? "sealed" : "final"}; genesis ${short(c.genesis, 12)} at block ${c.genesisH}.</dd></div>
      <div><dt><span class="badge ${v.verdict === "HELD" ? "ok" : "bad"}">R2</span></dt><dd>${v.verdict === "HELD" ? `${v.hop} transfer${v.hop === 1 ? "" : "s"}, every successor bound to its spend in one Action.` : esc(v.verdict + (v.error ? ": " + v.error : ""))}</dd></div>
      <div><dt><span class="badge ${v.verdict === "HELD" ? "ok" : "bad"}">R3</span></dt><dd>${v.verdict === "HELD" ? "The vault opens the current carrier; it is unspent." : "not held"}</dd></div>
      <div><dt class="meta">Checked</dt><dd>${v.simulated ? "simulated in this demo (no public test key for this item)" : `in this browser: ${v.blocks.toLocaleString("en-US")} blocks to ${v.tip_checked}, ${v.seconds.toFixed(1)} s, chain data from zcash-testnet.chainsafe.dev`}</dd></div>
    </dl>
    <p class="meta" style="margin-top:16px">History</p>
    <ol class="timeline">${it.history.map(([what, h, tx]) => `<li><span>${esc(what)} <span class="mute">· block ${h}${tx ? ` · tx ${tx}…` : ""}</span></span></li>`).join("")}</ol>`;
}

function item(c, it) {
  const name = `${c.label} #${it.num}`, key = keyOf(c, it), l = S.listings[key], u = me();
  const s = S.sale?.key === key && S.sale.stage !== "confirmed" ? S.sale : null;
  const own = mine(c, it), buying = u && s && s.buyer === u.id;
  const v = (own && S.check[key]) || (buying && s.verify);
  let action;
  if (it.burned) action = `<div class="in"><span class="badge bad">BURNED</span><p class="mute">This item's chain ended in a note that is not a valid carrier. It can never be bought or sold.</p></div>`;
  else if (own && s) action = `<div class="in"><p class="meta">Sale in progress</p><div class="price-big">${taz(s.price)}</div><p class="mute">A buyer reserved your listing. Zarrier answers for you while this site is open in your browser.</p></div>`;
  else if (own) action = `<div class="in"><p class="meta">You hold this item</p>
      ${l ? `<div class="price-big">${taz(l.price)}</div><p class="mute">On sale, ends in ${left(l.expiry)} (block ${l.expiry})</p><button class="btn block" data-do="unlist">CANCEL LISTING</button>`
          : `<button class="btn primary block" data-open="list">▶ LIST FOR SALE</button>`}</div>`;
  else if (buying) {
    const step = ORDER.indexOf(s.stage) < ORDER.indexOf("verified") ? 1 : ORDER.indexOf(s.stage) < ORDER.indexOf("offer") ? 2 : 3;
    const why = s.error ? "stopped, try again" : s.stage === "verified" && u.bal < s.price + FEE ? "waiting for your deposit" : step === 3 ? "waiting for the seller" : "in progress";
    action = `<div class="in"><p class="meta">You reserved this item · step ${step} of 3 · ${why}</p><div class="price-big">${taz(s.price)}</div>
      <button class="btn primary block" data-open="buy">▶ CONTINUE PURCHASE</button>
      ${step < 3 ? `<button class="btn block" data-do="cancelbuy">CANCEL</button>` : ""}</div>`;
  }
  else if (s) action = `<div class="in"><p class="meta">Reserved</p><p class="mute">Another player holds the lease on this listing. It reopens if they do not finish.</p></div>`;
  else if (l) action = `<div class="in"><p class="meta">Price</p><div class="price-big">${taz(l.price)}</div>
      <p class="mute">${zats(l.price)} · testnet, no value · ends in ${left(l.expiry)} (block ${l.expiry})</p>
      <button class="btn primary block" data-open="buy">▶ BUY NOW</button></div>`;
  else action = `<div class="in"><p class="meta">Not listed</p><p class="mute">The holder has not put this item on sale.</p></div>`;
  const more = c.items.filter((x) => x !== it);
  return `<div class="wrap"><div class="item">
    <div class="media"><div class="frame">${art(it.art, name)}</div><p class="meta" style="margin-top:12px">Art: 386 bytes in the carrier memo, drawn from those bytes</p></div>
    <div class="side">
      <a href="#/collection/${c.coll}" class="crumb">${esc(c.label)}</a> <span class="badge ok">R1</span>
      <h1>${esc(name)}</h1>
      <span class="mute">Holder: ${own ? "you" : "private (shielded)"}</span>
      <div class="chips"><span class="chip">IRONWOOD</span><span class="chip">TESTNET</span><span class="chip">ITEM ${it.num} OF ${c.items.length}</span><span class="chip">HOP ${it.hop}</span></div>
      <section class="panel" aria-label="Price">${action}</section>
      <section class="panel"><header><span>PROVENANCE</span><span class="meta">${v ? "verified" : "R1 public, R2 and R3 after disclosure"}</span></header><div class="in">${provenance(c, it, v)}</div></section>
      <section class="panel"><header><span>DETAILS</span></header><div class="in"><dl class="rows">
        <div><dt>Collection id</dt><dd>${c.coll}</dd></div><div><dt>Item</dt><dd>#${it.num}</dd></div>
        <div><dt>Standard</dt><dd>Carrier note v0 (ZCR1 memo), zero value</dd></div><div><dt>Metadata</dt><dd>Fully on chain</dd></div></dl></div></section>
      ${more.length ? `<p class="meta" style="margin-top:32px">More from this collection</p><div class="grid">${more.map((x) => card(c, x)).join("")}</div>` : ""}
    </div></div></div>`;
}

function account(tab) {
  const u = me();
  if (!u) return `<div class="wrap"><section class="gate"><h1>CONNECT WALLET</h1><p class="lead">Connect a wallet account to see your items, deposit and sell.</p>
      <button class="btn primary big" data-open="connect"><span class="blink">▶</span> CONNECT WALLET</button></section></div>`;
  const held = [];
  for (const c of COLLECTIONS) for (const it of c.items) if (mine(c, it)) held.push([c, it]);
  const listed = held.filter(([c, it]) => listing(c, it));
  const body = tab === "collected"
    ? (held.length ? `<div class="grid" style="margin-top:24px">${held.map(([c, it]) => card(c, it)).join("")}</div>` : `<p class="mute" style="margin-top:16px">Nothing yet. <a href="#/collection/${SEALED}">Browse a collection</a>.</p>`)
    : tab === "listed"
    ? `<dl class="rows" style="margin-top:16px">${listed.map(([c, it]) => `<div><dt>${esc(c.label)} #${it.num}</dt><dd>${taz(listing(c, it).price)} · ends block ${listing(c, it).expiry}</dd></div>`).join("") || "<p class='mute'>Nothing on sale.</p>"}</dl>`
    : `<p class="mute" style="margin-top:16px">Activity is kept in this browser only. The site publishes no sale events.</p>
       <ol class="timeline">${S.feed.filter((f) => f.who === u.id).reverse().map((f) => `<li><span>${esc(f.text)} <span class="mute">· ${new Date(f.at).toLocaleTimeString()}</span></span></li>`).join("") || "<li><span class='mute'>Nothing yet.</span></li>"}</ol>`;
  return `<div class="wrap"><div class="acct">
      <section class="panel bal"><header><span>${pname(u)} · COLLECTIBLES ACCOUNT</span><span class="meta">${esc(u.addr)}</span></header><div class="in">
        <p class="meta">Credit</p><div class="price-big">${taz(u.bal)}</div>
        <div class="btns"><button class="btn primary" data-open="deposit">▶ DEPOSIT</button><button class="btn" data-open="withdraw" ${u.bal > WITHDRAW_FEE ? "" : "disabled"}>WITHDRAW</button><button class="btn" data-open="connect">SWITCH ACCOUNT</button></div>
        <p class="mute" style="font-size:14px">Keep only what you plan to spend on items here. Your main ZEC stays in your wallet.</p></div></section>
      <section class="panel stats-p"><header><span>STATS</span></header><div class="in"><dl class="mini">
        <div><dt class="meta">Items held</dt><dd>${held.length}</dd></div><div><dt class="meta">On sale</dt><dd>${listed.length}</dd></div>
        <div><dt class="meta">Bought</dt><dd>${S.feed.filter((f) => f.who === u.id && f.kind === "bought").length}</dd></div><div><dt class="meta">Sold</dt><dd>${S.feed.filter((f) => f.who === u.id && f.kind === "sold").length}</dd></div></dl></div></section></div>
    <div class="tabs" role="tablist">${["collected", "listed", "activity"].map((t) => `<button class="tab" role="tab" data-atab="${t}" aria-selected="${tab === t}">${{ collected: "COLLECTED", listed: "ON SALE", activity: "ACTIVITY" }[t]}</button>`).join("")}</div>
    ${body}<div style="height:96px"></div></div>`;
}

// ---------- modals ----------
let modal = null;
const stepList = (rows) => `<ol class="steps">${rows.map(([cls, title, sub, right = ""]) => `<li class="${cls}"><span>${title}<br><span class="state">${sub}</span></span><span class="state">${esc(right)}</span></li>`).join("")}</ol>`;
const head = (t) => `<header><h2>${esc(t)}</h2><button class="x" data-close aria-label="Close">×</button></header>`;

function connectModal() {
  const u = me();
  return `${head(u ? "SWITCH ACCOUNT" : "CONNECT WALLET")}<div class="in">
    <p class="mute">Noir wallet, testnet build (simulated in this demo). Pick the account to play with.</p>
    <div class="accounts">${S.accounts.map((a) => `<button class="acc ${u?.id === a.id ? "on" : ""}" data-pick="${a.id}">
        <span class="pn">${pname(a)}</span><span class="ad">${esc(a.addr)}</span>
        <span class="st">${(S.held[a.id] || []).length} item${(S.held[a.id] || []).length === 1 ? "" : "s"} · ${taz(a.bal)}</span></button>`).join("")}
      <button class="acc new" data-do="newacct"><span class="pn">+ NEW ACCOUNT</span><span class="ad">a fresh wallet account</span></button></div>
    <div class="notice">Connecting signs one message that derives your collectibles keys. Only approve it on this site: it can move your carrier items and collectibles balance, never your ZEC.</div>
    ${u ? `<button class="btn block" data-do="disconnect">DISCONNECT</button>` : ""}</div>`;
}

function depositModal() {
  const u = me(), m = modal;
  if (!u) return connectModal();
  const qr = Array.from({ length: 121 }, (_, i) => `<i class="${(m.seed.charCodeAt(i % m.seed.length) * (i + 7)) % 3 ? "" : "on"}"></i>`).join("");
  return `${head("DEPOSIT")}<div class="in">
    <p class="mute">Send test TAZ to your collectibles account from Noir or any Zcash wallet. Free TAZ comes from the Zcash testnet faucet.</p>
    <div class="dep"><div class="qr" aria-hidden="true">${qr}</div>
      <div><p class="meta">Your deposit address</p><p class="addr">${esc(u.addr)}</p>
        <label class="meta" for="damt">Amount</label>
        <select id="damt"><option value="50000">0.0005 TAZ</option><option value="100000" selected>0.001 TAZ</option><option value="1000000">0.01 TAZ</option></select></div></div>
    ${m.phase === "wait" ? stepList([["done", "Sent from Noir", `tx ${m.txid.slice(0, 12)}… (simulated)`], ["now", "Waiting for 1 confirmation", "about 75 s on testnet, sped up here"]])
      : m.phase === "done" ? `<div class="success-line">+${taz(m.amount)} CREDIT</div><p class="mute" style="text-align:center;font-size:13px">tx ${m.txid.slice(0, 16)}… · block ${m.block} (simulated)</p><button class="btn primary block" data-close>CONTINUE</button>`
      : `<button class="btn primary block" data-do="deposit">▶ SEND FROM NOIR (SIMULATED)</button>`}</div>`;
}

function listModal() {
  const m = modal, name = `${m.c.label} #${m.it.num}`;
  if (m.phase === "form") return `${head(`LIST ${name}`)}<div class="in form">
    <label>Price (TAZ)<input type="number" step="0.00001" min="0.00001" value="${m.price / 1e8}" id="lp"></label>
    <label>Listing ends<select id="lttl"><option value="500">in 500 blocks (about 10 h)</option><option value="1000" selected>in 1,000 blocks (about 21 h)</option><option value="2000">in 2,000 blocks (about 42 h)</option></select></label>
    <div class="sum"><div><span>Price</span><span id="lp1">${taz(m.price)}</span></div><div><span>Marketplace fee</span><span>0</span></div><div><span>Network fee</span><span>paid by the buyer</span></div><div class="total"><span>You receive</span><span id="lp2">${taz(m.price)}</span></div></div>
    <div class="notice">The buyer who reserves this listing receives this item's vault viewing key, where its carrier is, and a pay address made for this listing. With the key they can read the item's full history and could republish it. Only that buyer receives it.</div>
    <button class="btn primary block" data-do="list">▶ LIST ITEM</button></div>`;
  const pub = m.public ? `<p class="meta">Public listing: these six fields are all the order book sees</p><pre class="json">${esc(JSON.stringify(m.public, null, 2))}</pre>` : "";
  return `${head(`LIST ${name}`)}<div class="in">
    ${stepList([
      [m.phase === "check" ? "now" : m.phase === "failed" ? "" : "done", "Check that you hold it", "R1 to R3 from chain, in this browser", m.checkNote || ""],
      [m.phase === "publish" ? "now" : m.phase === "done" ? "done" : "", "Publish the listing", "Only six fields go to the order book"],
    ])}
    ${pub}
    ${m.phase === "done" ? `<button class="btn primary block" data-close>DONE</button>` : m.phase === "failed" ? `<div class="notice">${esc(m.error)}</div><button class="btn primary block" data-do="relist">▶ TRY AGAIN</button><button class="btn block" data-close>CLOSE</button>` : ""}</div>`;
}

// The eight protocol stages, shown to the buyer as three steps.
const BUY_STEPS = [
  { title: "Check the item", sub: "Your browser verifies it on chain before you pay", from: "reserved", to: "verified" },
  { title: "Pay", sub: "Item and payment go in one transaction", from: "deposited", to: "offer" },
  { title: "Seller confirms", sub: "The seller signs last; then it is yours", from: "accepted", to: "confirmed" },
];
function buyModal() {
  const m = modal, u = me(), s = S.sale?.key === keyOf(m.c, m.it) ? S.sale : null, l = listing(m.c, m.it);
  const price = s ? s.price : l ? l.price : 0, total = price + FEE;
  const short_ = u && s && s.stage === "verified" && u.bal < total;
  const h = head(`BUY ${m.c.label} #${m.it.num}`);
  if (s?.stage === "confirmed") return `${h}<div class="in"><div class="success">${art(m.it.art, "")}<div><p class="meta">Stage clear</p><h2 class="got">YOU GOT IT.</h2><p class="mute">${esc(m.c.label)} #${m.it.num} is in a fresh vault of yours. One transaction moved the item and the payment (simulated: block ${s.block}).</p></div></div>
      <div class="btns" style="display:flex;gap:8px;margin-top:16px"><a class="btn primary" href="#/account" data-close>VIEW MY ITEMS</a><button class="btn" data-close>CLOSE</button></div></div>`;
  const idx = s ? ORDER.indexOf(s.stage) : -1;
  const note = (i) => {
    if (!s) return "";
    if (i === 0) return idx >= ORDER.indexOf("verified") ? (s.verifyNote || "HELD") : s.stage === "reserved" ? "reserving" : (s.verifyNote || "checking");
    if (i === 1) return idx >= ORDER.indexOf("offer") ? "sent" : s.stage === "proving" ? `building ${s.pct || 0}%` : s.stage === "deposited" ? "preparing" : short_ ? "deposit needed" : "";
    return s.stage === "accepted" ? "broadcast" : s.stage === "offer" ? "waiting for the seller" : "";
  };
  const rows = BUY_STEPS.map((g, i) => {
    const a = ORDER.indexOf(g.from), b = ORDER.indexOf(g.to);
    const cls = idx >= b ? "done" : s && idx + 1 >= a ? "now" : "";
    return [cls, g.title, g.sub, note(i)];
  });
  const pct = s ? Math.round(((idx + 1) / ORDER.length) * 100) : 0;
  return `${h}<div class="in">
    <div class="sum" style="border-top:0;margin-top:0;padding-top:0"><div><span>Price</span><span>${taz(price)}</span></div><div><span>Network fee</span><span>${taz(FEE)}</span></div><div class="total"><span>Total</span><span>${taz(total)}</span></div></div>
    ${s ? `<div class="bar" role="progressbar" aria-valuenow="${pct}" aria-valuemin="0" aria-valuemax="100"><i style="width:${pct}%"></i></div>` : ""}
    ${stepList(rows)}
    ${short_ ? `<div class="notice">Your credit is ${taz(u.bal)}. Deposit ${taz(total - u.bal)} more and the purchase continues by itself.</div><button class="btn primary block" data-open="deposit">▶ DEPOSIT</button>` : ""}
    ${!s ? `<p class="mute" style="font-size:14px">Either the whole sale happens or your funds stay yours.</p>
      ${u ? `<button class="btn primary block" data-do="reserve">▶ BUY FOR ${taz(price)}</button>` : `<button class="btn primary block" data-open="connect">▶ CONNECT WALLET TO BUY</button>`}` : ""}
    ${s?.error ? `<div class="notice">${esc(s.error)}</div><button class="btn primary block" data-do="retry">▶ TRY AGAIN</button>` : ""}
    ${s && idx < ORDER.indexOf("offer") ? `<button class="btn block" data-do="cancelbuy">CANCEL PURCHASE</button>` : ""}
  </div>`;
}

function withdrawModal() {
  const u = me(), m = modal;
  if (!u) return connectModal();
  const max = Math.max(0, u.bal - WITHDRAW_FEE);
  if (m.phase === "form") {
    const amt = m.amount ?? max;
    return `${head("WITHDRAW")}<div class="in form">
    <p class="mute">Send credit from your collectibles account back to a Zcash wallet. It is a normal shielded transaction.</p>
    <label>Amount (TAZ)<span class="amt"><input type="number" id="wamt" step="0.00001" min="0.00001" max="${max / 1e8}" value="${amt / 1e8}"><button class="btn" data-do="wmax" type="button">MAX</button></span></label>
    <label>To<input type="text" id="wto" value="${esc(m.to ?? walletOf(u))}" spellcheck="false" autocomplete="off"></label>
    <p class="mute" style="font-size:13px;margin-top:-8px">Default: your Noir wallet. Any shielded testnet address works.</p>
    <div class="sum"><div><span>Available</span><span>${taz(u.bal)}</span></div><div><span>Network fee</span><span>${taz(WITHDRAW_FEE)}</span></div><div class="total"><span>Leaves your credit</span><span id="wtot">${taz(amt + WITHDRAW_FEE)}</span></div></div>
    ${m.error ? `<div class="notice">${esc(m.error)}</div>` : ""}
    <button class="btn primary block" data-do="withdraw">▶ REVIEW AND SEND</button></div>`;
  }
  const at = ["prove", "broadcast", "confirm", "done"].indexOf(m.phase);
  const cls = (i) => (at > i ? "done" : at === i ? "now" : "");
  return `${head("WITHDRAW")}<div class="in">
    <div class="sum" style="border-top:0;margin-top:0;padding-top:0"><div><span>Amount</span><span>${taz(m.amount)}</span></div><div><span>Network fee</span><span>${taz(WITHDRAW_FEE)}</span></div><div><span>To</span><span>${esc(m.to)}</span></div></div>
    ${stepList([
      [cls(0), "Build and prove", "2 Actions, proved in this browser", m.phase === "prove" ? `${m.pct || 0}%` : at > 0 ? "done" : ""],
      [cls(1), "Broadcast", "Signed with your collectibles key", at > 1 ? `tx ${m.txid.slice(0, 12)}…` : at === 1 ? "sending" : ""],
      [cls(2), "Confirmed", "1 block, about 75 s on testnet, sped up here", at > 2 ? `block ${m.block}` : at === 2 ? "waiting for a block" : ""],
    ])}
    ${m.phase === "done" ? `<div class="success-line">-${taz(m.amount + WITHDRAW_FEE)} CREDIT</div><p class="mute" style="text-align:center;font-size:13px">tx ${m.txid}<br>(simulated in this demo)</p><button class="btn primary block" data-close>DONE</button>`
      : `<p class="mute" style="font-size:13px">You can close this window; the withdrawal keeps going.</p>`}</div>`;
}

async function doWithdraw() {
  const m = modal, u = me();
  const amount = Math.round(Number($("#wamt").value) * 1e8), to = $("#wto").value.trim();
  m.amount = amount; m.to = to; m.error = "";
  if (!(amount > 0)) m.error = "Enter an amount above zero.";
  else if (amount + WITHDRAW_FEE > u.bal) m.error = `Not enough credit: ${taz(amount)} plus the ${taz(WITHDRAW_FEE)} fee is more than ${taz(u.bal)}.`;
  else if (!/^(utest1|ztestsapling1)[0-9a-z…]{6,}$/.test(to)) m.error = "That does not look like a shielded testnet address (utest1…).";
  if (m.error) { beep("bad"); return renderModal(); }
  const id = u.id, show = () => { if (modal === m) renderModal(); };
  m.phase = "prove"; m.pct = 0; show();
  for (let p = 20; p <= 100; p += 20) { await wait(450); m.pct = p; show(); }
  m.phase = "broadcast"; show();
  await wait(800);
  m.txid = hex(32);
  save((s) => { const a = s.accounts.find((x) => x.id === id); a.bal -= amount + WITHDRAW_FEE;
    feed(s, id, `Withdrawal sent: ${taz(amount)} to ${to} · tx ${m.txid.slice(0, 12)}… (simulated)`); });
  m.phase = "confirm"; show();
  await wait(2200);
  m.block = TIP + 1; m.phase = "done";
  save((s) => feed(s, id, `Withdrawal confirmed in block ${m.block}`, "withdrawn"));
  beep("coin"); show();
}

function renderModal() {
  if (!modal) return $("#scrim").classList.remove("open");
  $("#modal").innerHTML = { connect: connectModal, list: listModal, buy: buyModal, deposit: depositModal, withdraw: withdrawModal }[modal.name]();
  hydrate($("#modal"));
  $("#scrim").classList.add("open");
}
let back = null; // a modal to return to (buy after deposit)
function openModal(name) {
  const [, page, a, b] = location.hash.split("/");
  const c = findColl(a || ""), it = c && c.items.find((x) => x.num === +b);
  if (modal && (name === "deposit" || name === "connect")) back = modal;
  modal = { name, c, it, phase: "form", price: 10000, seed: hex(8) };
  if (name === "buy") takeOver();
  renderModal();
  $("#modal").querySelector("button:not(.x), input, select")?.focus();
}
function closeModal() {
  if (back && (modal?.name === "deposit" || modal?.name === "connect")) { modal = back; back = null; return renderModal(); }
  modal = null; back = null; renderModal();
}

// ---------- actions ----------
async function doList() {
  const m = modal, u = me(), price = Math.round(Number($("#lp").value) * 1e8), ttl = Number($("#lttl").value);
  if (!(price > 0) || !u) return;
  m.price = price; m.phase = "check"; renderModal();
  let r;
  try { r = await holdsIt(m.c, m.it, (msg) => { m.checkNote = msg; renderModal(); }); }
  catch (e) { m.phase = "failed"; m.error = `The check could not run: ${report("list", e)}`; beep("bad"); return renderModal(); }
  if (r.verdict !== "HELD") { m.phase = "failed"; m.error = `Not listed: ${r.verdict} ${r.error || ""}`; beep("bad"); return renderModal(); }
  m.checkNote = r.simulated ? "simulated" : `HELD, ${r.blocks.toLocaleString("en-US")} blocks, ${r.seconds.toFixed(1)} s`;
  save((s) => { s.check[keyOf(m.c, m.it)] = r; });
  m.phase = "publish"; renderModal();
  await wait(700);
  const expiry = TIP + ttl;
  m.public = { listing_id: hex(32), coll: m.c.coll, num: m.it.num, price, expiry_height: expiry, seller_channel_key: hex(32) };
  m.phase = "done";
  save((s) => { s.listings[keyOf(m.c, m.it)] = { price, expiry, by: u.id }; feed(s, u.id, `Listed ${m.c.label} #${m.it.num} for ${taz(price)}`, "listed"); });
  beep("ok"); banner("READY!", `${m.c.label} #${m.it.num} is on sale`);
}

function doReserve() {
  const m = modal, u = me(), l = listing(m.c, m.it);
  if (!u || !l || (S.sale && S.sale.stage !== "confirmed")) return;
  if (l.by === u.id) return;
  save((s) => { s.sale = { key: keyOf(m.c, m.it), price: l.price, stage: "reserved", buyer: u.id, seller: l.by, driver: TAB, at: Date.now() };
    feed(s, u.id, `Reserved ${m.c.label} #${m.it.num}`); feed(s, l.by, `A buyer reserved ${m.c.label} #${m.it.num}`); });
  beep("click");
}

// A reload or another tab of the buyer picks the purchase up where it stopped.
function takeOver() {
  const s = S.sale, u = me();
  if (!s || s.stage === "confirmed" || !u || s.buyer !== u.id || s.driver === TAB) return;
  running.clear();
  save((x) => { x.sale.driver = TAB; delete x.sale.error; });
}
// The sale moves on its own: the buyer's browser verifies, proves and offers; the seller's side
// (its Zarrier session) discloses and accepts. In this demo one browser plays both.
const running = new Set();
function once(tag, fn) { if (running.has(tag)) return; running.add(tag); fn().catch((e) => { const msg = report("sale", e); save((x) => { if (x.sale) x.sale.error = `Stopped: ${msg}. Nothing was paid.`; }); }); }
function automate() {
  const s = S.sale;
  if (!s || s.stage === "confirmed" || s.driver !== TAB) return;
  const c = findColl(s.key.slice(0, 64)), it = c.items.find((x) => x.num === +s.key.slice(65));
  const tag = (k) => `${s.key}:${s.buyer}:${k}`;
  const buyer = acct(s.buyer);
  if (s.stage === "reserved") once(tag("disclose"), async () => { await wait(1200);
    save((x) => { x.sale.stage = "disclosed"; feed(x, x.sale.seller, "Lease granted; disclosure signed and sent"); }); });
  if (s.stage === "disclosed") once(tag("verify"), async () => {
    const r = await holdsIt(c, it, (msg) => save((x) => { x.sale.verifyNote = msg; }));
    save((x) => {
      if (r.verdict === "HELD") { x.sale.stage = "verified"; x.sale.verify = r; x.sale.verifyNote = r.simulated ? "simulated" : `HELD, ${r.seconds.toFixed(1)} s`; feed(x, x.sale.buyer, `Verified ${c.label} #${it.num} from chain`); }
      else x.sale.error = `Stopped: the item did not verify (${r.verdict}${r.error ? ": " + r.error : ""}). Nothing was paid.`;
    });
    if (r.verdict === "HELD") beep("ok");
  });
  if (s.stage === "verified" && buyer.bal >= s.price + FEE) once(tag("funds"), async () => { await wait(500); save((x) => { x.sale.stage = "deposited"; }); });
  if (s.stage === "deposited") once(tag("prove"), async () => {
    save((x) => { x.sale.stage = "proving"; x.sale.pct = 0; });
    for (let p = 10; p <= 100; p += 10) { await wait(650); save((x) => { x.sale.pct = p; }); }
    save((x) => { x.sale.stage = "offer"; feed(x, x.sale.buyer, "Offer signed and sent to the seller"); feed(x, x.sale.seller, "Offer received"); });
  });
  if (s.stage === "offer") once(tag("accept"), async () => {
    await wait(1400);
    save((x) => { x.sale.stage = "accepted"; feed(x, x.sale.seller, `Offer checked (price ${taz(x.sale.price)}, 3 Actions, expiry, carrier unspent); signed and broadcast`); });
    await wait(2600);
    save((x) => {
      const k = x.sale.key, b = x.accounts.find((a) => a.id === x.sale.buyer), sl = x.accounts.find((a) => a.id === x.sale.seller);
      x.sale.stage = "confirmed"; x.sale.block = TIP + 2;
      x.held[sl.id] = x.held[sl.id].filter((h) => h !== k);
      (x.held[b.id] ||= []).push(k);
      b.bal -= x.sale.price + FEE; sl.bal += x.sale.price;
      delete x.listings[k];
      x.check[k] = x.sale.verify;
      feed(x, sl.id, `Sold ${c.label} #${it.num} for ${taz(x.sale.price)}`, "sold");
      feed(x, b.id, `Bought ${c.label} #${it.num} for ${taz(x.sale.price)}`, "bought");
    });
  });
}

// Celebrate events that belong to the account active in this tab.
let seenAt = Date.now();
function celebrate() {
  const u = me(), box = $("#toasts");
  for (const f of S.feed.filter((f) => f.at > seenAt && u && f.who === u.id)) {
    const t = document.createElement("div");
    t.className = "toast"; t.textContent = f.text; box.append(t); setTimeout(() => t.remove(), 5000);
    if (f.kind === "bought") { banner("STAGE CLEAR!", "+1 ITEM"); burst(); beep("win"); }
    if (f.kind === "sold") { banner("SOLD!", `+${f.text.split(" for ")[1] || ""}`); burst(innerWidth / 2, innerHeight / 3); beep("coin"); }
  }
  seenAt = Math.max(seenAt, ...S.feed.map((f) => f.at));
}

// ---------- router ----------
let ui = { tab: "items", listedOnly: false, sort: "price-asc", atab: "collected" };
function render() {
  const [, page, a, b] = location.hash.split("/");
  const app = $("#app"), u = me();
  document.querySelectorAll("[data-nav]").forEach((n) => n.removeAttribute("aria-current"));
  const conn = $("#connect");
  conn.innerHTML = u ? `<span class="pn">${pname(u)}</span> <span class="ad">${esc(u.addr)}</span>` : "CONNECT WALLET";
  conn.classList.toggle("on", !!u);
  $("#hud-bal").hidden = !u;
  $("#hud-dep").hidden = !u;
  if (u) $("#bal").textContent = taz(u.bal);
  document.title = `Zarrier${u ? ` · ${pname(u)}` : ""} (testnet)`;
  let html;
  if (page === "collection" && findColl(a)) html = collection(findColl(a), ui.tab, ui.listedOnly, ui.sort);
  else if (page === "item" && findColl(a)) { const c = findColl(a), it = c.items.find((x) => x.num === +b); html = it ? item(c, it) : "<div class='wrap section'>No such item.</div>"; }
  else if (page === "account") { html = account(ui.atab); $('[data-nav="account"]').setAttribute("aria-current", "page"); }
  else { html = explore(); $('[data-nav="explore"]').setAttribute("aria-current", "page"); }
  const y = window.scrollY;
  app.innerHTML = html; hydrate(app); window.scrollTo(0, y);
}
function changed() { render(); if (modal) renderModal(); automate(); celebrate(); }

document.addEventListener("click", (e) => {
  const t = e.target.closest("button, a[data-close], [data-open]");
  if (!t) return;
  if (t.tagName === "BUTTON") beep("click");
  if (t.id === "connect") return openModal("connect");
  if (t.id === "reset") { localStorage.removeItem(STORE); sessionStorage.removeItem(ACTIVE); S = load(); modal = null; location.hash = "#/"; return changed(); }
  if (t.id === "sound") { soundOn = !soundOn; localStorage.setItem("zarrier-sound", soundOn ? "on" : "off"); t.textContent = soundOn ? "SOUND ON" : "SOUND OFF"; t.setAttribute("aria-pressed", String(soundOn)); return beep("ok"); }
  if (t.dataset.open) { e.preventDefault(); return openModal(t.dataset.open); }
  if (t.hasAttribute("data-close")) return closeModal();
  if (t.dataset.pick) {
    sessionStorage.setItem(ACTIVE, t.dataset.pick);
    const a = me();
    save((s) => feed(s, a.id, `Connected as ${pname(a)}`));
    modal = back; back = null; renderModal();
    banner(pname(a), "READY"); beep("coin");
    return;
  }
  const d = t.dataset.do;
  if (d === "newacct") return save((s) => { const n = Math.max(...s.accounts.map((a) => a.n)) + 1; const a = newAccount(n); s.accounts.push(a); s.held[a.id] = []; });
  if (d === "disconnect") { sessionStorage.removeItem(ACTIVE); modal = null; return changed(); }
  if (d === "list") return doList();
  if (d === "relist") { modal.phase = "form"; renderModal(); return; }
  if (d === "unlist") { const [, , a, b] = location.hash.split("/"); return save((s) => { delete s.listings[`${findColl(a).coll}/${b}`]; feed(s, me().id, "Listing cancelled"); }); }
  if (d === "reserve") return doReserve();
  if (d === "cancelbuy") { if (modal) { modal = null; back = null; renderModal(); }
    return save((s) => { if (s.sale && s.sale.buyer === me()?.id && ORDER.indexOf(s.sale.stage) < ORDER.indexOf("offer")) { feed(s, s.sale.buyer, "Purchase cancelled; nothing was paid"); feed(s, s.sale.seller, "The buyer released the reservation"); s.sale = null; } }); }
  if (d === "retry") { running.clear(); return save((s) => { if (s.sale) { delete s.sale.error; s.sale.driver = TAB; if (s.sale.stage === "verified" || s.sale.stage === "disclosed") s.sale.stage = "disclosed"; } }); }
  if (d === "deposit") {
    const m = modal, amount = Number($("#damt").value), id = me().id;
    m.phase = "wait"; m.amount = amount; m.txid = hex(32); renderModal();
    return wait(2200).then(() => { m.phase = "done"; m.block = TIP + 1; save((s) => { s.accounts.find((a) => a.id === id).bal += amount; feed(s, id, `Deposited ${taz(amount)} · tx ${m.txid.slice(0, 12)}… (simulated)`); }); beep("coin"); });
  }
  if (d === "withdraw") return doWithdraw();
  if (d === "wmax") { const v = Math.max(0, me().bal - WITHDRAW_FEE); $("#wamt").value = v / 1e8; $("#wtot").textContent = taz(v + WITHDRAW_FEE); return; }
  if (t.dataset.tab) { ui.tab = t.dataset.tab; return render(); }
  if (t.dataset.atab) { ui.atab = t.dataset.atab; return render(); }
});
document.addEventListener("change", (e) => {
  if (e.target.name === "st") { ui.listedOnly = e.target.value === "listed"; render(); }
  if (e.target.id === "sort") { ui.sort = e.target.value; render(); }
});
document.addEventListener("input", (e) => {
  if (e.target.id === "wamt") { const v = Math.round(Number(e.target.value || 0) * 1e8); $("#wtot").textContent = taz(v + WITHDRAW_FEE); }
  if (e.target.id === "lp") { const v = `${e.target.value || 0} TAZ`; $("#lp1").textContent = v; $("#lp2").textContent = v; }
});
$("#scrim").addEventListener("click", (e) => { if (e.target.id === "scrim") closeModal(); });
document.addEventListener("keydown", (e) => { if (e.key === "Escape") closeModal(); });
window.addEventListener("hashchange", () => { ui = { ...ui, tab: "items", listedOnly: false }; window.scrollTo(0, 0); render(); });
$("#sound").textContent = soundOn ? "SOUND ON" : "SOUND OFF";
$("#tip").textContent = "…";
// Live testnet height, refreshed every block (about 75 s).
async function liveTip() {
  if (!window.zarrierTip) await new Promise((r) => window.addEventListener("zarrier-verify-ready", r, { once: true }));
  try { const h = await window.zarrierTip(); if (h > 0) { TIP = h; $("#tip").textContent = TIP.toLocaleString("en-US"); $("#hud-tip").classList.add("live"); render(); } }
  catch { $("#tip").textContent = "offline"; }
}
liveTip(); setInterval(liveTip, BLOCK_SECS * 1000);
changed();
