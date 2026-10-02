import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
const { instance } = await WebAssembly.instantiate(readFileSync('target/wasm32-unknown-unknown/release/grazer_wasm_check.wasm'), {});
for (const [mode, path, run] of [
    ['M0', 'target/native-trace.txt', instance.exports.run_trace],
    ['M1', 'target/native-simulation-trace.txt', instance.exports.run_simulation_trace],
]) {
    const hashes = readFileSync(path, 'utf8').trim().split('\n');
    assert.equal(hashes.length, 100000, `${mode} requires the full conformance trace`);
    const pointer = run(hashes.length);
    const data = new DataView(instance.exports.memory.buffer, pointer, hashes.length * 8);
    for (let tick = 0; tick < hashes.length; tick++) {
        assert.equal(data.getBigUint64(tick * 8, true).toString(16).padStart(16, '0'), hashes[tick], `${mode} diverged at tick ${tick + 1}`);
    }
    if (mode === 'M1') {
        assert.equal(BigInt.asUintN(64, instance.exports.run_replay_check(10000)).toString(16).padStart(16, '0'), hashes[9999], 'M1 encoded replay diverged');
    }
    console.log(`PASS: ${mode} ${hashes.length} native/WASM per-tick hashes; final ${hashes.at(-1)}`);
}
console.log('PASS: M1 10,000-tick encoded replay executed in WASM');
