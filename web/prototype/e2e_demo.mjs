// Playwright check of the prototype with real mouse clicks (SPEC 11 item 10): one whole sale on
// demo.html, both sides, plus layout checks at several widths. Screenshots go to .harness/runs/.
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

// 1. Layout: nothing in the top bar spills over, at phone, half-screen, tablet and desktop widths.
for (const width of [375, 512, 800, 1280]) {
  const page = await browser.newPage({ viewport: { width, height: 800 } });
  watch(page);
  for (const route of ['#/', ITEM, '#/account']) {
    await page.goto(`${BASE}/index.html?as=buyer${route}`);
    await page.waitForSelector('#app *');
    const spill = await page.evaluate(() => {
      const bad = [];
      for (const el of document.querySelectorAll('.top .wrap > *, .item .side > *, .acct *, .stats')) {
        const r = el.getBoundingClientRect();
        if (r.width && (r.right > innerWidth + 1 || r.left < -1)) bad.push(el.className || el.id || el.tagName);
      }
      return { bad, scroll: document.documentElement.scrollWidth > innerWidth + 1 };
    });
    check(!spill.scroll && !spill.bad.length, `${width}px ${route}: no horizontal overflow${spill.bad.length ? ' (' + spill.bad.join(', ') + ')' : ''}`);
  }
  await page.screenshot({ path: `${OUT}/item-${width}.png`, fullPage: true });
  await page.close();
}

// 2. One sale, both sides, with real clicks inside the two frames of demo.html.
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
watch(page);
await page.goto(`${BASE}/demo.html`);
await page.evaluate(() => localStorage.removeItem('zypher-demo-v1'));
await page.reload();
const seller = page.frameLocator('#seller'), buyer = page.frameLocator('#buyer');

await seller.locator('#connect').click();
await seller.locator('[data-do="connect"]').click();
await seller.locator('#modal [data-close].btn').click();
await seller.locator('.card', { hasText: '#1' }).first().click();
await seller.locator('[data-open="list"]').click();
await seller.locator('#lp').fill('0.0001');
await seller.locator('[data-do="list"]').click();
await seller.locator('pre.json').waitFor({ timeout: 60000 });
const pub = JSON.parse(await seller.locator('pre.json').textContent());
check(Object.keys(pub).sort().join() === 'coll,expiry_height,listing_id,num,price,seller_channel_key', 'public listing has exactly the six SPEC 3.1 fields');
check((await seller.locator('#modal .steps li').first().textContent()).includes('HELD'), 'seller holder check ran from chain (HELD)');
await page.screenshot({ path: `${OUT}/1-listed.png` });
await seller.locator('#modal [data-close].btn').click();

await buyer.locator('#connect').click();
await buyer.locator('[data-do="connect"]').click();
await buyer.locator('#modal [data-close].btn').click();
await buyer.locator('a.btn', { hasText: 'Explore collections' }).click();
await buyer.locator('.card', { hasText: '#1' }).first().click();
await buyer.locator('[data-open="buy"]').click();
await buyer.locator('[data-do="reserve"]').click();
await buyer.locator('[data-do="pay"]').waitFor({ timeout: 60000 });
check((await buyer.locator('#modal .steps').textContent()).includes('HELD'), 'buyer verified the item from chain (HELD) before paying');
await page.screenshot({ path: `${OUT}/2-verified.png` });
await buyer.locator('[data-do="pay"]').click();
await buyer.locator('#modal', { hasText: 'You got it.' }).waitFor({ timeout: 60000 });
await page.screenshot({ path: `${OUT}/3-bought.png` });
await buyer.locator('#modal a', { hasText: 'View in Account' }).click();
await buyer.locator('.card', { hasText: 'Yours' }).first().waitFor({ timeout: 5000 }).catch(() => {});
check(await buyer.locator('.card', { hasText: 'Yours' }).count() === 1, 'buyer account shows the item');
await seller.locator('.nav a', { hasText: 'Account' }).click();
check((await seller.locator('.price-big').textContent()).trim() === '0.0001 TAZ', 'seller balance shows the payment');
await page.screenshot({ path: `${OUT}/4-after.png` });

// 3. Narrow screen: one side at a time, the switch works.
await page.setViewportSize({ width: 800, height: 800 });
await page.reload();
check(await page.locator('#panes').getAttribute('data-show') === 'seller', 'narrow demo starts with one side');
await page.locator('button[data-show="buyer"]').click();
check(await page.locator('#buyer').isVisible() && !(await page.locator('#seller').isVisible()), 'switch shows the buyer side');
await page.screenshot({ path: `${OUT}/5-narrow.png` });

check(!errors.length, `no page errors${errors.length ? ': ' + errors.join(' | ') : ''}`);
await browser.close();
console.log(`screenshots in ${OUT}`);
process.exit(failed ? 1 : 0);
