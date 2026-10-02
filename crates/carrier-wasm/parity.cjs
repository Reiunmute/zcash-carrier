// Run by verify.sh: node parity.cjs <wasm-bindgen nodejs out dir> <coreref.json>
// Calls carrier-core through the wasm build and compares with native reference values.
const w = require(require('path').resolve(process.argv[2], 'carrier_wasm.js'));
const r = JSON.parse(require('fs').readFileSync(process.argv[3], 'utf8'));
const hex = (u) => Buffer.from(u).toString('hex');
const enc = (s) => new TextEncoder().encode(s);
const checks = {
  seal_address: w.seal_address_hex(r.coll) === r.seal,
  collectibles_seed: hex(w.collectibles_seed(enc('wallet-signature'), enc('Carrier collectibles key v1'))) === r.seed,
  vault_verify_ok: w.vault_verify(r.fvk, enc('parity transcript'), Buffer.from(r.sig, 'hex')) === true,
  vault_verify_wrong_msg: w.vault_verify(r.fvk, enc('other transcript'), Buffer.from(r.sig, 'hex')) === false,
};
const m = JSON.parse(w.decode_memo(Buffer.from(r.memo, 'hex')));
checks.decode_memo = m.kind === 3 && m.coll === r.coll && m.num === 1 && m.hop === 2;
checks.decode_non_memo = w.decode_memo(new Uint8Array(512)) === 'null';
for (const [k, v] of Object.entries(checks)) console.log(`${v ? 'ok  ' : 'FAIL'} ${k}`);
process.exit(Object.values(checks).every(Boolean) ? 0 : 1);
