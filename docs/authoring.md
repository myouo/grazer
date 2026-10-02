# Make and distribute a Grazer game

This guide targets the repository's proposed 0.1.0 SDK. The crates.io release is
still `0.1.0-alpha.1` (M0 only). Until a new release is published, use the checked
repository/source package as a path dependency. A new game can use the default
dependency-free core, the windowed runner, the C SDK or WASM browser host.

## Start with a stage

Create `stage.graze`:

```text
task main() {
    let ship = enemy(vec(width()/2.0, 100.0), vec(0.0, 0.0),
        24.0, 120, 0, 0xe99fffff);
    boss(ship, 120);
    fork pattern(ship);
    while alive(ship) { wait(1); }
    cancel_children();
    cancel_shots(true);
    complete();
}
task pattern(ship: entity) {
    attach(ship);
    let angle = 0.0;
    while alive(ship) {
        ring(position(ship), 12, 1.5, angle, 3.0, 420);
        angle = angle + 0.025;
        wait(90);
    }
}
```

Positions/speeds/radii/angles use Q16.16 values, with velocities per tick. Angles
are clockwise turns on downward-positive Y. Each script must define `task main`.
Tasks cooperate using fixed-tick waits; there is no script clock/network/file
access. New advanced builtins select explicit completion mode, so a Boss death
can be followed by another phase. See [language](m3-language.md),
[patterns/lasers/drops](m4-advanced.md) and the nine `assets/examples` scripts.

Use the supplied JSON/RGBA atlas/tone project first, then replace its sprite
rectangles/pixels/tones while keeping the required numeric IDs. The atlas is
uncompressed RGBA8; paths in the manifest are relative and validated. Resource
loading validates sizes, IDs, rectangles, waveforms and tone limits.

```sh
cargo run --release --example play --features desktop -- --script stage.graze --project assets/demo/project.json --watch
cargo run --release --example script -- compile stage.graze target/stage.gzb
cargo run --release --features resources --example project -- pack stage.graze assets/demo/project.json target/my-game.grazer "My game"
cargo run --release --example play --features desktop -- --bundle target/my-game.grazer
```

The `.grazer` file contains title, config/seed/difficulty/VM limits, verified
bytecode/source mappings, atlas/sprite/tone resources and independent identity/
version checks. It needs no source/atlas files beside it. Load an archive in the
browser's development pane or with `Project::from_bytes`, then `create_game`.
Builder APIs support custom config/seed/limits; the CLI uses the standard 480×640
game defaults. Errors are explicit; corrupt/incompatible bundles do not replace
a running Game.

## Use an external Rust host

Create a separate Cargo project with a path dependency to an extracted Grazer
source package:

```toml
[dependencies]
grazer = { path = "../grazer" }
```

```rust
use grazer::{project::Project, GameInput};
let bytes = std::fs::read("my-game.grazer")?;
let project = Project::from_bytes(&bytes)?;
let mut game = project.create_game()?;
for _ in 0..600 {
    game.step(GameInput { fire: true, ..Default::default() })?;
    for sprite in game.sprites() { /* draw */ }
    for segment in game.laser_segments() { /* draw a thick segment */ }
    for sound in game.audio_events() { /* consume once */ }
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

Your loop advances fixed 60Hz simulation ticks and draws separately. Use the
provided `FrameClock` for wall-time scheduling. Input axes are -1/0/1; fire/Bomb/
focus/restart are explicit controls. Snapshot handles include kind/slot/generation
and belong to one world/run. Never retain stale handles after removal/reset or
mix handles from unrelated Games. The snapshot/resource IDs are enough to build
an external renderer/audio backend.

For native C/C++, link the shared/static library and include `grazer.h`.
`grazer_game_create_project` loads the bundle and existing HUD/sprite/beam/audio/
resource getters work unchanged. `grazer_game_restore_project` pairs a checkpoint
with the bundle's actual resources and program identity. `examples/c_project.c`
is a complete load/step/save/restore/ownership example. See [SDK/ABI](sdk-abi.md).

## Test and iterate

F1/F2 show hitboxes/CPU timings; P pauses, N steps, F5 reloads validated files,
F6/F7 save/restore a checkpoint and F8 advances 600 ticks. `--watch` reloads file
changes. Browser controls also support source editing, project/checkpoint/replay
loading, recording downloads and task/source/register inspection.

Record a reproducible failing run with `.grz`, retain its matching `.grazer`
resources and source identity, then seek to an earlier checkpoint and step.
Changing source or atlas starts a new run and archives an active recording;
incompatible imports are rejected. Do not compare performance while compiling,
running unrelated GPU workloads or enabling expensive debug overlays. The panel
reports CPU work/submission, not GPU completion/display latency.

## Distribute

Desktop artifacts contain the runner, C header/library, licenses, guides and
`prism-passage.grazer`. Replace the bundle with your archive and run
`grazer --bundle my-game.grazer`. Distribute the necessary OS runtime dependencies
according to that platform; Linux desktop audio requires ALSA. Release archives
are built separately for Linux, Windows and macOS by Distribution artifacts CI.

For browsers, build with the matching wasm-bindgen CLI, serve `web/` over HTTPS
(localhost is allowed for development), and ship the generated `pkg` and asset
files. Explicit `backend=webgpu`/`webgl` fail visibly if unavailable; `auto` allows
WebGL2 fallback. Browser audio starts only after a user gesture. External sites
must host their own same-origin stage/resource files or load a `.grazer` archive.

`scripts/build-web.sh` exports the demo. An archive can be loaded after startup;
keep a consistent asset set for that deployed version. Use the platform lifecycle
checks for scaling, focus/pause, audio unlock, resource/source errors and graphics
recovery. Signing, store packaging and hosting credentials are supplied by the
game publisher. This repository has not published a new crate/site as part of
release-candidate preparation.
