# Grazer

Deterministic Rust 2D bullet-hell (STG) runtime.

**M2 / experimental, repository sources:** a playable Rust stage SDK with
desktop/WebGPU/WebGL2 runners, textured sprites and HUD, versioned atlas/tone
resources, audio events, shooting/focus/Bomb, enemy waves, Boss and death/restart.
The shared headless core retains fixed 60Hz, checked Q16.16, generational pools,
swept collision, grazing and input/command replay. The dedicated language/VM,
advanced STG tools, stable SDK/ABI and editor are later milestones.
Public APIs may change before 0.1.0. The published `0.1.0-alpha.1` crate contains
M0; these M1/M2 additions have not been published.

```rust
use grazer::{Game, GameInput};
let mut game = Game::new(42).unwrap();
game.step(GameInput { fire: true, ..GameInput::default() }).unwrap();
let hud = game.hud(); // caller owns time and advances exactly one tick
for sprite in game.sprites() { /* render with game.resources() */ }
for sound in game.audio_events() { /* play resource ID once after each step */ }
```

The default library has no third-party dependencies. Enable `ffi` for the native
C ABI, `desktop` for the windowed example, or `web` for browser bindings.
The `graphics` feature provides the shared wgpu presentation backend.
`Game` is protocol 3 over the protocol-2 `Simulation`. Legacy `Runtime`, C ABI
v1 and the `desktop`/`motion.html` demonstrations retain M0. The additive
`grazer_game_*` ABI v2 consumes the same Game snapshots, HUD, audio and resources.
See [M2 SDK and controls](docs/m2-playable.md) and [M1 contracts](docs/m1-headless.md).

## Run from the repository

```sh
cargo test --workspace
cargo run --release --example play --features desktop
# Optional: --project assets/demo/project.json; --autoplay --frames 600
cargo run --release --example benchmark -- 100000 1200 m1 circle
cargo run --release --example benchmark -- 100000 1200 m1 capsule
cargo run --release --example benchmark -- 100000 1200 m1 curve
cargo run --release --example desktop --features desktop -- 100000
cargo run --release --example benchmark -- 100000 1200
cargo build --release --features ffi
cc -std=c11 -Wall -Wextra -Werror -Iinclude examples/c_host.c -Ltarget/release -lgrazer -Wl,-rpath,"$PWD/target/release" -o target/c_host
target/c_host
cc -std=c11 -Wall -Wextra -Werror -Iinclude examples/c_game.c -Ltarget/release -lgrazer -Wl,-rpath,"$PWD/target/release" -o target/c_game
target/c_game
```

In the playable demo: arrow keys/WASD move, Z/Space shoots, X uses a Bomb, Shift
halves speed, P pauses, R/Enter restarts. Twenty waves occupy two minutes; the
Boss follows. Sustained centered shooting clears the stage at about 2:50 with
the high-health validation fixture. Normal play starts with three health and
three Bombs. Focus loss/minimization pause desktop ticks; browser visibility
and focus loss pause ticks. Pause/resume discards paused wall time.

Desktop audio uses CPAL. Linux builds need ALSA development files
(`libasound2-dev` on Debian/Ubuntu, `alsa-lib-devel` on Fedora, `alsa-lib` on Arch)
and `pkg-config`. Audio unavailability is reported; simulation remains playable.
The legacy `desktop` example still wraps 100,000 bullets without gameplay.

Browser build requires the `wasm32-unknown-unknown` target and `wasm-bindgen-cli`
matching Cargo.lock's wasm-bindgen version:

```sh
rustup target add wasm32-unknown-unknown
sh scripts/build-web.sh
python3 -m http.server 8080 --directory web
```

Open `http://localhost:8080/?backend=webgpu` or
`http://localhost:8080/?backend=webgl`. Explicit backend selection
fails visibly when unavailable; `backend=auto` selects WebGPU with WebGL2
fallback. Click **Enable sound** to unlock event-driven browser audio.
The M0 performance demo is at `/motion.html?backend=webgl&count=30000`.
Generated `web/pkg` and `web/assets` are excluded from Git. The checked-in
`assets/demo` contains the JSON manifest and raw RGBA atlas; `make_assets`
exports the authored Rust pixel patterns for a web build or resource editor.

Native/WASM conformance (100,000 per-tick hashes, runs the actual WASM in Node):

```sh
cargo build --release -p grazer-wasm-check --target wasm32-unknown-unknown
cargo run --release --example trace -- 100000 > target/native-trace.txt
cargo run --release --example trace -- 100000 m1 > target/native-simulation-trace.txt
cargo run --release --example trace -- 100000 m2 > target/native-game-trace.txt
node scripts/check-wasm.mjs
node scripts/benchmark-wasm.mjs 30000 1200 circle
```

## Contracts

- All three protocols use integer authoritative state, SplitMix64 and explicit update
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
- `Game::clone`, `Simulation::clone` and `Runtime::clone` are in-process checkpoints. M1 replay
  format 1 records configuration, seed, inputs, accepted commands and per-tick
  hashes; incompatible versions are rejected. Serialized checkpoints, resource
  metadata and a long-term compatibility promise are later milestones. Playable
  Game input traces must also match stage content ID and resource content hash;
  the full saved gameplay replay/debug workflow is M5.

See [M0 platform validation](docs/m0-validation.md) and
[M1 core validation](docs/m1-validation.md) for actual coverage and performance
evidence, and [M2 playable validation](docs/m2-validation.md) for the shared SDK.
Next milestones add the dedicated language/VM, advanced
STG features, replay/debug tools, stable SDK/ABI and editor integration, in that order.

Licensed under MIT OR Apache-2.0.
