# Grazer

Deterministic Rust 2D bullet-hell (STG) runtime.

**M1 / experimental, repository sources:** fixed 60Hz headless simulation,
checked Q16.16 arithmetic, seeded RNG, generational entity pools, player/enemy
damage, swept circle/capsule/polyline collision, once-per-projectile grazing,
stable lifecycle events and binary input/command replay. The dedicated language,
playable runner, audio events, resource pipeline and editor are later milestones.
Public APIs may change before 0.1.0. The published `0.1.0-alpha.1` crate contains
M0; these M1 additions have not been published.

```rust
use grazer::{Input, Simulation, SimulationConfig};
let mut game = Simulation::new(SimulationConfig::default(), 42).unwrap();
game.step_with_input(Input { x: 1, y: 0 }).unwrap(); // host owns time
for entity in game.snapshots() {
    // Present entity using your renderer; coordinates remain fixed point.
}
for event in game.events() { /* consume damage, graze and destruction events */ }
```

The default library has no third-party dependencies. Enable `ffi` for the native
C ABI, `desktop` for the windowed example, or `web` for browser bindings.
The `graphics` feature provides the shared wgpu presentation backend.
`Simulation` is the protocol-2 M1 core. `Runtime`, the C ABI and desktop/browser
examples retain the protocol-1 M0 motion workload. The playable M1 presentation
and host integration are M2 work. See [M1 API and contracts](docs/m1-headless.md).

## Run from the repository

```sh
cargo test --workspace
cargo run --release --example benchmark -- 100000 1200 m1 circle
cargo run --release --example benchmark -- 100000 1200 m1 capsule
cargo run --release --example benchmark -- 100000 1200 m1 curve
cargo run --release --example desktop --features desktop -- 100000
cargo run --release --example benchmark -- 100000 1200
cargo build --release --features ffi
cc -std=c11 -Wall -Wextra -Werror -Iinclude examples/c_host.c -Ltarget/release -lgrazer -Wl,-rpath,"$PWD/target/release" -o target/c_host
target/c_host
```

Arrow keys move the white player marker. The M0 demo wraps bullets at the
playfield edges. It has no collisions or scoring. Optional second desktop
argument sets a frame limit for smoke tests. Window minimization and focus loss
pause the desktop clock. Browser visibility loss pauses the demo.

Browser build requires the `wasm32-unknown-unknown` target and `wasm-bindgen-cli`
matching Cargo.lock's wasm-bindgen version:

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown --features web
wasm-bindgen --target web --out-dir web/pkg target/wasm32-unknown-unknown/release/grazer.wasm
python3 -m http.server 8080 --directory web
```

Open `http://localhost:8080/?backend=webgpu&count=30000` or
`http://localhost:8080/?backend=webgl&count=30000`. Explicit backend selection
fails visibly when unavailable; `backend=auto` selects WebGPU with WebGL2
fallback. Click **Enable audio** to test browser audio unlock with a short tone.

Native/WASM conformance (100,000 per-tick hashes, runs the actual WASM in Node):

```sh
cargo build --release -p grazer-wasm-check --target wasm32-unknown-unknown
cargo run --release --example trace -- 100000 > target/native-trace.txt
cargo run --release --example trace -- 100000 m1 > target/native-simulation-trace.txt
node scripts/check-wasm.mjs
node scripts/benchmark-wasm.mjs 30000 1200 circle
```

## Contracts

- Both protocols use integer authoritative state, SplitMix64 and explicit update
  order. Hashes include configuration, RNG, pending input and ordered entities;
  protocol 2 also hashes generations, free-list order and gameplay state.
  Hashes diagnose divergence and are not cryptographic.
- `Fixed` operations are checked; division truncates toward zero. Velocities are
  units per tick. Draw floats are presentation-only.
- The caller controls every tick and records inputs/commands. Same protocol,
  configuration, seed and commands give identical native/WASM simulation.
- C structs and ownership are specified in `include/grazer.h`. Calls are serial
  per runtime; callers provide valid pointers and own output buffers. No Rust
  container layouts or GPU handles cross the ABI.
- `Simulation::clone` and `Runtime::clone` are in-process checkpoints. M1 replay
  format 1 records configuration, seed, inputs, accepted commands and per-tick
  hashes; incompatible versions are rejected. Serialized checkpoints, resource
  metadata and a long-term compatibility promise are later milestones.

See [M0 platform validation](docs/m0-validation.md) and
[M1 core validation](docs/m1-validation.md) for actual coverage and performance
evidence. Next milestones add a playable stage, the dedicated language, advanced
STG features, replay/debug tools, stable SDK/ABI and editor integration, in that order.

Licensed under MIT OR Apache-2.0.
