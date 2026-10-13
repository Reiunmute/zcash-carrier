"""Serves web/prototype for local testing, with caching off so edits show at once.
Usage: python3 scripts/serve_web.py [port] [host]   (default 8790 on 127.0.0.1; open / in a browser)
Build the wasm bundle first: bash scripts/build_web.sh

It also relays the three lightwalletd grpc-web calls the verifier makes (POST /lwd/...) to
the public testnet server, so a browser that cannot reach that server directly (an extension,
a DNS filter, a VPN) still works. The relay is not trusted: the wasm verifier checks every byte."""
import http.server
import os
import sys
import urllib.request

UPSTREAM = "https://zcash-testnet.chainsafe.dev"
METHODS = {"GetLatestBlock", "GetBlockRange", "GetTransaction"}
PREFIX = "/lwd/cash.z.wallet.sdk.rpc.CompactTxStreamer/"


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        ".js": "text/javascript",
        ".mjs": "text/javascript",
        ".wasm": "application/wasm",
    }

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def do_GET(self):
        if self.path == "/lwd/ping":
            self.send_response(204)
            self.end_headers()
            return
        if self.path.startswith("/__client_error"):
            self.send_response(204)
            self.end_headers()
            return
        super().do_GET()

    def do_POST(self):
        if not self.path.startswith(PREFIX) or self.path[len(PREFIX):] not in METHODS:
            self.send_error(404)
            return
        body = self.rfile.read(min(int(self.headers.get("Content-Length", 0)), 4096))
        req = urllib.request.Request(UPSTREAM + self.path[len("/lwd"):], data=body, method="POST", headers={
            "content-type": "application/grpc-web+proto", "x-grpc-web": "1"})
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                data = r.read()
                status = r.headers.get("grpc-status")
        except Exception as e:  # noqa: BLE001
            self.send_error(502, f"relay: {e}")
            return
        self.send_response(200)
        self.send_header("Content-Type", "application/grpc-web+proto")
        if status is not None:
            self.send_header("grpc-status", status)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


if __name__ == "__main__":
    root = os.environ.get("ZARRIER_ROOT") or os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "web", "prototype")
    os.chdir(root)
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8790
    host = sys.argv[2] if len(sys.argv) > 2 else "127.0.0.1"
    print(f"serving {root} at http://{host}:{port}/", flush=True)
    http.server.ThreadingHTTPServer((host, port), Handler).serve_forever()
