// Real verification in the browser: carrier-core's rules R1 to R3 compiled to wasm, over chain
// data from a public lightwalletd via grpc-web. Everything the server returns is checked in wasm.
import init, { BrowserChain } from './pkg/carrier_wasm.js';
import * as lwd from './lib/lwd.mjs';

export const PUBLIC = 'https://zcash-testnet.chainsafe.dev';
let ready, server;
// Talk to the public server directly when this browser can reach it; otherwise use the relay
// that serves this page (scripts/serve_web.py locally, /lwd on the hosted site). The relay is not
// trusted either: the verifier checks every byte.
async function pickServer() {
  try {
    const t = AbortSignal.timeout ? AbortSignal.timeout(6000) : undefined;
    await lwd.latestHeight(PUBLIC, t);
    return PUBLIC;
  } catch {}
  try {
    const r = await fetch('/lwd/ping', { cache: 'no-store' });
    if (r.status === 204) return `${location.origin}/lwd`;
  } catch {}
  return PUBLIC;
}

// { collection, item, pkg?, walk?, holder?, from } -> verdict object from BrowserChain.verify
async function verifyFromChain({ collection, item, pkg = null, walk = null, holder = null, from }, progress = () => {}) {
  progress('loading the verifier');
  ready ||= init().catch((e) => { ready = null; throw new Error(`verifier did not load: ${e.message || e}`); });
  await ready;
  const t0 = performance.now();
  server ||= await pickServer();
  const SERVER = server;
  progress('asking the chain for its tip');
  const tip = await lwd.latestHeight(SERVER).catch((e) => { server = null; throw new Error(`chain server unreachable (${SERVER}): ${e.message || e}`); });
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

// The live testnet tip, for the header.
async function tip() {
  server ||= await pickServer();
  return lwd.latestHeight(server).catch((e) => { server = null; throw e; });
}

window.zarrierVerify = verifyFromChain;
window.zarrierTip = tip;
window.dispatchEvent(new Event('zarrier-verify-ready'));
