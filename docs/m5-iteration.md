# M5 reliable iteration

M5 adds portable complete-game checkpoints, input/control replay, verified seek,
Boss practice, tick stepping, collision outlines, CPU timings, state inspection
and atomic source/resource reload. Gameplay remains protocol 3/4; checkpoint and
Game replay formats are independently versioned at 1. The default library still
has no third-party dependencies.

## Desktop and browser controls

```sh
cargo run --release --example play --features desktop -- --practice 2 --hitboxes --performance
cargo run --release --example play --features desktop -- --record target/run.grz
cargo run --release --example play --features desktop -- --replay target/run.grz
cargo run --release --example play --features desktop -- --script my_stage.graze --watch
cargo run --release --example play --features desktop -- --checkpoint target/start.gcp --paused
```

| Desktop control | Action |
| --- | --- |
| P | Pause/resume, discard paused wall time |
| N while paused | One complete simulation tick |
| F1 / F2 | Collision outlines / CPU panel |
| F5 | Reload stage and project files; validate before replacing the run |
| F6 / F7 | Save/restore an in-memory checkpoint |
| F8 | Execute 600 intermediate ticks without rendering/audio |
| F9 | Start recording / save completed recording |
| R / Enter | Restart the current live/practice/recording start |

`--practice 1..3` starts a Prism Passage Boss phase. `--record`, `--replay` and
`--checkpoint` name files. `--paused`, `--hitboxes`, `--performance` select initial
debug controls. `--watch` detects changed source/bytecode or project/atlas/tone
content once per second; F5 supports the same source/bytecode loading rules as
startup. Both source and resources must validate before committing a reload.

The browser's **Practice and development** pane offers stepping, 600-tick
advance, hitboxes, CPU panel, three Boss entries, replay recording/download,
checkpoint download, `.grz`/`.gcp` loading, frame seek, task/entity inspection,
source editing and file reload. N steps while paused; F5 reads fresh stage/project
assets. Source editing uses **Apply source and restart**. Errors appear in the
pane, with source locations where available, while the previous run stays intact.

Reload restarts a validated candidate at tick zero, then pauses for review. It
does not patch a running VM. A successful reload/restore/practice switch archives
an active recording rather than mixing program/resource identities. Failed
compile, resource validation or incompatible import keeps the current Game and
recording. Browser archives download as `.grz`; desktop saves to `--record` or
`target/desktop-replay.grz`. Resume when ready.

## Complete checkpoints

`Simulation::checkpoint/restore_checkpoint` save config, pending input, world RNG,
player state, every pool slot/generation/dense/free order, previous positions,
remaining lifetimes, graze flags, pending removals, motion controllers, laser
timing, death rewards, drop pool/metrics and committed events.

`Game::checkpoint/restore_checkpoint` additionally bind the stage identity,
program, resource summary, config/seed, initial and current stage/VM, score,
Bombs/power, shot cooldown, flash, phase/status, input edges and last audio events.
It can save Playing, GameOver, Cleared and Faulted state. Initial-stage state is
included so a restored Game also restarts identically.

```rust
use grazer::{game::showcase, advanced::Difficulty};
use grazer::language::ScriptStage;
let game = showcase::game(Difficulty::Normal, 0).unwrap();
let bytes = game.checkpoint().unwrap();
let restored = grazer::Game::<ScriptStage>::restore_checkpoint(
    game.resource_pack(), &bytes).unwrap();
assert_eq!(game.state_hashes(), restored.state_hashes());
```

Restore checks magic/version/tick rate, lengths/checksums, source/bytecode/VM,
resource summary, config, pool occupancy/generations/free ordering, lifetimes,
shapes, mode/timing, reserved death-drop capacity, event bounds, Game counters,
VM/world tick pairing and component fingerprints. Failed loading never mutates
an existing Game. Ordinary world and replay steps still reuse preallocated
buffers; encode/restore/clone/seek/reload may allocate.

Native `DemoStage` and `ScriptStage` implement `game::checkpoint::CheckpointStage`.
Custom Rust stages implement `save_stage`, `restore_stage` and a content identity,
including every persistent field. Change CONTENT_ID when changing native stage
semantics. Script checkpoints embed verified bytecode and full VM continuation.
Rendering handles, clocks, timing panels and audio device state are excluded.

## Game replay and seek

`GameRecorder` exclusively owns its Game so external authoritative mutations
cannot bypass the recording. It records accepted Step inputs and explicit Reset
operations, frame outcomes (including reproducible Game faults), tick and seven
component hashes. Invalid inputs and a reached frame/checkpoint limit reject
before a new step. The default reserves 100,000 frames and checkpoints every 600
frames. Interval 0 disables periodic checkpoints. Snapshot encoding/file-size
failure can occur after an accepted frame has been captured; stop/save the log
and address the reported limit instead of advancing blindly.

```rust
use grazer::game::{showcase, replay::{GameRecorder, GameReplay,
    RecordingOptions, ReplayPlayer}};
use grazer::language::ScriptStage;
use std::sync::Arc;
let mut recorder = GameRecorder::new(showcase::conformance_game(),
    RecordingOptions::default(), "Example run").unwrap();
for frame in 0..1200 { recorder.step(showcase::input(frame)).unwrap(); }
let bytes = recorder.finish().to_bytes().unwrap();
let replay = Arc::new(GameReplay::from_bytes(&bytes).unwrap());
let mut player = ReplayPlayer::<ScriptStage>::new(replay,
    Arc::new(grazer::resources::ResourcePack::builtin())).unwrap();
player.seek(900).unwrap();
player.fast_forward(300).unwrap();
```

Metadata includes format/tick rate, Game/world protocols, stage/content hash,
full resource digest plus dimensions/atlas size/sprite/tone counts, config,
difficulty/drop capacity, seed, initial tick/component hashes and a label. The
initial checkpoint may be a mid-stage/practice state. Resources must be supplied
with the same summary/digest; missing/different resources or unsupported
protocols fail explicitly. Standalone typed players restore the embedded program;
interactive sessions additionally require the current program identity to match
imports. Replay configuration is restored from its metadata, not silently mixed
with current live settings.

Replay **frame** counts operations; world **tick** may freeze on end/fault or
return to an earlier tick on reset. Seek accepts `0..=frames`, restores the nearest
preceding checkpoint and verifies every remaining operation. A failed seek keeps
the current player intact. Fast-forward executes all ticks/commands in sequence.
Hosts suppress intermediate graphics and audio rather than skipping simulation.

The first mismatch reports frame, actual tick, component and expected/actual
fingerprint. Components are Game, World, Stage/VM, Player, Enemies, Projectiles,
Drops, Outcome and Tick. Playback stops after divergence; seek can recover to a
valid earlier boundary. Checkpoints are bound to recorded frame fingerprints;
full replay verification from frame zero checks the entire input history. FNV
checksums/fingerprints detect corruption and differences, not authenticity.

Bounds: checkpoints 64 MiB, replay 256 MiB, 1,000,000 frames, 4,096 periodic
checkpoints, 4,096-byte label. Existing M1 input/command replay format 1 is retained
as a separate protocol, and all M0–M4 historical gameplay hashes are unchanged.

## Practice, inspection and timings

Prism Passage practice reaches a phase deterministically with rendering/audio
suppressed and temporary player protection, then creates a new initial
checkpoint: configured health (normally 3), configured Bomb stock (normally 3),
power 4, zero score/grazes/rewards, clean friendly shots/drops and 120 protected
ticks. The phase's existing enemy/VM/beam timing is retained. Entry ticks are
25,201 / 28,801 / 32,401. Session restart returns to that exact practice start.
The underlying `GameInput.restart` still has its historical full-stage behavior;
official runtime R/Restart controls use session reset.

`DebugSession` separates live, recording and playback runners. Paused normal
advance has no Game/input/replay effects; `single_step` requires pause and runs a
whole tick. Debug flags and profiling never enter authoritative hashes. Borrowed
`Vm::inspect_tasks` exposes task/parent/owner/wait/join state, call frames, next PC,
source span/line/column and typed registers. Entity snapshots expose generational
tokens, raw fixed position/radius and health. Browser inspection caps the shown
entities at 256 and reports total count. These are runtime inspection facilities;
the editor control protocol remains M7 scope.

Collision outlines draw circles and each capsule of thick polylines. Player hit
and graze circles use separate colors; warning/fading laser geometry has a
different tint from active hazards. These floats describe the current boundary
for display, while swept contact remains integer simulation.

The CPU panel uses host-supplied monotonic update/draw-submit/frame durations and
a fixed 240-sample rolling window; p95 is nearest-rank. Invalid/negative/nonfinite
or excessively large samples are ignored. Draw-submit timing is not GPU execution
or display/vsync latency. The panel itself and validation load affect timings;
M6's fixed-device final frame budget is a separate acceptance.

## Tools and C interface

```sh
cargo run --release --example replay -- record target/showcase.grz 100000
cargo run --release --example replay -- verify target/showcase.grz
cargo run --release --example replay -- seek target/showcase.grz 28891
cargo run --release --example replay -- practice 2 target/boss2.gcp
cargo run --release --example replay -- checkpoint target/boss2.gcp
cargo run --release --example replay -- audit
```

Additive checkpoint API v1 preserves game ABI v2. `grazer_game_checkpoint` uses
count/query buffers; restore creates an owned Game. `grazer_replay_load` creates
an owned player; step returns whether a frame advanced; seek/status/hash and an
owned Game clone expose playback to external hosts. C restoration uses the same
builtin resource pack as its existing constructors. Data, incompatibility and
divergence are separate error codes. See `include/grazer.h` pointer/lifetime rules
and `examples/c_replay.c` for complete use. The WASM harness consumes the actual
native `.grz` file and compares every frame, then exercises checkpoint seeks.
