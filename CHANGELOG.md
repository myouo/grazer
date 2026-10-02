# Changelog

## Unreleased — M3

- Add typed `.graze` source: int/bool/Q16.16/vector/entity/task values, variables,
  functions, conditionals/loops, short-circuit expressions and cooperative tasks.
- Add versioned verified register bytecode, deterministic fork/wait/join/cancel/
  entity-owned task lifetimes, checked arithmetic and instruction/command/task/
  call-depth budgets with source-localized faults.
- Add binary VM snapshots containing frames/locals, task generations/order,
  waits/joins/ownership, RNG, limits/status/fault and program compatibility checks.
- Migrate First Sortie/Boss to script; desktop/Web default to that source with
  visible compilation/runtime diagnostics. Retain the M2 native stage reference.
- Add source/bytecode constructors and diagnostics to the existing C game handle,
  plus complete native/C/WASM/browser and serialized-continuation conformance.

## Unreleased — M2

- Add generic `Game<Stage>`, the Rust-native First Sortie stage (20 waves + Boss),
  shooting, focus speed, Bomb damage/clear/protection, score/HUD and death/restart.
- Add versioned JSON/RGBA/tone resources, a reproducible pixel-atlas exporter,
  shared textured sprite/HUD renderer, desktop CPAL audio and browser audio events.
- Add the `play` desktop runner and replace the web landing demo with the playable
  stage; retain the M0 browser workload at `motion.html`.
- Add game ABI v2 alongside ABI v1: shared sprite/HUD/audio/atlas metadata buffers
  and a C host that compares every frame of the native Game trace.
- Add protocol-3 native/WASM/browser conformance, full-stage and host-clock checks.

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
