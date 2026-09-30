// Browser-origin test: can a web page on another origin reach testnet lightwalletd?
import { createRequire } from 'module';
const require = createRequire(process.env.PW_BASE || import.meta.url); // dir with node_modules/playwright
const { chromium } = require('playwright');
const browser = await chromium.launch(process.env.CHROME ? { executablePath: process.env.CHROME } : {});
const page = await browser.newPage();
await page.goto(process.env.PAGE || 'http://127.0.0.1:8766/index.html');
const targets = [
  'https://testnet.zec.rocks/cash.z.wallet.sdk.rpc.CompactTxStreamer/GetLatestBlock',
  'https://zcash-wallet-grpc-testnet.deltarpc.com/cash.z.wallet.sdk.rpc.CompactTxStreamer/GetLatestBlock',
  'https://zcash-testnet.chainsafe.dev/cash.z.wallet.sdk.rpc.CompactTxStreamer/GetLatestBlock',
  'https://zcash-mainnet.chainsafe.dev/cash.z.wallet.sdk.rpc.CompactTxStreamer/GetLatestBlock',
  'http://127.0.0.1:8799/cash.z.wallet.sdk.rpc.CompactTxStreamer/GetLatestBlock',
];
for (const u of targets) {
  const r = await page.evaluate(async (u) => {
    try {
      const body = new Uint8Array([0, 0, 0, 0, 0]);
      const res = await fetch(u, { method: 'POST', headers: { 'content-type': 'application/grpc-web+proto', 'x-grpc-web': '1' }, body });
      const buf = new Uint8Array(await res.arrayBuffer());
      // decode BlockID{height=1 (varint), hash=2} from the first grpc-web data frame
      let height = null;
      if (buf.length > 5 && buf[0] === 0) {
        const n = (buf[1] << 24) | (buf[2] << 16) | (buf[3] << 8) | buf[4];
        const m = buf.slice(5, 5 + n);
        if (m[0] === 0x08) { let v = 0, s = 0, i = 1; while (i < m.length) { const b = m[i++]; v += (b & 0x7f) * 2 ** s; s += 7; if (!(b & 0x80)) break; } height = v; }
      }
      return { ok: true, status: res.status, bytes: buf.length, height };
    } catch (e) { return { ok: false, err: String(e) }; }
  }, u);
  console.log(u, JSON.stringify(r));
}
await browser.close();
