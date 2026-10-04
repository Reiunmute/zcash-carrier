// The browser verifier (carrier-wasm `sale` feature) over grpc-web, run in Node 24. Same cases and
// verdicts as verify.sh, so wasm and the native CLI must agree. Read-only: GetLatestBlock,
// GetBlockRange and GetTransaction only.
// usage: node browser_verify.mjs <wasm-bindgen nodejs out dir> [grpc-web base URL]
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import * as lwd from '../../web/prototype/lib/lwd.mjs';

const require = createRequire(import.meta.url);
const w = require(resolve(process.argv[2], 'carrier_wasm.js'));
const BASE = process.argv[3] || 'https://zcash-testnet.chainsafe.dev';
const json = (p) => readFileSync(p, 'utf8');
const key = (file, name) => JSON.parse(json(file))[name].fvk;
const E1 = 'evidence/testnet-2026-09-29', E2 = 'evidence/testnet-2026-09-30', E3 = 'evidence/testnet-2026-10-02';
const K2 = `${E2}/viewing_keys.json`, K3 = `${E3}/viewing_keys.json`;
const P4_TX = 'e96845ecfc84008674195d8ee77fd433cdb7cdfc3464711ae92221d58ada3b1d';
const FROM = 4416690; // genesis of the 2026-09-29 collection, the oldest one checked

const t0 = Date.now();
const tip = await lwd.latestHeight(BASE);
const chain = new w.BrowserChain((txid) => lwd.getTransaction(BASE, txid));
let n = 0;
for await (const b of lwd.blockRange(BASE, FROM, tip)) { chain.add_block(b); n++; }
console.log(`indexed ${n} blocks ${FROM}..${tip} from ${BASE} in ${((Date.now() - t0) / 1000).toFixed(1)}s`);

const cases = [
  ['2026-09-29 package, former holder acct3', json(`${E1}/collection.json`), 1, json(`${E1}/package_item1.json`), null, key(K2, 'acct3'), 'SPENT'],
  ['forged item #2, victim acct3', json(`${E1}/collection.json`), 2, json(`${E1}/package_item2_forged.json`), null, key(K2, 'acct3'), 'FAIL'],
  ['forged item #2, real successor acct0', json(`${E1}/collection.json`), 2, json(`${E1}/package_item2_forged.json`), null, key(K2, 'acct0'), 'BURNED'],
  ['separate-seed sale, buyer seed acct6', json(`${E1}/collection.json`), 1, json(`${E3}/package_item1_hop3.json`), null, key(K3, 'buyer_acct6'), 'HELD'],
  ['no holder key', json(`${E1}/collection.json`), 1, json(`${E3}/package_item1_hop3.json`), null, null, 'UNVERIFIED'],
  ['P4 record walk, buyer seed vault 17', json(`${E3}/collection_sealed.json`), 1, null, P4_TX, key(K3, 'buyer_acct7'), 'HELD'],
  ['P4 record walk, former vault', json(`${E3}/collection_sealed.json`), 1, null, P4_TX, key(K3, 'sealed_acct2'), 'FAIL'],
];
let bad = 0;
for (const [name, col, item, pkg, walk, holder, want] of cases) {
  const t = Date.now();
  const r = JSON.parse(await chain.verify(col, item, pkg, walk, holder, tip));
  const ok = r.verdict === want;
  bad += !ok;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}: ${r.verdict}${r.hop !== undefined ? ` hop ${r.hop}` : ''}${r.error ? ` (${r.error})` : ''}, finalized=${r.finalized}, sealed=${r.sealed}, ${Date.now() - t} ms`);
  if (!ok) console.log(r.log.join('\n'));
}

// Lying or lagging servers must make verification fail, never pass.
const sealed = json(`${E3}/collection_sealed.json`), holder17 = key(K3, 'buyer_acct7');
const neg = async (name, ch, minTip, want) => {
  const r = JSON.parse(await ch.verify(sealed, 1, null, P4_TX, holder17, minTip));
  const ok = r.verdict === 'FAIL' && r.error.includes(want);
  bad += !ok;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}: ${r.verdict}${r.error ? ` (${r.error})` : ''}`);
};
// 1. blocks stop before the tip another server reported (the P4 sale is in the hidden part)
const short = new w.BrowserChain((txid) => lwd.getTransaction(BASE, txid));
for await (const b of lwd.blockRange(BASE, 4435127, 4435600)) short.add_block(b);
await neg('blocks end below the known tip', short, tip, 'below the highest known tip');
// 2. a server that reports a tx at a different height than the block that holds it
const liar = new w.BrowserChain(async (txid) => { const t = await lwd.getTransaction(BASE, txid); return txid === P4_TX ? { ...t, height: t.height + 1 } : t; });
for await (const b of lwd.blockRange(BASE, 4435127, tip)) liar.add_block(b);
await neg('tx height differs from its block', liar, tip, 'in the indexed blocks, not');
// 3. a tx that is in no indexed block (here: the index starts after it)
const late = new w.BrowserChain((txid) => lwd.getTransaction(BASE, txid));
for await (const b of lwd.blockRange(BASE, 4435200, tip)) late.add_block(b);
await neg('tx outside the indexed blocks', late, tip, 'not in the indexed blocks');
process.exit(bad ? 1 : 0);
