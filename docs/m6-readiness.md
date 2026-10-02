# M6 release readiness

Date: 2026-10-03. M6 is **in progress**, not complete. SDK/ABI, project packaging,
external consumption and platform automation are being prepared for 0.1.0.
The public crate remains M0 `0.1.0-alpha.1`. Formal performance gates and the
actual release remain pending. The user will provide an idle test window; no
other user application or system power setting has been changed for benchmarking.

## Implemented and locally verified

- Independently versioned resource/project archives include checked bytecode,
  config/seed/difficulty/VM budgets and atlas/sprite/tone definitions/digests.
  They reject malformed/version/content changes and instantiate a complete Game.
- Rust/C POD size/offset/version-family assertions define the compatibility
  baseline without changing existing gameplay/C protocol IDs.
- Desktop `--bundle`, browser import and C create/restore project APIs consume
  self-contained archives. Authoring and distribution guides are standalone.
- An external Rust project builds against the extracted package with no default
  dependencies and loads/steps/checkpoints/restores the bundled game. Rust and
  C external hosts both reach tick 600, hash `f652a18ae1afbee2`.
- Graphics recovery rebuilds presentation while retaining the exact Game state.
  A real software WebGPU device destroy/recreate keeps tick 240 and hash
  `95f66fed1bc76c39`, then advances normally. Resize/focus checks leave state
  unchanged. Shader/pipeline validation is awaited before renderer exposure.
- Current local tests include all historical golden traces, checkpoint/replay
  continuation, project archives and C layout assertions. The default dependency
  boundary is unchanged. Package verification/all-target builds pass.

`docs/authoring.md` describes an external game and `docs/sdk-abi.md` defines the
proposed compatibility/version/lifetime baseline. Distribution CI assembles
Linux/Windows/macOS executables plus C libraries/header and bundled game, and a
web artifact. CI results will be recorded against an actual source revision;
workflow configuration alone is not platform-run evidence.

## Full-frame benchmark and current gate

The new fixture uses a 1920×1080 simulation/surface, visible radius-2 hostile
circles, seeded motion up to 1.5 units/axis/tick, checked native movement,
player swept hit/graze, expiry/edge culling, stable pool recycling and upload/
normal instanced drawing. Replenishment keeps the requested count before every
tick and draw. Timing includes the replenishment work in the full frame.

Warmup 120 frames, samples 1,200, nearest-rank p95. Native rendering disables
vsync and waits for GPU completion. WebGPU waits on `queue.onSubmittedWorkDone`,
WebGL2 uses `finish`; periodic animation-frame yields are outside the timed work.
This measures work through GPU completion, not physical display/vsync latency.
The intended gate is simulation p95 ≤8 ms and full-frame p95 ≤16.7 ms, at desktop
100,000 and web 30,000 bullets. Hashes/counters verify equal workloads.

Current results were collected while other high-load applications were active:

| Execution | Count | Simulation p95 ms | GPU-complete draw p95 ms | Frame p95 ms | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| Native Vulkan, initial | 100000 | 29.435 | 11.787 | 38.654 | Not met |
| Native Vulkan, compaction/outer-test update | 100000 | 29.520 | 13.154 | 39.209 | Not met |
| Native Vulkan, CPU affinity diagnostic | 100000 | 34.303 | 14.986 | 45.723 | Not met |
| Hardware Chrome WebGPU | 30000 | 12.900 | 17.600 | 28.200 | Not met |
| Hardware Chrome WebGL2 | 30000 | 11.100 | 4.000 | 14.100 | Simulation not met |

The affinity diagnostic also reports simulation **thread CPU** p95 26.072 ms;
it is diagnostic data and does not replace the required wall-time frame gate.
Both native variants end at `611df99f167333eb`; both browser backends at
`f3d078b197af7a7d`, with 365 hits/1,190 grazes. The browser was verified through
Chromium SystemInfo and renderer output as NVIDIA RTX 3060 Laptop/Vulkan,
driver 615.71.9.0, rather than SwiftShader. It uses Chrome 150.

The Chrome setup follows [Chrome's hardware headless GPU guidance](https://developer.chrome.com/blog/supercharge-web-ai-testing).
No driver or OS power configuration was modified. Software-browser compatibility
and hardware-device identification are distinct from meeting the frame budget.
All failed/raw observations are preserved in ignored `target/m6-*` artifacts.
The user selected an idle-window retest; these loaded-run observations do not
certify M6 performance. Further optimization remains possible if idle runs fail.

## Reproduction

```sh
cargo run --release --features desktop --example frame_benchmark -- 100000 1200
sh scripts/build-web.sh
# Serve web on 8080, start an isolated hardware browser on the chosen CDP port.
node scripts/check-frame-benchmark.mjs webgpu 9230 30000 1200
node scripts/check-frame-benchmark.mjs webgl 9230 30000 1200
node scripts/check-release-browser.mjs webgpu 9227
cargo run --release --features resources --example project -- demo target/prism-passage.grazer
cargo build --release --features ffi
cc -std=c11 -Wall -Wextra -Werror -Iinclude examples/c_project.c -Ltarget/release -lgrazer -Wl,-rpath,"$PWD/target/release" -o target/c_project
target/c_project target/prism-passage.grazer
```

Linux x86_64/Wayland, Ryzen 7 5800H, RTX 3060 Laptop, Rust 1.97.1 release/thin-LTO,
Chrome 150. Native and browser conditions, exact sample sizes and evidence paths
are captured above. The previous milestones' isolated/headless-only results do
not satisfy this full-frame gate. Registry/site/public release writes will follow
the final accepted source/artifacts and applicable publication authorization;
none is claimed here.
