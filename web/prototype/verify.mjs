// Real verification in the browser: carrier-core's rules R1 to R3 compiled to wasm, over chain
// data from a public lightwalletd via grpc-web. Everything the server returns is checked in wasm.
import init, { BrowserChain } from './pkg/carrier_wasm.js';
import * as lwd from './lib/lwd.mjs';

export const SERVER = 'https://zcash-testnet.chainsafe.dev';
let ready;

// { collection, item, pkg?, walk?, holder?, from } -> verdict object from BrowserChain.verify
async function verifyFromChain({ collection, item, pkg = null, walk = null, holder = null, from }, progress = () => {}) {
  ready ||= init();
  await ready;
  const t0 = performance.now();
  const tip = await lwd.latestHeight(SERVER);
  const chain = new BrowserChain((txid) => lwd.getTransaction(SERVER, txid));
  let n = 0;
  for await (const b of lwd.blockRange(SERVER, from, tip)) {
    chain.add_block(b);
    if (++n % 2500 === 0) progress(`indexed ${n.toLocaleString('en-US')} of ${(tip - from + 1).toLocaleString('en-US')} blocks`);
  }
  progress('checking the rules');
  const r = JSON.parse(await chain.verify(JSON.stringify(collection), item, pkg && JSON.stringify(pkg), walk, holder));
  r.blocks = n;
  r.seconds = (performance.now() - t0) / 1000;
  return r;
}

window.zypherVerify = verifyFromChain;
window.dispatchEvent(new Event('zypher-verify-ready'));
