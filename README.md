# Grazer

Deterministic Rust 2D bullet-hell (STG) runtime.

**M6 release preparation, repository sources:** portable `.grazer` project
archives, an SDK/ABI compatibility baseline, external Rust/C examples,
graphics-device recovery and complete CPU/GPU frame benchmarks. Formal 0.1.0
performance acceptance is still pending; no new release has been published.
The M5 tools provide complete Game/world/VM checkpoints,
resource-bound replay, verified seek/fast-forward, Boss practice, pause/step,
collision outlines, CPU timings, state inspection and atomic source/resource
reload. The M4 creation tools provide deterministic ring/fan/aimed/spiral
patterns, composed acceleration/turning, telegraphed straight/curve lasers,
multi-phase Bosses, bullet cancellation, point/power/Bomb drops, scoring and
three difficulties. Desktop/WebGPU/WebGL2 default to the ten-minute scripted
Prism Passage: 70 waves and three Boss phases. Nine focused scripts under
`assets/examples` demonstrate each ability. The typed language retains verified
bytecode, cooperative tasks, budgets, source diagnostics and VM save/restore.
The shared headless core uses fixed 60Hz, checked Q16.16, generational pools,
swept collision, grazing and input/command replay. Stable SDK/ABI, final platform/
performance acceptance and editor remain later milestones.
Public APIs may change before 0.1.0. The published `0.1.0-alpha.1` crate contains
M0; these M1–M6 repository additions have not been published.

```rust
use grazer::{GameInput, advanced::Difficulty, game::showcase};
let mut game = showcase::game(Difficulty::Normal, 0).unwrap();
game.step(GameInput { fire: true, ..GameInput::default() }).unwrap();
let hud = game.hud(); // caller owns time and advances exactly one tick
for sprite in game.sprites() { /* render with game.resources() */ }
for beam in game.laser_segments() { /* render capsule/polyline segments */ }
for sound in game.audio_events() { /* play resource ID once after each step */ }
```

The default library has no third-party dependencies. Enable `ffi` for the native
C ABI, `desktop` for the windowed example, or `web` for browser bindings.
The `graphics` feature provides the shared wgpu presentation backend.
M4 `Game`/`Simulation` use protocol 4; opt-out worlds retain protocols 3/2.
`Game::new` and `ScriptStage::builtin` retain the First Sortie references.
Legacy `Runtime`, C ABI
v1 and the `desktop`/`motion.html` demonstrations retain M0. The additive
`grazer_game_*` ABI v2 consumes the same Game snapshots, HUD, audio and resources.
See [M4 creation tools](docs/m4-advanced.md), [M2 SDK and controls](docs/m2-playable.md)
and [M1 contracts](docs/m1-headless.md).

## Run from the repository

```sh
cargo test --workspace
cargo run --release --example play --features desktop
# Distributable game: --bundle target/my-game.grazer
# Package source/resources: cargo run --release --features resources --example project -- pack stage.graze assets/demo/project.json target/my-game.grazer "My game"
# Iteration: --practice 2 --hitboxes --performance; --record target/run.grz;
# --replay target/run.grz; --script my_stage.graze --watch; --paused.
# Optional --difficulty easy|normal|hard; --health 10000 for validation.
# Optional --script my_stage.graze or --script target/my_stage.gzb.
cargo run --release --example script -- compile assets/demo/advanced_showcase.graze target/advanced_showcase.gzb
cargo run --release --example advanced
cargo run --release --example script
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
halves speed, P pauses, R/Enter restarts. Seventy waves occupy seven minutes,
followed by three one-minute Boss phases. Defeating a phase earns a bonus;
surviving its timer also advances the stage. Thin warning/fading lasers are
harmless; bright active beams persist when they hit. Point drops score, power
drops strengthen shots (four levels), and Bomb drops replenish stock. Nearby
drops attract to the ship; moving into the top quarter attracts all drops.
Normal play starts with three health and
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
Choose Easy/Normal/Hard in the page or use `?difficulty=hard`. Load a focused
example with `?script=./examples/curve_laser.graze`; the historical M3 stage
remains at `?script=./first_sortie.graze`.
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
cargo run --release --example trace -- 100000 m3 > target/native-script-trace.txt
cargo run --release --example trace -- 100000 m4 > target/native-advanced-trace.txt
cargo run --release --example replay -- record target/showcase.grz 100000
cargo run --release --example script -- restore-check > target/native-vm-restore.txt
node scripts/check-wasm.mjs
node scripts/benchmark-wasm.mjs 30000 1200 circle
```

## Contracts

- All simulation/stage protocols use integer authoritative state, SplitMix64 and explicit update
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
  hashes; incompatible versions are rejected. M5 complete checkpoints and Game
  replay bind program/resources/config/seed, preserve pools/VM/RNG/score/control
  edges and report the first component divergence. Full-history verification and
  checkpoint seek are available. A long-term compatibility promise starts at M6.

See [M0 platform validation](docs/m0-validation.md) and
[M1 core validation](docs/m1-validation.md) for actual coverage and performance
evidence, and [M2 playable validation](docs/m2-validation.md) for the shared SDK.
See [M3 language and VM](docs/m3-language.md) and [M3 validation](docs/m3-validation.md)
for script authoring, and [M4 validation](docs/m4-validation.md) for actual
ten-minute stage evidence. See [M5 iteration tools](docs/m5-iteration.md) and
[M5 validation](docs/m5-validation.md) for recording/checkpoints, practice, debug
controls and actual host evidence. Next milestones are M6 stable SDK/ABI and
formal platform/performance acceptance, then M7 editor integration. See
[authoring/distribution](docs/authoring.md), [SDK/ABI baseline](docs/sdk-abi.md)
and [M6 release readiness](docs/m6-readiness.md).

Licensed under MIT OR Apache-2.0.
