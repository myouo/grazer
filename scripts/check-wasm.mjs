import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
const hashes = readFileSync('target/native-trace.txt', 'utf8').trim().split('\n');
assert.equal(hashes.length, 100000, 'requires the full conformance trace');
const { instance } = await WebAssembly.instantiate(readFileSync('target/wasm32-unknown-unknown/release/grazer_wasm_check.wasm'), {});
const pointer = instance.exports.run_trace(hashes.length);
const data = new DataView(instance.exports.memory.buffer, pointer, hashes.length * 8);
for (let tick = 0; tick < hashes.length; tick++) {
    assert.equal(data.getBigUint64(tick * 8, true).toString(16).padStart(16, '0'), hashes[tick], `diverged at tick ${tick + 1}`);
}
console.log(`PASS: ${hashes.length} native/WASM per-tick hashes; final ${hashes.at(-1)}`);
