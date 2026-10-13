#!/usr/bin/env bash
# Assembles the static marketplace plus the /lwd relay into OUT (default .vercel-site) for
# `vercel deploy`. The wasm bundle (web/prototype/pkg) is gitignored, so build it first:
#   bash scripts/build_web.sh && bash scripts/deploy_vercel.sh && (cd .vercel-site && vercel deploy --prod)
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=${OUT:-.vercel-site}
test -f web/prototype/pkg/carrier_wasm_bg.wasm || { echo "missing wasm bundle: run scripts/build_web.sh" >&2; exit 1; }
rm -rf "$OUT/api" "$OUT/public"; mkdir -p "$OUT/public" "$OUT/api"
P=web/prototype
cp $P/index.html $P/demo.html $P/app.js $P/styles.css $P/verify.mjs $P/mark.svg "$OUT/public/"
cp -R $P/fonts $P/lib "$OUT/public/"
mkdir -p "$OUT/public/pkg" && cp $P/pkg/carrier_wasm.js $P/pkg/carrier_wasm_bg.wasm "$OUT/public/pkg/"
cp deploy/vercel/api/lwd.js "$OUT/api/"
cp deploy/vercel/vercel.json "$OUT/"
printf '{ "name": "zarrier-site", "private": true }\n' > "$OUT/package.json"
echo "assembled $OUT: $(du -sh "$OUT" | cut -f1)"
