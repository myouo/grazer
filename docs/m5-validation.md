# M5 validation

Date: 2026-10-03. M5 reliable iteration is implemented and locally accepted. No
new crate release is published. Gameplay protocols and all M0–M4 golden traces
are retained. See [iteration tools and contracts](m5-iteration.md).

## Core and serialization

The local suite has 91 passing tests/documentation examples: 8 unit, 13 M4,
12 Game, 20 M1, 6 allocation, 13 language/VM, 9 reliable-iteration, 6 M0 and
4 documentation examples. Formatting, Clippy with warnings denied, Rustdoc with
warnings denied, default package verification and extracted all-feature/all-target
builds are checked.

M5 checks include complete scripted and native Game checkpoints, world and VM
RNG continuation, generational/free/dense pool layout, retirement/reuse, motion,
laser phases, once-per-generation graze, drops, score/controls, last world/audio
events, fault state and restart equivalence. Restored Games match hashes, HUD,
sprites, beams, world events and audio at every continued tick. Truncation,
corruption, wrong stage/resources/protocol and invalid pool fields are rejected.

Replay tests exercise encoded input/reset operations, periodic checkpoints,
fault-frame outcomes, seek/fast-forward/EOF, frame limits, atomic invalid input,
first-component divergence, persistent stop and recovery by valid earlier seek.
Pause/inspection/hitbox/profiling controls do not change authoritative state.
Failed reload preserves the Game and active recording; successful reload returns
the old recording, installs a fresh paused candidate and preserves its identity.

All three Boss entries have normal 3-health/3-Bomb loadout, power 4, their proper
phase/timer and reproducible starts at ticks 25,201 / 28,801 / 32,401. Session reset
returns to the exact initial practice checkpoint.

Restored playback and live debug tick stepping plus rolling p95 calculation
run 1,000 steps with **zero ordinary tick allocations**. Construction/recording
checkpoint encoding/restore/seek/reload/explicit inspection/presentation buffers
are outside that guarantee. M4's complete-stage allocation fixture still passes.

## Complete native-file cross-host replay

The shipped M4 conformance game is recorded for 100,000 input frames with 600-frame
checkpoints: **166 checkpoints, 15,084,649 bytes**. Label: `Prism Passage M5`.
Content `203edc647617ee21`, resources `afabadb97731dbdc`, seed 42, capacity 1,024,
health 10,000. Replay file SHA-256:
`eed5f6fc3120e2bdff7b7bb1590f8dd46514530518d4c3189db76e255ba8f23a`.

Native playback, the C host and actual WASM/Node all load that file and compare
every one of 100,000 Game hashes against the retained M4 trace; final hash is
`f45a50e4211ec63f`. Native/WASM seek to frame 0/1/600, phase entries/active beams,
clear, reset boundaries and EOF and compare recorded hashes. Out-of-range seek
rejects without replacing the current state. The C host checks seek/status,
owned Game clones, checkpoint query/restore and corrupt payload rejection.

Historical native/WASM M0/M1/M2/M3/M4 conformance, M1 encoded replay and M3 VM/RNG
restore still pass. Final M0–M4 hashes remain `b07e2bf531c8d412`,
`436de57247bc84be`, `b1b54e555b1dfae4`, `291dc8eab41e48d0`,
`f45a50e4211ec63f`; VM restore remains `1f29dd93b0380257`.

## Actual browser operations and pixels

Chrome 150 software WebGL2 and software WebGPU both pass 41-frame recording with
movement/shoot/focus and a reset, file encode/decode, full playback, checkpoint
resume, verified seek, invalid seek/corrupt import with state preservation,
normal-health Boss practice, pause/single-step, outlines/CPU panel/inspection,
source reload failure/success and atlas reload with incompatible replay rejection.

Both reproduce frame-20 hash `db2789b5d972be90`, frame-30
`e49025fd719cf9cb`, and the final post-reset hash `676d919afc35dbf7`.
Phase-2 practice begins with three health, three Bombs, power 4, four tasks and
hash `fa7c1f05e4748e9b`. One step reaches 28,802; reset returns to the same hash.
Reloading an invalid int/bool source reports a located diagnostic and preserves
the previous run. A valid `wave(77)` replacement restarts and executes correctly.

WebGPU texture readback confirms hit-circle outline `[255,82,102,255]`. Changing
the player's atlas center pixel through the resource reload path yields
`[0,0,255,255]`; the resource digest changes to `c6d9a2ce20b1caf6` and a replay
made with the previous atlas is rejected. Reads have no scoped GPU validation
error. Outlines use explicit-LOD sampling in the shared shader; initialization
awaits shader/pipeline validation so errors cannot appear as a ready renderer.

M4 WebGPU still passes both lasers, complete three-phase stage and 100,000 browser
hashes. M3 WebGL2 still passes source diagnostics, audio unlock, focus/Bomb,
pause/death/restart, full-stage clear and its 100,000 browser/native trace.
Software WebGL2 screenshots show the practice pane, source editor, entity/task
inspection and outlines. Software WebGPU compositor screenshots retain their
known limitation; texture readback is its output evidence.

## Actual desktop

Linux x86_64/Wayland, NVIDIA RTX 3060 Laptop GPU/Vulkan, 720 × 1152. The desktop
practice/debug/record run (`--practice 2 --hitboxes --performance --record ...
--autoplay --health 10000 --frames 300`) exits normally at tick 28,858, hash
`9fcfa125b38af089`, and saves 57 recorded steps. The headless verifier loads that
recording and reproduces the same tick/hash. CPAL schedules/starts ten events,
drops zero, peak 0.0639, failed=false. Observed CPU update+submit p95 4.064 ms over
180 samples after 120 warmup frames is short-run compatibility data, not M6's
fixed-workload performance acceptance.

The file-watch run starts paused, sees a changed source, validates/reloads it and
remains paused at tick zero. The final `wave(88)` source has content
`0b4b3018c2f20c2d`, final Game hash `9b367193fb3e697b`; 1,800 frames complete with
no runtime/GPU/audio error. Its subprocess collector's 50-second wait expired
before the window finished; the application subsequently wrote its normal final
summary. Core and browser tests separately verify invalid-source state retention.

Hardware-browser performance, Windows/macOS GUI/audio, physical-speaker audibility
and a new remote CI result are not claimed. The CI matrix includes replay file
generation/audit, C playback and actual WASM replay. M6 stable SDK/ABI and formal
platform/performance acceptance remain next.

## Reproduction and artifacts

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --release --locked
cargo run --release --example replay -- record target/showcase.grz 100000
cargo run --release --example replay -- audit
cargo run --release --example replay -- verify target/showcase.grz
cargo build --release --features ffi
cc -std=c11 -Wall -Wextra -Werror -Iinclude examples/c_replay.c -Ltarget/release -lgrazer -Wl,-rpath,"$PWD/target/release" -o target/c_replay
target/c_replay target/showcase.grz target/native-advanced-trace.txt
cargo build --release -p grazer-wasm-check --target wasm32-unknown-unknown
# Generate the retained traces and VM fixture listed in README first.
node scripts/check-wasm.mjs
sh scripts/build-web.sh
# Serve web on 8080 and launch isolated browsers as in m2-validation.md.
node scripts/check-iteration-browser.mjs webgl 9226
node scripts/check-iteration-browser.mjs webgpu 9227
cargo package --allow-dirty --locked --offline
cargo check --manifest-path target/package/grazer-0.1.0-alpha.1/Cargo.toml --all-features --all-targets --locked --offline --target-dir target
```

Ignored `target/` contains the replay, traces, compiled fixtures, test logs,
`m5-webgl-metrics.json`, `m5-webgpu-metrics.json`, screenshots and desktop logs.
Public source/examples/tests are packaged; local design material and generated
web artifacts are excluded. The local package contains 79 files, 814.8 KiB
uncompressed / 166.3 KiB compressed. No release was published.
