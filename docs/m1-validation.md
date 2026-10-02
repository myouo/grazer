# M1 validation

Date: 2026-10-02. Status: M1 implementation and local acceptance complete.
M1 adds the complete headless core in repository sources;
the published `grazer 0.1.0-alpha.1` still contains M0. No new crate release
has been made. See [API and protocol contracts](m1-headless.md).

## Correctness and conformance

- Tests cover circle/capsule/polyline boundaries and tangency, one-raw-bit
  separation, degenerate segments, curve joins, full Q16.16 geometry range,
  relative swept actor/projectile motion, high-speed hit and graze, once-only
  grazing, exact invulnerability duration, health saturation and death.
- Entity tests cover stable spawn/contact/survivor order, simultaneous shots,
  generation reuse/retirement, stale/wrong-kind handles, capacity recovery,
  final lifetime collision passes, whole-shape culling and player clamping.
  Rejected commands and failed motion/ticks preserve state.
- Checkpoint continuation compares hashes, snapshots and events. Binary replay
  round trips compare every tick; truncation, unsupported versions, invalid
  counts, rejected recording commands and hash divergence are checked.
- Allocation-count tests fill a cloned world's pools and its maximum contact /
  event workload, including player death. Spawning, stepping, hashing and
  snapshot iteration allocate zero times. A 500-tick replay player also
  allocates zero times after construction.
- Native and **actual WASM executed in Node** match all 100,000 protocol-2
  hashes. The fixture cycles all three shapes through hit/graze/miss offsets,
  enemy/friendly/body contacts, expiry/culling, explicit destruction, handle
  recycling and host RNG commands. Final hash: `436de57247bc84be`.
- WASM records, encodes, decodes and plays a 10,000-tick input/command replay;
  its final hash matches the corresponding native trace tick. Protocol-1 M0
  still matches all 100,000 native/WASM hashes, ending `b07e2bf531c8d412`.
- Final local checks passed: 33 unit/integration tests plus 2 documentation
  examples, all-feature/all-target Clippy with warnings denied, formatting,
  and workspace Rust documentation with warnings denied.
- Native release desktop/FFI examples and the browser-feature WASM release
  build passed. The actual C host still reports ABI version/layout/lifecycle
  success and hash `c93b8966b5256127`.
- `cargo package --allow-dirty --locked --offline` verified the default packaged
  build. The extracted package also passed all-feature/all-target compilation.
  Its 29 files include all five M1 source modules and both new test files;
  local design documents are excluded. Package size: 225.0 KiB uncompressed /
  57.2 KiB compressed. This package was verified locally and not published.

The CI workflow includes M1 tests and both conformance traces in the existing
Linux/Windows/macOS native and WASM matrix. Local M1 execution covers Linux and
WASM/Node. New Windows/macOS CI results and GUI execution are not claimed here.
M1 does not add a gameplay renderer; M0 presentation coverage is recorded in
[the M0 report](m0-validation.md).

## Performance

Reference host: AMD Ryzen 7 5800H, 8 cores / 16 logical CPUs, Linux x86_64
`7.2.6-zen2-1-zen`, Rust 1.97.1, release + thin LTO, Node v24.15.0. Headless
CPU/WASM measurements use no GPU, window or browser. Runs were sequential.
Each workload has 120 warmup steps and 1,200 measured steps; p95 is nearest
rank (`ceil(0.95 × samples) - 1`). Values are milliseconds.

| Shape / count | Native median | Native p95 | WASM/Node median | WASM/Node p95 |
| --- | ---: | ---: | ---: | ---: |
| Circle / 100,000 | 5.0054 | 6.7244 | — | — |
| Circle / 30,000 | 0.7712 | 1.1720 | 0.9983 | 1.4676 |
| Capsule / 100,000 | 4.8697 | 6.1424 | — | — |
| Capsule / 30,000 | 0.7329 | 1.1071 | 1.0449 | 1.5878 |
| Three-point curve / 100,000 | 4.8824 | 6.0132 | — | — |
| Three-point curve / 30,000 | 0.8142 | 1.1921 | 1.0765 | 1.6556 |

The measured step includes motion preflight, motion, player collision/grazing,
contact resolution, lifecycle events and stable compaction. All projectiles
are hostile, with two-unit thickness, small seeded linear velocities and no
expiry/culling. The player follows versioned moving input. Consumed projectiles
are replenished outside the step timer, so every measured step **starts with
the full requested count**. Replenishment p95 was ≤0.0034 ms. State hashing,
snapshots and drawing are outside the step timer.

Native 100,000-circle samples generated 4,399 hit events and 12,366 total
grazes including warmup; the minimum count after a step was 99,984. The
30,000-circle workload generated 1,325 sampled hits and 3,735 total grazes;
its minimum count was 29,992 before replenishment. Native/WASM workloads have
identical final hashes, graze totals and minimum counts for each 30,000 shape:

| 30,000 shape | Final native/WASM hash | Total grazes | Minimum after step |
| --- | --- | ---: | ---: |
| Circle | `6a9275ae3b555b22` | 3,735 | 29,992 |
| Capsule | `4517071a10beec76` | 4,398 | 29,987 |
| Three-point curve | `af62db81d0a89dc9` | 4,458 | 29,987 |

These samples satisfy the ≤8 ms simulation p95 budget on this host. Maximum
native 100,000-circle step time was 10.9153 ms, so this is a percentile result,
not a worst-case guarantee. Browser execution, 1080p drawing and the final
≤16.7 ms total-frame budget remain M2/M6 validation. Curves here have three
points; more segments, dense friendly-projectile/enemy pairs and future lasers
need separate capacity measurements.

## Reproduction

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo build --release -p grazer-wasm-check --target wasm32-unknown-unknown --locked
cargo run --release --example trace --locked -- 100000 > target/native-trace.txt
cargo run --release --example trace --locked -- 100000 m1 > target/native-simulation-trace.txt
node scripts/check-wasm.mjs

cargo run --release --example benchmark --locked -- 100000 1200 m1 circle
cargo run --release --example benchmark --locked -- 30000 1200 m1 circle > target/m1-native-30000-circle.json
node scripts/benchmark-wasm.mjs 30000 1200 circle target/m1-native-30000-circle.json
# Repeat both measurements with capsule and curve.

cargo build --release --features ffi,desktop --examples --locked
cargo build --release --features ffi --locked
cc -std=c11 -Wall -Wextra -Werror -Iinclude examples/c_host.c -Ltarget/release -lgrazer -Wl,-rpath,"$PWD/target/release" -o target/c_host
target/c_host
cargo build --release --target wasm32-unknown-unknown --features web --locked -p grazer
cargo package --allow-dirty --locked --offline
cargo check --manifest-path target/package/grazer-0.1.0-alpha.1/Cargo.toml --all-features --all-targets --locked --offline --target-dir target
```

Generated native/WASM traces and benchmark JSON go to ignored `target/`.
Source documentation stays outside the local ignored design documents.
