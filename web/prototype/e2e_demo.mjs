// Playwright check of the marketplace with real mouse clicks: one site, two players.
// Player 1 lists an item; Player 2, in a second tab of the same browser, connects, buys, runs out
// of credit, deposits and finishes. Plus layout checks at several widths. Screenshots go to .harness/runs/.
// usage: node web/prototype/e2e_demo.mjs [base URL, default http://127.0.0.1:8790]
// Needs a running server (python3 scripts/serve_web.py) and Playwright; PW_BASE may name a
// directory whose node_modules has playwright (for example an npx cache entry).
import { createRequire } from 'node:module';
import { mkdirSync } from 'node:fs';

const require = createRequire(process.env.PW_BASE ? `${process.env.PW_BASE}/` : import.meta.url);
const { chromium } = require('playwright');
const BASE = process.argv[2] || 'http://127.0.0.1:8790';
const OUT = `.harness/runs/${new Date().toISOString().replace(/[:.]/g, '-')}`;
mkdirSync(OUT, { recursive: true });
const ITEM = '#/item/5fc17ef389fdae9b23142e4218153b17e74ae859d344667351a32e03d74e881f/1';
let failed = 0;
const check = (ok, what) => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`); failed += !ok; };

const browser = await chromium.launch();
const errors = [];
const watch = (page) => page.on('pageerror', (e) => errors.push(String(e)));

// 1. Layout: nothing spills over at phone, half-screen, tablet and desktop widths.
for (const width of [375, 512, 800, 1280]) {
  const ctx = await browser.newContext({ viewport: { width, height: 800 } });
  const page = await ctx.newPage();
  watch(page);
  for (const route of ['#/', ITEM, '#/account', '#/collection/5fc17ef389fdae9b23142e4218153b17e74ae859d344667351a32e03d74e881f']) {
    await page.goto(`${BASE}/index.html${route}`);
    await page.waitForSelector('#app *');
    if (route === '#/account' && !(await page.locator('.acct').count())) {
      await page.locator('#app [data-open="connect"]').click();
      await page.locator('[data-pick="p1"]').click();
    }
    const spill = await page.evaluate(() => {
      const bad = [];
      for (const el of document.querySelectorAll('.top .wrap > *, .item .side > *, .acct *, .stats, .hero *, .score *, .grid > *')) {
        const r = el.getBoundingClientRect();
        if (r.width && (r.right > innerWidth + 1 || r.left < -1)) bad.push(el.className || el.id || el.tagName);
      }
      return { bad, scroll: document.documentElement.scrollWidth > innerWidth + 1 };
    });
    check(!spill.scroll && !spill.bad.length, `${width}px ${route.slice(0, 20)}: no horizontal overflow${spill.bad.length ? ' (' + spill.bad.slice(0, 5).join(', ') + ')' : ''}`);
  }
  await page.goto(`${BASE}/index.html#/`);
  await page.waitForTimeout(400);
  await page.screenshot({ path: `${OUT}/home-${width}.png`, fullPage: true });
  await page.goto(`${BASE}/index.html${ITEM}`);
  await page.waitForTimeout(300);
  await page.screenshot({ path: `${OUT}/item-${width}.png`, fullPage: true });
  await ctx.close();
}

// 2. One sale on one site: two tabs, two players.
const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
const a = await ctx.newPage(); watch(a);
await a.goto(`${BASE}/index.html#/`);
await a.evaluate(() => { localStorage.clear(); sessionStorage.clear(); });
await a.reload();
await a.screenshot({ path: `${OUT}/0-home.png` });

// Player 1 connects and lists Masks, sealed #1.
await a.locator('#connect').click();
await a.locator('[data-pick="p1"]').click();
await a.locator('.nav a', { hasText: 'MY ITEMS' }).click();
await a.locator('.card', { hasText: '#1' }).first().click();
await a.locator('[data-open="list"]').click();
await a.locator('#lp').fill('0.0001');
await a.locator('[data-do="list"]').click();
await a.locator('pre.json').waitFor({ timeout: 90000 });
const pub = JSON.parse(await a.locator('pre.json').textContent());
check(Object.keys(pub).sort().join() === 'coll,expiry_height,listing_id,num,price,seller_channel_key', 'public listing has exactly the six SPEC 3.1 fields');
check((await a.locator('#modal .steps li').first().textContent()).includes('HELD'), 'holder check ran from chain before listing (HELD)');
await a.screenshot({ path: `${OUT}/1-listed.png` });
await a.locator('#modal [data-close].btn').click();

// Player 2 opens the same site in another tab.
const b = await ctx.newPage(); watch(b);
await b.setViewportSize({ width: 1440, height: 900 });
await b.goto(`${BASE}/index.html#/`);
check(await b.locator('.card', { hasText: '0.0001 TAZ' }).count() >= 1, 'the listing shows on the home page for everyone');
await b.locator('.card', { hasText: '0.0001 TAZ' }).first().click();
await b.locator('[data-open="buy"]').click();
await b.locator('#modal [data-open="connect"]').click();
await b.locator('[data-pick="p2"]').click();
check(await b.locator('#modal [data-do="reserve"]').count() === 1, 'connecting returns to the purchase');
await b.locator('[data-do="reserve"]').click();
await b.locator('#modal [data-open="deposit"]').waitFor({ timeout: 90000 });
check((await b.locator('#modal .steps').textContent()).includes('HELD'), 'buyer verified the item from chain (HELD) before paying');
await b.screenshot({ path: `${OUT}/2-verified-needs-funds.png` });
await b.locator('#modal [data-open="deposit"]').click();
await b.locator('[data-do="deposit"]').click();
await b.locator('.success-line').waitFor({ timeout: 10000 });
await b.screenshot({ path: `${OUT}/3-deposited.png` });
await b.locator('#modal [data-close].btn').click();
await b.locator('#modal', { hasText: 'YOU GOT IT.' }).waitFor({ timeout: 60000 });
await b.waitForTimeout(300);
await b.screenshot({ path: `${OUT}/4-bought.png` });
check((await b.locator('#bal').textContent()).trim() === '0.00075 TAZ', 'buyer credit: 0.001 deposited minus 0.0001 price minus 0.00015 fee');
await b.locator('#modal a', { hasText: 'VIEW MY ITEMS' }).click();
await b.locator('.card', { hasText: 'YOURS' }).first().waitFor({ timeout: 5000 }).catch(() => {});
check(await b.locator('.card', { hasText: 'YOURS' }).count() === 1, 'buyer now holds the item');

// Back in Player 1's tab: the sale arrived through the shared market.
await a.bringToFront();
await a.locator('.nav a', { hasText: 'MY ITEMS' }).click();
check((await a.locator('.price-big').first().textContent()).trim() === '0.0001 TAZ', 'seller credit shows the payment');
check(await a.locator('.card', { hasText: '#1' }).count() === 0 && await a.locator('.card', { hasText: '#2' }).count() === 1, 'seller keeps #2 only');
await a.screenshot({ path: `${OUT}/5-seller-after.png` });

// One tab can switch between players too.
await a.locator('#connect').click();
await a.locator('[data-pick="p2"]').click();
check((await a.locator('.price-big').first().textContent()).trim() === '0.00075 TAZ', 'switching account in one tab shows that player');

check(!errors.length, `no page errors${errors.length ? ': ' + errors.join(' | ') : ''}`);
await browser.close();
console.log(`screenshots in ${OUT}`);
process.exit(failed ? 1 : 0);
