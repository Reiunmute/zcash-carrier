// Usage: node pw_noir.mjs <url>
import { createRequire } from 'module';
const require = createRequire(process.env.PW_BASE || import.meta.url); // dir with node_modules/playwright
const { chromium } = require('playwright');
const url = process.argv[2];
const browser = await chromium.launch(process.env.CHROME ? { executablePath: process.env.CHROME } : {});
const page = await browser.newPage();
page.on('console', m => { const t = m.text(); if (/error|Error|fail/i.test(t)) console.log('[console]', t.slice(0, 300)); });
page.on('pageerror', e => console.log('[pageerror]', String(e).slice(0, 300)));
await page.goto(url);
await page.waitForFunction(() => window.__results, null, { timeout: 180000 });
const r = await page.evaluate(() => window.__results);
const sig = x => x && x.result ? x.result.signature : (x && x.error) || null;
console.log('init', JSON.stringify(r.init).slice(0, 200));
for (const k of ['derived1', 'derived2', 'derived3', 'current1', 'current2', 'otherMsg']) {
  const v = r[k];
  console.log(k, 'sig', sig(v) ? String(sig(v)).slice(0, 40) + '..' : null, 'pubkey', v && v.result ? String(v.result.pubkey).slice(0, 20) : '', 'addr', v && v.result ? v.result.address : '');
}
const eq = (a, b) => sig(r[a]) !== null && sig(r[a]) === sig(r[b]);
console.log('derived deterministic (1=2=3):', eq('derived1', 'derived2') && eq('derived2', 'derived3'));
console.log('current deterministic (1=2):', eq('current1', 'current2'));
console.log('different message -> different sig:', sig(r.otherMsg) !== sig(r.derived1));
console.log('derived key != current key:', r.derived1?.result?.pubkey !== r.current1?.result?.pubkey);
if (r.error) console.log('error', r.error);
await browser.close();
