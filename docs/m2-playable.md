# M2 playable SDK

The repository implements First Sortie with `Game<DemoStage>`. Default builds
remain dependency-free; the native Rust SDK owns no window, clock, GPU or audio
device. `resources` enables JSON/file loading. `desktop` adds winit/wgpu/CPAL;
`web` adds the dedicated browser bindings and shared wgpu renderer.

## Run and controls

```sh
cargo run --release --example play --features desktop
cargo run --release --example play --features desktop -- --project assets/demo/project.json
# Smoke mode holds fire; it is not an invincible or winning pilot.
cargo run --release --example play --features desktop -- --autoplay --frames 600

rustup target add wasm32-unknown-unknown
# wasm-bindgen-cli must match Cargo.lock (currently 0.2.128).
sh scripts/build-web.sh
python3 -m http.server 8080 --bind 127.0.0.1 --directory web
```

Open `/?backend=auto`, `/?backend=webgpu` or `/?backend=webgl`. Enable sound
with the button. Arrow keys/WASD move, Z/Space fires, X activates a Bomb, Shift
halves movement speed, P pauses, R/Enter restarts. Touch buttons support held
movement/shooting/focus. The M0 stress demo is now at `motion.html`; its C ABI
and `desktop` example remain available.

The playfield is 480 × 640. Twenty waves spawn every six seconds for two minutes,
then a stationary Boss uses native fan and aimed shots. Boss health is 1,000;
twin shots fire every six ticks. The high-health, centered-fire validation run
clears at tick 10,195 (169.92 seconds). Normal play starts at three health and
three Bombs, and survival depends on dodging. Bombs clear hostile shots, deal
80 damage to every enemy and protect for 90 ticks including activation. They
use a rising input edge, so holding X spends one Bomb. Restart also uses an edge.

## Stage authoring

Implement `Stage` with a stable `CONTENT_ID`, `update(&mut Simulation)` and
`state_hash()`. The shipped [native stage](../src/game/stage.rs) is a complete
example. `StageStatus` supplies wave, optional Boss handle/max health and explicit
completion. Persistent content fields must contribute to the stage hash; avoid
clocks, OS RNG, filesystem/network access and unordered iteration. Reuse buffers
when retaining the built-in zero-allocation tick guarantee. M3 replaces this
native content with the dedicated language/VM.

```rust
use grazer::{Game, GameConfig, Stage, StageStatus, Simulation, SimulationError};
use grazer::resources::ResourcePack;
#[derive(Clone)]
struct MyStage;
impl Stage for MyStage {
    const CONTENT_ID: u64 = 1;
    fn update(&mut self, world: &mut Simulation) -> Result<StageStatus, SimulationError> {
        // Spawn/despawn enemies and projectiles at this tick boundary.
        Ok(StageStatus { complete: world.tick() >= 180 * 60, ..StageStatus::default() })
    }
    fn state_hash(&self) -> u64 { 0 } // no persistent fields in this example
}
let game = Game::with_stage(GameConfig::default(), 42, ResourcePack::builtin(), MyStage).unwrap();
```

`GameInput` owns x/y axes and fire/Bomb/focus/restart. C/browser flags are
`1=fire, 2=Bomb, 4=focus, 8=restart`; unknown bits and invalid axes are rejected
without mutation. Each active tick updates native content, handles Bomb/shot
commands, advances the M1 simulation with the selected speed, then updates
score, audio and phase. A simultaneous player death takes precedence over clear.
GameOver/Cleared freeze world ticks; restart recreates the original stage/seed.

Stage/command errors stop the Game with phase Faulted and a returned error;
partially executed native stage commands remain until restart. Commands are
not silently dropped. Ordinary built-in ticks, hashing, sprites, HUD and audio
event iteration allocate zero times; creation, cloning, restart, file/JSON
loading and browser marshaling can allocate.

`FrameClock` is a host helper: exact rational 60Hz accumulation, at most eight
ticks per rendered frame, retained overload backlog. Paused/focus-lost wall time
is discarded. Consume each tick's audio before advancing another tick. Hosts
own their audio clocks and synthesize resource-defined tones; sound/GPU output
does not feed back into simulation.

## Resources and snapshots

Resource format 1 uses a strict JSON project manifest, raw RGBA8 atlas and tone
metadata. `assets/demo/project.json` contains every rectangle/sound definition;
`sprites.rgba` has 128 × 128 × 4 bytes. `ResourcePack::new` accepts metadata and
bytes without JSON dependencies. `from_json`/`load` validate versions, lengths,
rectangle bounds, duplicate/nonzero IDs, count limits, tone ranges and relative
atlas paths. The GPU host also checks backend texture limits. Resource changes
require recreating the renderer; hot reload is later work.

Sprite IDs: 1 player, 2 enemy, 3 Boss, 4 player shot, 5 hostile shot, 6 heart,
7 Bomb, 8 star, 9 solid fill; HUD glyphs use 256 + uppercase/digit/punctuation
ASCII. Sound IDs: 1 shot, 2 Bomb, 3 damage, 4 explosion, 5 graze, 6 Boss entry,
7 clear, 8 death. Waveforms 0/1/2 mean square/triangle/sine. Tone values are
frequency Hz, duration ms and gain/256. The authored pixel patterns/exporter
are replaceable project resources; no external art or audio is required.

`Game::sprites()` returns owned `GameSprite` records: kind/slot/generation,
resource ID, world position, display dimensions, RGBA and layer. Display quads
are separate from circle hitboxes. `hud()` returns tick/score/graze/health/Bombs/
wave/Boss/phase/counts; `audio_events()` returns resource ID, per-tick sequence
and tick. The shared renderer draws layers 10/20/30 and adds its HUD around the
playfield. Renderer floats remain presentation-only.

Game protocol 3 hashes stage content/state, original stage, seed, resource content
fingerprint, M1 world, input/control edges, cooldown, Bomb stock/flash, score and
phase/status. Derived output audio and GPU/clock state are excluded. Asset hashes
are FNV divergence diagnostics, not cryptographic digests. `Game::clone` is an
in-process checkpoint. Replaying GameInput records requires matching content,
config/seed/resources; full saved gameplay replay metadata belongs to M5. The
M1 binary input/command format remains for the standalone `Simulation`.

## C host

ABI v2 uses new `grazer_game_*` entrypoints without changing ABI v1. See
[the header](../include/grazer.h) for layouts, ownership, flags and buffer queries.
The opaque Game supports seed, optional projectile-capacity/health overrides,
step/restart, hash, sprite/HUD/audio outputs, atlas bytes and sprite/sound metadata.
Output buffers belong to the caller; undersized buffers never partially write.
Handles and generations are scoped to a run; restart ends the previous run.

```sh
cargo build --release --features ffi --locked
cargo run --release --example trace --locked -- 100000 m2 > target/native-game-trace.txt
cc -std=c11 -Wall -Wextra -Werror -Iinclude examples/c_game.c -Ltarget/release -lgrazer -Wl,-rpath,"$PWD/target/release" -o target/c_game
target/c_game target/native-game-trace.txt
```

See [M2 validation](m2-validation.md) for actual platform evidence and limits.
