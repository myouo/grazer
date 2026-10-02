import { readFileSync } from 'node:fs';
import { performance } from 'node:perf_hooks';
import assert from 'node:assert/strict';
const count = Number(process.argv[2] ?? 30000), ticks = Number(process.argv[3] ?? 1200);
const shape = process.argv[4] ?? 'circle', kind = ['circle', 'capsule', 'curve'].indexOf(shape);
assert(Number.isInteger(count) && count >= 0 && count <= 1000000, 'count must be 0..1,000,000');
assert(Number.isInteger(ticks) && ticks > 0, 'positive sample count required');
assert(kind >= 0, 'shape must be circle, capsule or curve');
const { instance } = await WebAssembly.instantiate(readFileSync('target/wasm32-unknown-unknown/release/grazer_wasm_check.wasm'), {});
const api = instance.exports;
assert.equal(api.benchmark_begin(count, kind), 1);
for (let i = 0; i < 120; i++) {
    assert.equal(api.benchmark_step(), 1); assert.equal(api.benchmark_replenish(), 1);
}
const times = [], replenishment = [];
let minimumCount = count;
for (let i = 0; i < ticks; i++) {
    assert.equal(api.benchmark_count(), count, 'full workload before each step');
    const start = performance.now(), result = api.benchmark_step();
    times.push(performance.now() - start); assert.equal(result, 1);
    minimumCount = Math.min(minimumCount, api.benchmark_count());
    const replenishStart = performance.now(), replenished = api.benchmark_replenish();
    replenishment.push(performance.now() - replenishStart); assert.equal(replenished, 1);
}
times.sort((a,b) => a-b); replenishment.sort((a,b) => a-b);
const p95 = Math.ceil(ticks * 0.95) - 1;
const metrics = {
    workload: 'm1-motion-collision-graze', execution: 'WASM in Node', node: process.version,
    shape, count, ticks, warmup: 120, simulation_median_ms: times[Math.floor(ticks / 2)],
    simulation_p95_ms: times[p95], simulation_max_ms: times.at(-1), replenish_p95_ms: replenishment[p95],
    minimum_count_after_step: minimumCount, grazes_including_warmup: Number(api.benchmark_grazes()),
    hash: BigInt.asUintN(64, api.benchmark_hash()).toString(16).padStart(16, '0'),
};
if (process.argv[5]) {
    const native = JSON.parse(readFileSync(process.argv[5], 'utf8'));
    for (const key of ['shape', 'count', 'ticks', 'warmup', 'hash', 'grazes_including_warmup', 'minimum_count_after_step']) {
        assert.equal(metrics[key], native[key], `native/WASM benchmark differs: ${key}`);
    }
    metrics.native_hash_match = true;
}
console.log(JSON.stringify(metrics));
