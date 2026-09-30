import { createRequire } from 'module';
const require = createRequire(process.env.PW_BASE || import.meta.url); // dir with node_modules/playwright
const { chromium } = require('playwright');
const browser = await chromium.launch(process.env.CHROME ? { executablePath: process.env.CHROME } : {});
const page = await browser.newPage();
page.on('console', m => console.log('[console]', m.type(), m.text().slice(0, 300)));
page.on('pageerror', e => console.log('[pageerror]', String(e).slice(0, 400)));
page.on('requestfailed', r => console.log('[reqfail]', r.url(), r.failure()?.errorText));
page.on('worker', w => console.log('[worker]', w.url()));
await page.goto(process.argv[2]);
try {
  await page.waitForFunction(() => window.__r, null, { timeout: Number(process.argv[3] || 300000) });
  console.log('result', JSON.stringify(await page.evaluate(() => window.__r)));
} catch (e) {
  console.log('timeout; page text:', await page.evaluate(() => document.body.innerText.slice(0, 300)), 'isolated:', await page.evaluate(() => crossOriginIsolated));
}
await browser.close();
