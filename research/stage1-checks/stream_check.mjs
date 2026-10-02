// P6: does server-streaming GetBlockRange work over grpc-web, and does it carry Ironwood compact actions?
// Node 24 built-ins only. Read-only: calls OPTIONS and GetBlockRange, nothing else.
// usage: node stream_check.mjs [baseUrl] [start] [end] [pools]
//   baseUrl default https://zcash-testnet.chainsafe.dev (or http://127.0.0.1:8767 for relay.mjs)
//   pools: comma list of PoolType ints (2 SAPLING, 3 ORCHARD, 4 IRONWOOD); empty = server default
const BASE = process.argv[2] || 'https://zcash-testnet.chainsafe.dev';
const START = Number(process.argv[3] || 4425590), END = Number(process.argv[4] || 4425594);
const POOLS = (process.argv[5] || '').split(',').filter(Boolean).map(Number);
if (![START, END, ...POOLS].every(Number.isSafeInteger) || START < 1 || END < START || END - START > 20000) throw new Error('bad range or pools (max 20001 blocks)');
const ORIGIN = process.env.ORIGIN || 'https://marketplace.example';
const URL_ = BASE + '/cash.z.wallet.sdk.rpc.CompactTxStreamer/GetBlockRange';
// known facts (evidence): tx mined at 4425592 with 3 Ironwood actions, one of which has this cmx
const TXID = '6bdeb823ff6556dce7ef8c254c8ceaa257913cc820c43da271c52dabdc98ba5b'; // display order
const CMX = '5cb2cdaab67c89f8a2e084ef6d16e10e226ec5206597a9dabfce32cd8a37d720';

// --- protobuf, hand-rolled (field numbers from zcash_client_backend 0.24.0 proto/*.rs) ---
const varint = n => { const o = []; while (n > 127) { o.push((n & 127) | 128); n = Math.floor(n / 128); } o.push(n); return o; };
const ld = (f, bytes) => [(f << 3) | 2, ...varint(bytes.length), ...bytes];
const blockId = h => [0x08, ...varint(h)]; // BlockID.height = 1
// BlockRange { start = 1, end = 2, pool_types = 3 (packed enum) }
const msg = [...ld(1, blockId(START)), ...ld(2, blockId(END)), ...(POOLS.length ? ld(3, POOLS.flatMap(varint)) : [])];
const body = Buffer.concat([Buffer.from([0, 0, 0, 0, 0]), Buffer.from(msg)]); body.writeUInt32BE(msg.length, 1);

function* fields(b) {
  let i = 0;
  const rv = () => { let v = 0, s = 1, x; do { x = b[i++]; v += (x & 127) * s; s *= 128; } while (x & 128); return v; };
  while (i < b.length) {
    const k = rv(), f = Math.floor(k / 8), w = k & 7;
    if (w === 0) yield [f, rv()];
    else if (w === 2) { const n = rv(); yield [f, b.subarray(i, i += n)]; }
    else if (w === 1) yield [f, b.subarray(i, i += 8)];
    else if (w === 5) yield [f, b.subarray(i, i += 4)];
    else throw new Error('bad wire type ' + w);
  }
}
const hex = b => Buffer.from(b).toString('hex');
const rhex = b => Buffer.from(b).reverse().toString('hex');
const action = m => { const a = {}; for (const [f, v] of fields(m)) { if (f === 1) a.nf = hex(v); if (f === 2) a.cmx = hex(v); } return a; };
function tx(m) { // CompactTx { index 1, txid 2, spends 4, outputs 5, actions 6 (Orchard), ironwood_actions 9 }
  const t = { index: 0, txid: '', sapling: 0, orchard: [], ironwood: [] };
  for (const [f, v] of fields(m)) {
    if (f === 1) t.index = v; else if (f === 2) t.txid = rhex(v); // protocol order -> display hex
    else if (f === 4 || f === 5) t.sapling++; else if (f === 6) t.orchard.push(action(v)); else if (f === 9) t.ironwood.push(action(v));
  }
  return t;
}
function block(m) { // CompactBlock { height 2, hash 3, prev_hash 4, vtx 7, chain_metadata 8 { sapling 1, orchard 2, ironwood 3 } }
  const b = { height: 0, hash: '', prev: '', vtx: [], meta: {} };
  for (const [f, v] of fields(m)) {
    if (f === 2) b.height = v; else if (f === 3) b.hash = rhex(v); else if (f === 4) b.prev = rhex(v);
    else if (f === 7) b.vtx.push(tx(v));
    else if (f === 8) for (const [g, x] of fields(v)) b.meta[['', 'sapling', 'orchard', 'ironwood'][g] || g] = x;
  }
  return b;
}
const acHeaders = h => Object.fromEntries([...h].filter(([k]) => k.startsWith('access-control-') || k === 'vary'));

// --- 1. CORS preflight as a browser would send it ---
const tp = performance.now();
const pre = await fetch(URL_, { method: 'OPTIONS', headers: { origin: ORIGIN, 'access-control-request-method': 'POST', 'access-control-request-headers': 'content-type,x-grpc-web,x-user-agent' } });
console.log('preflight', pre.status, (performance.now() - tp).toFixed(0) + 'ms', JSON.stringify(acHeaders(pre.headers)));

// --- 2. streaming call, frames parsed incrementally as network chunks arrive ---
console.log(`POST ${URL_} range ${START}..${END} pools [${POOLS}]`);
const t0 = performance.now(); const ms = () => (performance.now() - t0).toFixed(0);
const res = await fetch(URL_, { method: 'POST', headers: { 'content-type': 'application/grpc-web+proto', 'x-grpc-web': '1', origin: ORIGIN }, body });
console.log('status', res.status, 'headers at', ms() + 'ms', res.headers.get('content-type'), JSON.stringify(acHeaders(res.headers)));
let buf = Buffer.alloc(0), chunks = 0, tFirst = null, trailer = '', prevHash = null, nBlocks = 0, nIw = 0, bytes = 0;
const found = { tx: null, cmx: null }, verbose = END - START <= 20, chainOk = [];
for await (const c of res.body) {
  chunks++; bytes += c.length; buf = Buffer.concat([buf, c]);
  while (buf.length >= 5 && buf.length >= 5 + buf.readUInt32BE(1)) {
    const n = buf.readUInt32BE(1), flag = buf[0], m = buf.subarray(5, 5 + n); buf = buf.subarray(5 + n);
    if (flag & 0x80) { trailer += m.toString(); continue; }
    tFirst ??= ms(); nBlocks++;
    const b = block(m);
    if (prevHash !== null) chainOk.push(b.prev === prevHash); prevHash = b.hash;
    const iw = b.vtx.reduce((s, t) => s + t.ironwood.length, 0); nIw += iw;
    if (verbose || iw) console.log(`  +${ms()}ms chunk#${chunks} block ${b.height} ${b.hash} txs=${b.vtx.length} ironwoodActions=${iw} meta=${JSON.stringify(b.meta)}`);
    for (const t of b.vtx) {
      if (verbose || t.ironwood.length) console.log(`    tx[${t.index}] ${t.txid} sapling=${t.sapling} orchard=${t.orchard.length} ironwood=${t.ironwood.length}`);
      for (const a of t.ironwood) if (verbose) console.log(`      iw nf=${a.nf} cmx=${a.cmx}`);
      if (t.txid === TXID) found.tx = { height: b.height, ironwood: t.ironwood.length };
      for (const a of [...t.ironwood, ...t.orchard]) if (a.cmx === CMX || rhex(Buffer.from(a.cmx, 'hex')) === CMX)
        found.cmx = { height: b.height, txid: t.txid, pool: t.ironwood.includes(a) ? 'ironwood' : 'orchard', byteOrder: a.cmx === CMX ? 'as-serialized' : 'reversed' };
    }
  }
}
console.log('trailer', JSON.stringify(trailer.trim()), 'leftover', buf.length);
console.log(`blocks=${nBlocks} ironwoodActions=${nIw} chunks=${chunks} bytes=${bytes} firstBlock=${tFirst}ms total=${ms()}ms prevHashLinks=${chainOk.filter(Boolean).length}/${chainOk.length}`);
console.log('known tx', TXID.slice(0, 12), found.tx ? JSON.stringify(found.tx) : 'NOT FOUND');
console.log('known cmx', CMX.slice(0, 12), found.cmx ? JSON.stringify(found.cmx) : 'NOT FOUND');
