// Relay for the three lightwalletd grpc-web calls the verifier makes, for browsers that cannot
// reach the public server directly (an extension, a DNS filter, a VPN). Not trusted: the wasm
// verifier checks every byte. Streams the upstream body through unchanged.
export const config = { runtime: "edge" };
const UPSTREAM = "https://zcash-testnet.chainsafe.dev";
const PREFIX = "cash.z.wallet.sdk.rpc.CompactTxStreamer/";
const METHODS = new Set(["GetLatestBlock", "GetBlockRange", "GetTransaction"]);

export default async function handler(req) {
  const p = new URL(req.url).searchParams.get("p") || "";
  if (p === "ping" || p === "client_error") return new Response(null, { status: 204, headers: { "cache-control": "no-store" } });
  if (req.method !== "POST" || !p.startsWith(PREFIX) || !METHODS.has(p.slice(PREFIX.length))) return new Response("not found", { status: 404 });
  const body = await req.arrayBuffer();
  if (body.byteLength > 4096) return new Response("too large", { status: 413 });
  const up = await fetch(`${UPSTREAM}/${p}`, { method: "POST", body, headers: { "content-type": "application/grpc-web+proto", "x-grpc-web": "1" } });
  const h = new Headers({ "content-type": "application/grpc-web+proto", "cache-control": "no-store" });
  const st = up.headers.get("grpc-status");
  if (st !== null) h.set("grpc-status", st);
  return new Response(up.body, { status: up.status, headers: h });
}
