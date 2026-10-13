// lightwalletd over grpc-web, for the browser and Node 24 (fetch only, no dependencies).
// Hand-rolled protobuf; field numbers from zcash_client_backend 0.24.0 proto/*.rs.
// Every result is untrusted: the wasm verifier checks hash links, txids and compact/full agreement.
const SVC = '/cash.z.wallet.sdk.rpc.CompactTxStreamer/';
const MAX_RANGE = 10_000; // blocks per GetBlockRange request

const varint = (n) => { const o = []; while (n > 127) { o.push((n % 128) | 128); n = Math.floor(n / 128); } o.push(n); return o; };
const ld = (f, bytes) => [(f << 3) | 2, ...varint(bytes.length), ...bytes];
const frame = (msg) => { const b = new Uint8Array(5 + msg.length); new DataView(b.buffer).setUint32(1, msg.length); b.set(msg, 5); return b; };

function* fields(b) {
  let i = 0;
  const rv = () => { let v = 0, s = 1, x; do { x = b[i++]; v += (x & 127) * s; s *= 128; } while (x & 128); return v; };
  while (i < b.length) {
    const k = rv(), f = Math.floor(k / 8), w = k & 7;
    if (w === 0) yield [f, rv()];
    else if (w === 2) { const n = rv(); yield [f, b.subarray(i, (i += n))]; }
    else if (w === 1) yield [f, b.subarray(i, (i += 8))];
    else if (w === 5) yield [f, b.subarray(i, (i += 4))];
    else throw new Error('protobuf: wire type ' + w);
  }
}

// POSTs one grpc-web request and yields each response message (data frames), then checks the
// trailer's grpc-status.
async function* call(base, method, msg, signal) {
  const res = await fetch(base + SVC + method, {
    method: 'POST',
    signal,
    headers: { 'content-type': 'application/grpc-web+proto', 'x-grpc-web': '1' },
    body: frame(Uint8Array.from(msg)),
  });
  if (!res.ok) throw new Error(`${method}: HTTP ${res.status}`);
  const reader = res.body.getReader();
  let buf = new Uint8Array(0), status = res.headers.get('grpc-status');
  for (;;) {
    while (buf.length >= 5) {
      const n = new DataView(buf.buffer, buf.byteOffset).getUint32(1);
      if (buf.length < 5 + n) break;
      const flag = buf[0], payload = buf.slice(5, 5 + n);
      buf = buf.slice(5 + n);
      if (flag & 0x80) { const m = /grpc-status:\s*(\d+)/i.exec(new TextDecoder().decode(payload)); if (m) status = m[1]; }
      else yield payload;
    }
    const { value, done } = await reader.read();
    if (done) break;
    const nb = new Uint8Array(buf.length + value.length); nb.set(buf); nb.set(value, buf.length); buf = nb;
  }
  if (status !== null && status !== '0') throw new Error(`${method}: grpc-status ${status}`);
}

export async function latestHeight(base, signal) {
  for await (const m of call(base, 'GetLatestBlock', [], signal)) for (const [f, v] of fields(m)) if (f === 1) return v;
  throw new Error('GetLatestBlock: no height');
}

// Yields serialized CompactBlock messages for start..=end, in order.
export async function* blockRange(base, start, end) {
  for (let s = start; s <= end; s += MAX_RANGE) {
    const e = Math.min(end, s + MAX_RANGE - 1);
    const msg = [...ld(1, [0x08, ...varint(s)]), ...ld(2, [0x08, ...varint(e)])];
    yield* call(base, 'GetBlockRange', msg);
  }
}

// { data: Uint8Array (raw tx), height } for a txid in display (big-endian) hex.
export async function getTransaction(base, txidHex) {
  if (!/^[0-9a-f]{64}$/.test(txidHex)) throw new Error('bad txid');
  const hash = txidHex.match(/../g).map((x) => parseInt(x, 16)).reverse();
  for await (const m of call(base, 'GetTransaction', ld(3, hash))) {
    let data = new Uint8Array(0), height = 0;
    for (const [f, v] of fields(m)) { if (f === 1) data = v.slice(); if (f === 2) height = v; }
    return { data, height };
  }
  throw new Error('GetTransaction: empty response');
}
