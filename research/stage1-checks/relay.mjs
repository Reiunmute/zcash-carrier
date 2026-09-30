// Minimal grpc-web -> lightwalletd relay (PoC). Unary calls only.
// grpc-web+proto framing is byte-identical to gRPC framing for the message part;
// only trailers differ (gRPC uses HTTP/2 trailers, grpc-web sends them as a 0x80 frame).
import http from 'node:http';
import http2 from 'node:http2';
const UP = process.env.UPSTREAM || 'https://testnet.zec.rocks:443';
const PORT = Number(process.env.PORT || 8767);
const ALLOW = /^cash\.z\.wallet\.sdk\.rpc\.CompactTxStreamer\/(GetLatestBlock|GetLightdInfo|GetTreeState|GetTransaction|SendTransaction|GetBlock)$/;
let client = null;
const upstream = () => {
  if (!client || client.destroyed || client.closed) {
    client = http2.connect(UP);
    client.on('error', () => { client = null; });
    client.on('goaway', () => { client = null; });
  }
  return client;
};
const cors = { 'access-control-allow-origin': '*', 'access-control-allow-headers': 'content-type,x-grpc-web,x-user-agent', 'access-control-expose-headers': 'grpc-status,grpc-message' };
http.createServer((req, res) => {
  if (req.method === 'OPTIONS') { res.writeHead(204, cors); return res.end(); }
  const path = req.url.replace(/^\//, '');
  if (req.method !== 'POST' || !ALLOW.test(path)) { res.writeHead(403, cors); return res.end(); }
  const chunks = []; req.on('data', c => chunks.push(c));
  req.on('end', () => {
    const s = upstream().request({ ':method': 'POST', ':path': '/' + path, 'content-type': 'application/grpc', te: 'trailers' });
    const out = []; let status = '0', msg = '';
    s.on('data', c => out.push(c));
    s.on('trailers', t => { status = String(t['grpc-status'] ?? '0'); msg = String(t['grpc-message'] ?? ''); });
    s.on('response', h => { if (h['grpc-status'] !== undefined) status = String(h['grpc-status']); });
    s.on('end', () => {
      const tr = Buffer.from(`grpc-status:${status}\r\ngrpc-message:${msg}\r\n`);
      const hdr = Buffer.alloc(5); hdr[0] = 0x80; hdr.writeUInt32BE(tr.length, 1);
      res.writeHead(200, { ...cors, 'content-type': 'application/grpc-web+proto' });
      res.end(Buffer.concat([...out, hdr, tr]));
    });
    s.on('error', e => { res.writeHead(502, cors); res.end(String(e)); });
    s.end(Buffer.concat(chunks));
  });
}).listen(PORT, '127.0.0.1', () => console.log('relay on', PORT, '->', UP));
