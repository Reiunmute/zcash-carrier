// Run by verify.sh when the `sale` feature was built: node sale_selftest.cjs <wasm-bindgen nodejs out dir>
// Runs the PCZT sale self-test inside wasm and prints its log and the time it took.
const w = require(require('path').resolve(process.argv[2], 'carrier_wasm.js'));
const t = Date.now();
const out = w.sale_selftest();
console.log(out);
console.log(`wasm sale selftest: ${((Date.now() - t) / 1000).toFixed(1)}s`);
process.exit(/^txid [0-9a-f]{64}$/m.test(out) ? 0 : 1);
