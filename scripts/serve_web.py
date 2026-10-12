"""Serves web/prototype on 127.0.0.1 for local testing, with caching off so edits show at once.
Usage: python3 scripts/serve_web.py [port]   (default 8790; open / in a browser)
Build the wasm bundle first: bash scripts/build_web.sh"""
import http.server
import os
import sys


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


os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "web", "prototype"))
port = int(sys.argv[1]) if len(sys.argv) > 1 else 8790
print(f"serving web/prototype at http://127.0.0.1:{port}/  ", flush=True)
http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
