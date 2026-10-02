# M3 validation

Date: 2026-10-02. M3 implementation and local acceptance complete. No new crate
release is published; registry `0.1.0-alpha.1` remains M0. The default dependency
boundary stays unchanged. See [language/VM contracts](m3-language.md).

- All 65 tests/documentation examples passed: 6 unit, 12 Game, 20 M1, 4 allocation,
  13 language/VM, 6 M0 and 4 documentation examples. Clippy, Rustdoc with warnings
  denied and formatting passed.
- Language tests cover typed functions/branches/loops, short-circuit evaluation,
  exact fixed/vector motion, invalid syntax/types/arity/literals, recursion and
  absent OS/I/O capabilities. Runtime tests cover overflow/zero division, source
  line/function/task identity, instruction/command/birth/call-depth limits,
  stable fork/join/return/cancel, owner death, stale handles and fault persistence.
- Bytecode tests reject truncated/corrupt inputs, invalid targets, register types
  and definite-assignment violations. VM tests round-trip RNG, locals, wait/join/
  ownership/frame state and reject truncation or a different program. Restored
  simulation/VM hashes agree at every continued tick.
- First Sortie moved to six script functions/tasks and 155 instructions. Program
  fingerprint: `f206fc009ddae636`. Native/scripted worlds, HUD/audio and sprites
  match every tick through Boss clear at tick 10,195, score 3,720.
- The cloned scripted Game runs through 11,000 input frames with zero allocations
  for ordinary tick/fork/call/wait/owner-cancellation/hash work. Compiler/load/
  snapshot/restart allocations are outside this guarantee.
- Native, C, actual WASM/Node, WebGL2 browser and WebGPU browser compare all
  100,000 scripted Game hashes. Final: `291dc8eab41e48d0`; trace SHA-256:
  `ad6d936d11cc347844c111ed0a4802d9ba55dcfc5e9c3b73977c68075aae8fef`.
- Native and WASM additionally serialize halfway through a 10,000-tick RNG/wait
  workload, restore to a matching world checkpoint, and compare every subsequent
  tick. Final VM hash: `1f29dd93b0380257`.
- The script C host completes 8 clears and consumes 15,923 audio events while
  comparing every hash. It also exercises compile-time type diagnostics,
  infinite-loop budget faults, stopped tick/phase and diagnostic text. Native
  M2 and legacy C ABIs keep their historical fixtures/hashes.

## Actual runners

Linux x86_64/Wayland, AMD Ryzen 7 5800H, NVIDIA RTX 3060 Laptop GPU, Rust 1.97.1
release + thin LTO. The scripted desktop `play --autoplay --frames 3000` runs
Vulkan at 720 × 1152 and exits 0 at tick 1,078. CPAL schedules/starts 189 events,
drops zero, reports no errors and generates peak sample magnitude 0.2308.
CPU update+submit p95: 14.504 ms, 1,200 samples after 120 warmup frames. Other
validation load overlapped this run; it is compatibility evidence, not a final
performance budget or GPU duration measurement.

Chrome 150 software WebGL2 and WebGPU both load the source, render the shared
sprite/HUD pipeline, unlock audio with a real click, handle shoot/focus/Bomb,
pause, default-health death/restart and complete the full script Boss. The
high-health full-stage fixture is the same controlled validation approach as M2;
normal gameplay stays at three health and three Bombs. Both backends display a
type error and a located infinite-loop budget error; the latter has phase
Faulted and tick 0. WebGPU readback confirms player `[149,255,255,255]`, Boss
wing `[94,64,103,255]` and background `[4,5,11,255]` without a scoped validation
error. At clear, both browser hosts report hash `5ca198683b794327`.

Hardware browser performance, Windows/macOS GUI/audio and new remote CI results
are not claimed locally. The matrix includes script hashes/C host and VM restore
continuation. The software WebGPU screenshot compositor limitation from M0/M2
still applies; readback is the output evidence. Physical-speaker audibility is
not separately recorded.

## Package and reproduction

The local package includes 55 files, 570.2 KiB uncompressed / 115.9 KiB compressed,
with compiler/VM, shipped source, malformed-source test fixtures and CLI/hosts.
Default and extracted all-feature/all-target builds are checked; local design
and generated web assets are excluded. No full serialized Game/world replay
format, advanced M4 patterns/lasers or M6 performance claim is introduced.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo run --release --example script -- compile assets/demo/first_sortie.graze target/first_sortie.gzb
cargo build --release --features ffi --locked
target/c_game target/native-script-trace.txt script
sh scripts/build-web.sh
# Isolated browser flags/server setup are in m2-validation.md.
node scripts/check-game-browser.mjs webgl 9226
node scripts/check-game-browser.mjs webgpu 9227
cargo package --allow-dirty --locked --offline
cargo check --manifest-path target/package/grazer-0.1.0-alpha.1/Cargo.toml --all-features --all-targets --locked --offline --target-dir target
```

Generate native M0/M1/M2/M3 traces plus `script restore-check` before running
`scripts/check-wasm.mjs`. Generated traces, screenshots, measurements and `.gzb`
go under ignored `target/`; web export includes its diagnostic test sources.
