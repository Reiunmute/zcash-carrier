// Real verification in the browser: carrier-core's rules R1 to R3 compiled to wasm, over chain
// data from a public lightwalletd via grpc-web. Everything the server returns is checked in wasm.
import init, { BrowserChain } from './pkg/carrier_wasm.js';
import * as lwd from './lib/lwd.mjs';

export const SERVER = 'https://zcash-testnet.chainsafe.dev';
let ready;

// { collection, item, pkg?, walk?, holder?, from } -> verdict object from BrowserChain.verify
async function verifyFromChain({ collection, item, pkg = null, walk = null, holder = null, from }, progress = () => {}) {
  progress('loading the verifier');
  ready ||= init().catch((e) => { ready = null; throw new Error(`verifier did not load: ${e.message || e}`); });
  await ready;
  const t0 = performance.now();
  progress('asking the chain for its tip');
  const tip = await lwd.latestHeight(SERVER).catch((e) => { throw new Error(`chain server unreachable (${SERVER}): ${e.message || e}`); });
  const chain = new BrowserChain((txid) => lwd.getTransaction(SERVER, txid));
  let n = 0;
  for await (const b of lwd.blockRange(SERVER, from, tip)) {
    chain.add_block(b);
    if (++n % 1000 === 0) progress(`indexed ${n.toLocaleString('en-US')} of ${(tip - from + 1).toLocaleString('en-US')} blocks`);
  }
  progress('checking the rules');
  // One server for now; with our own node (P11) min_tip becomes the higher of the two tips.
  const r = JSON.parse(await chain.verify(JSON.stringify(collection), item, pkg && JSON.stringify(pkg), walk, holder, tip));
  r.blocks = n;
  r.seconds = (performance.now() - t0) / 1000;
  return r;
}

window.zarrierVerify = verifyFromChain;
window.dispatchEvent(new Event('zarrier-verify-ready'));
