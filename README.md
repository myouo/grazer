# Grazer

Deterministic foundations for a Rust 2D bullet-hell (STG) runtime.

**M0 / experimental:** fixed 60Hz simulation, checked Q16.16 arithmetic,
seeded motion, tick-boundary input, state hashing, bulk draw snapshots, a C ABI,
and optional desktop/WebGPU/WebGL2 demonstrations. Collisions, the dedicated
stage/bullet language, audio events, replays, resource pipelines and editor are
planned milestones, not implemented features. Public APIs may change before 0.1.0.

```rust
use grazer::{Config, Input, Runtime};
let mut game = Runtime::new(Config::default(), 42).unwrap();
game.set_input(Input { x: 1, y: 0 }).unwrap();
game.step().unwrap(); // exactly one tick; the host owns time
for sprite in game.sprites() {
    // Present sprite using your renderer.
}
```

The default library has no third-party dependencies. Enable `ffi` for the native
C ABI, `desktop` for the windowed example, or `web` for browser bindings.
The `graphics` feature provides the shared wgpu presentation backend.

## Run from the repository

```sh
cargo test --workspace
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
node scripts/check-wasm.mjs
```

## Contracts

- Protocol 1 uses integer authoritative state, SplitMix64 and explicit update
  order. Hashes include configuration, RNG, pending input and ordered entities.
  Hashes diagnose divergence and are not cryptographic.
- `Fixed` operations are checked; division truncates toward zero. Velocities are
  units per tick. Draw floats are presentation-only.
- The caller controls every tick and records inputs/commands. Same protocol,
  configuration, seed and commands give identical native/WASM simulation.
- C structs and ownership are specified in `include/grazer.h`. Calls are serial
  per runtime; callers provide valid pointers and own output buffers. No Rust
  container layouts or GPU handles cross the ABI.
- In-process `Runtime::clone` is useful for testing checkpoints; there is no
  stable serialized replay or save format yet.

See `docs/m0-validation.md` in the repository for actual platform coverage and
performance evidence. Future milestones add full simulation, a playable stage,
the dedicated language, advanced STG features, replay/debug tools, stable SDK/ABI
and editor integration, in that order.

Licensed under MIT OR Apache-2.0.
