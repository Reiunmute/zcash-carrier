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
// disclosure channel: messages sealed by native code open in wasm
const b = (h) => Buffer.from(h, 'hex');
const fill = (v) => new Uint8Array(32).fill(v);
const t = w.channel_transcript(fill(1), b(r.coll), 1, 10000n, 4500000n, fill(3), b(r.ch_pk), fill(4));
checks.channel_transcript = hex(t) === r.ch_t;
checks.channel_open = Buffer.from(w.channel_open(b(r.ch_sk), 3, 0, 0, t, b(r.ch_msg))).toString() === 'parity plaintext';
const throws = (f) => { try { f(); return false; } catch (_) { return true; } };
checks.channel_open_wrong_seq = throws(() => w.channel_open(b(r.ch_sk), 3, 0, 1, t, b(r.ch_msg)));
const d = JSON.parse(w.disclosure_open(b(r.ch_sk), 0, t, b(r.ch_disc)));
checks.disclosure_open = d.vault_fvk === r.fvk && d.carrier_action === 2 && d.pay_addr === r.seal;
checks.disclosure_wrong_transcript = throws(() => w.disclosure_open(b(r.ch_sk), 0, fill(0), b(r.ch_disc)));
checks.pow_ok = w.pow_ok(t, 10, BigInt(r.pow_stamp)) === true;
const kp = w.channel_keypair();
const m2 = w.channel_seal(kp.slice(32), 2, 1, 5, t, Buffer.from('wasm to wasm'));
checks.channel_wasm_round_trip = Buffer.from(w.channel_open(kp.slice(0, 32), 2, 1, 5, t, m2)).toString() === 'wasm to wasm';
for (const [k, v] of Object.entries(checks)) console.log(`${v ? 'ok  ' : 'FAIL'} ${k}`);
process.exit(Object.values(checks).every(Boolean) ? 0 : 1);
