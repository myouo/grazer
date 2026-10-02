# M4 validation

Date: 2026-10-02. M4 implementation and local acceptance complete. The registry
`0.1.0-alpha.1` release remains M0; no new release is published. The default
library still has zero third-party dependencies. See [creation contracts](m4-advanced.md).

## Core acceptance

- 79 tests/documentation examples pass: 6 unit, 13 M4, 12 Game, 20 M1, 5 allocation,
  13 language/VM, 6 M0, 4 documentation examples. Formatting, Clippy with warnings
  denied, Rustdoc with warnings denied and extracted-package builds pass.
- M4 tests cover exact cardinal/wrapped angles, full-range integer aiming,
  ring/fan/aimed/spiral order, single-bullet fans, atomic capacity failure,
  acceleration/turn integration and atomic overflow. Laser checks cover warning/
  active/fade timing, persistent blocked hits, once-per-generation grazing,
  16-point curve geometry/segment snapshots, expiry/reuse, invalid timing/shape
  and the 64-beam bound.
- Drop checks cover reserved death rewards, capacity rejection, stable generation
  reuse, swept pickup, point/power/Bomb rewards/caps, difficulty score multiplier,
  rewarded cancellation, restart and preservation of higher custom Bomb stock.
  Player death takes precedence over simultaneous completion.
- All nine creation examples compile, bytecode-round-trip and execute without
  faults. The shortened Boss example defeats/transitions through three phases,
  awards phase bonuses and clears at tick 721.
- A cloned showcase runs 36,002 input frames, including composed motion, both
  lasers, tagged enemy deaths, drop pickup and presentation iteration, with
  **zero ordinary tick allocations**. Construction/clone/compiler/snapshot/
  restart and host-owned presentation buffers are outside this guarantee.
- The complete showcase reaches all three phases, both laser shapes, power/
  pickup/cancellation, then commits victory at tick 36,001. During phase 2 a VM
  is serialized/restored into a cloned matching advanced world; all subsequent
  VM/status/world hashes match for 500 ticks.

Prism Passage has 18 functions/tasks and 432 instructions. Program fingerprint:
`203edc647617ee21`. Resource hash remains `afabadb97731dbdc`.

The full authored stage runs at all three difficulties with health 10,000 and
fixed fire/focus/Bomb input, using the ordinary 8,192-projectile game config:

| Difficulty | Clear tick | Score | Grazes | Collected | Cancelled | Phase bonus | Peak shots | Peak beam segments | Clear hash |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| Easy | 36001 | 36060 | 347 | 76 | 446 | 15000 | 90 | 15 | `a8fb63c0d5c28c71` |
| Normal | 36001 | 72380 | 428 | 74 | 747 | 30000 | 117 | 15 | `b05ee0ff4aa4276c` |
| Hard | 36001 | 63690 | 664 | 70 | 908 | 0 | 153 | 15 | `303d2ff74000527c` |

All reach power 4 and exactly three phases; restart returns the initial hash.
Hard's fixture survives timers rather than defeating those higher-health phases,
so it earns no phase-defeat bonus. These are deterministic acceptance fixtures,
not a claim that a human can finish without taking damage.

## Native, C and WASM

Native, C, actual WASM/Node, WebGL2 and WebGPU compare **every one of 100,000 M4
Game hashes**, using capacity 1,024, health 10,000, seed 42 and the fixed showcase
input. Final hash `f45a50e4211ec63f`; trace SHA-256:
`9386e13ee68c41e476d850d3f759ee617cd9b8ecbc0ede4f31d554c1c862fbe2`.

Both M4 C source and compiled-bytecode constructors pass that trace, with two
clears and 15,938 audio events. They check the 48-byte advanced HUD, 40-byte beam
POD, three phases, both shapes, difficulty, bulk counts/no-partial writes,
restart, invalid inputs/options and existing script diagnostics.

Historical M0/M1/M2/M3 native/WASM goldens remain `b07e2bf531c8d412`,
`436de57247bc84be`, `b1b54e555b1dfae4`, `291dc8eab41e48d0` respectively.
Native M2 and scripted M3 C hosts also pass all 100,000 hashes, each with eight
clears and 15,923 audio events. M1 encoded replay and M3 serialized RNG/wait
continuation run inside WASM; the latter still ends at `1f29dd93b0380257`.

## Actual presentation

Chrome 150 software WebGL2 and software WebGPU both load the default M4 source,
render waves/drops, reach Boss phases 1/2/3, display both laser warnings/active
beams, clear at tick 36,001, reset inventory/phase/hash on restart and accept
Easy/Hard selection. Their controlled run with one Bomb in phase 1 ends with
score 68,720, power 4, collected 74, cancelled 167, phase bonus 30,000 and hash
`3ce1f9d1d443b2af` (capacity 8,192). The separate conformance helper uses 1,024.

WebGPU texture readback confirms straight and curved active cores
`[238,255,255,255]`, background `[4,5,11,255]`, with no scoped GPU validation
error. WebGL2 screenshots confirm the curved strip/endcaps, bullets, Boss, player
and phase/timer/power HUD. Software WebGPU compositor screenshots can be blank;
its texture readback is the image-output evidence. M3 WebGL2 still passes
shoot/focus/Bomb, audio unlock, pause, default-health death/restart, full-stage
clear, compile/runtime faults and its 100,000-frame browser trace.

Linux x86_64/Wayland desktop, NVIDIA RTX 3060 Laptop GPU/Vulkan, 720 × 1152:
`play --script assets/examples/curve_laser.graze --autoplay --health 10000
--frames 300` exits 0 after 300 frames/tick 315, with no renderer/runtime fault.
CPAL schedules/starts 53 events, drops zero, peak sample 0.0639, failed=false.
Observed CPU update+submit p95 is 37.770 ms over 180 samples after 120 warmup
frames while release builds/browser validation overlap. This short concurrent
compatibility run is not a steady performance budget or GPU-duration measure.
M6 performance targets, hardware-browser timings, Windows/macOS GUI/audio and
new remote CI results are not claimed. Physical-speaker audibility is not recorded.

## Package and reproduction

Local package: 69 files, approximately 666.6 KiB uncompressed / 136.6 KiB
compressed. It includes M4 core/script/hosts and all nine creation examples.
Default verification and extracted all-feature/all-target builds pass. Local
design material and generated web/target artifacts remain excluded. The CI
matrix now includes M4 native/C/WASM traces; no remote run is claimed locally.
Repository stage sources use LF on every checkout to keep program fingerprints
stable across Windows, macOS and Linux.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --release --locked
cargo run --release --example advanced
cargo run --release --example trace -- 100000 m4 > target/native-advanced-trace.txt
# Generate the retained M0/M1/M2/M3 traces and restore fixture listed in README.
cargo build --release --features ffi --locked
cc -std=c11 -Wall -Wextra -Werror -Iinclude examples/c_game.c -Ltarget/release -lgrazer -Wl,-rpath,"$PWD/target/release" -o target/c_game
target/c_game target/native-advanced-trace.txt advanced
cargo run --release --example script -- compile assets/demo/advanced_showcase.graze target/showcase.gzb
target/c_game target/native-advanced-trace.txt advanced-bytecode target/showcase.gzb
cargo build --release -p grazer-wasm-check --target wasm32-unknown-unknown
node scripts/check-wasm.mjs
sh scripts/build-web.sh
# Serve web on port 8080; browser setup flags are in m2-validation.md.
node scripts/check-advanced-browser.mjs webgl 9226
node scripts/check-advanced-browser.mjs webgpu 9227
cargo package --allow-dirty --locked --offline
cargo check --manifest-path target/package/grazer-0.1.0-alpha.1/Cargo.toml --all-features --all-targets --locked --offline --target-dir target
```

Ignored `target/` holds native traces, compiled bytecode, screenshots,
`m4-webgl-metrics.json`, `m4-webgpu-metrics.json` and `m4-desktop-curve.log`.
