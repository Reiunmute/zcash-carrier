import { readFileSync } from 'fs';
import { initSync, bench } from './provebench.js';
initSync({ module: readFileSync(new URL('./provebench_bg.wasm', import.meta.url)) });
console.log('node-wasm a2', bench(2));
console.log('node-wasm a4', bench(4));
