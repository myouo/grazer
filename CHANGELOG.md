# Changelog

## Unreleased — M1

- Add the dependency-free protocol-2 `Simulation` with generational projectile
  and enemy pools, player health and invulnerability, stable lifecycle events
  and owned fixed-point snapshots.
- Add exact integer swept circle/capsule/polyline contact, once-per-projectile
  grazing, damage, lifetime and offscreen destruction.
- Add versioned binary input/command recording and replay with per-tick hash
  verification, checkpoint continuation and allocation checks.
- Extend native/actual-WASM conformance and collision/grazing benchmarks.
- Preserve the published protocol-1 `Runtime`, C ABI and presentation fixtures.

## 0.1.0-alpha.1

Initial M0 foundations: deterministic motion, optional wgpu presentation,
Rust/C/browser interfaces and native/WASM conformance tooling.
This is a development preview, not a complete STG game engine.
