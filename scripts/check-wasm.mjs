import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
const { instance } = await WebAssembly.instantiate(readFileSync('target/wasm32-unknown-unknown/release/grazer_wasm_check.wasm'), {});
for (const [mode, path, run] of [
    ['M0', 'target/native-trace.txt', instance.exports.run_trace],
    ['M1', 'target/native-simulation-trace.txt', instance.exports.run_simulation_trace],
    ['M2', 'target/native-game-trace.txt', instance.exports.run_game_trace],
    ['M3', 'target/native-script-trace.txt', instance.exports.run_script_trace],
    ['M4', 'target/native-advanced-trace.txt', instance.exports.run_advanced_trace],
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
const restored=BigInt.asUintN(64,instance.exports.run_vm_restore_check(10000)).toString(16).padStart(16,'0');
assert.equal(restored,readFileSync('target/native-vm-restore.txt','utf8').trim(),'serialized VM/RNG native/WASM continuation');
console.log(`PASS: M3 VM serialized/RNG restore continuation, 10000 ticks; ${restored}`);
