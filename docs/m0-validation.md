# M0 validation

Date: 2026-09-10. Status: M0 implementation, validation and publication complete.
Published [grazer 0.1.0-alpha.1](https://crates.io/crates/grazer/0.1.0-alpha.1).

M0 validates deterministic linear motion and presentation. It does not establish
the eventual 100,000 desktop / 30,000 Web bullet performance target with collision
and grazing. Timing of CPU submit is not GPU execution time.

## Correctness and platform evidence

- Rust unit/integration/doc tests cover fixed-point overflow and rounding,
  negative/large motion wrapping, invalid commands, atomic spawn batches, stable
  IDs, checkpoint equivalence, RNG golden values and a 100,000-tick golden trace.
- Native and actual WASM in Node compare all 100,000 tick hashes. Final hash:
  `b07e2bf531c8d412`. Both browser backends reproduce the same entire trace;
  SHA-256 of its newline-separated hexadecimal representation is
  `182db3ef45eb1e14668df2683c209721534edf5a2a85a13d6a96d277914a5b22`.
- [Initial CI](https://github.com/myouo/grazer/actions/runs/34430779883) passed
  Linux, Windows, macOS and WASM. Native jobs execute Rust tests, compile the
  desktop example, compile and execute the C host, and verify Cargo packaging.
  These jobs do not validate desktop GUI presentation on Windows or macOS.
- [Final source CI](https://github.com/myouo/grazer/actions/runs/34431547273)
  passed all four jobs for `ad59afa1d456881acfb5921aa4628ab5a1842092`, including
  the lifecycle fix, auto-backend detection and additional conformance tests.
  Rust-cache emitted non-fatal missing-directory cleanup annotations; test,
  compilation, C host and package verification steps all succeeded.
- The C host checks ABI version/layout, null arguments, capacity failures,
  unchanged state after rejected input, snapshot size queries, no partial
  writes, expected positions, and creation/destruction. Native C fixture hash:
  `c93b8966b5256127`.
- Linux/Wayland desktop renders 100,000 bullets at 1920×1080 on an NVIDIA RTX
  3060 Laptop GPU (Vulkan). A shutdown crash was traced to destroying GPU/window
  resources after the event loop's Wayland display connection. Releasing them
  in `ApplicationHandler::exiting` fixed the crash; a 300-frame rerun exited 0.
- Chrome 150 runs explicit WebGPU and WebGL2 with 30,000 bullets. Both passed
  rendering calls, user-gesture AudioContext unlock, pause and native/browser
  conformance. WebGL2 screenshots show sprites. WebGPU pixel readback verifies
  player `[255,255,255,255]` and background `[4,5,11,255]`.
- Automatic backend detection was tested in a browser with no WebGPU adapter:
  `backend=auto` selected WebGL2 and rendered successfully.

## Browser validation limitations

The default headless browser offered no WebGPU adapter. For reproducible testing,
isolated Chrome instances used software Vulkan/SwiftShader. Linux hardware
WebGPU attempts encountered driver/shared-image failures. Software WebGPU
submitted valid frames, but this headless compositor captured a black canvas;
GPU readback confirmed actual pixels. Hardware WebGPU presentation and normal
Windows/macOS browser presentation remain outside this local coverage.

The diagnostic script temporarily enables COPY_SRC on the browser canvas and
checks WebGPU validation errors before readback; this is test configuration,
not a runtime rendering requirement. Audio checks establish a running context
and scheduled test tone, not physical speaker audibility.

## Initial performance measurements

Reference host: AMD Ryzen 7 5800H (16 logical CPUs), NVIDIA RTX 3060 Laptop GPU,
Linux x86_64/Wayland, Rust 1.97.1, release + thin LTO. Nearest-rank p95, 120
warmup ticks/frames. CPU motion sample count is 1,200. Browser buffers are
1920×1080; the playfield is aspect-preserving 4:3 within that buffer.

| Workload | Samples | p95 |
| --- | ---: | ---: |
| Native 100,000 bullets, motion only | 1,200 ticks | 0.4724 ms |
| Native 30,000 bullets, motion only | 1,200 ticks | 0.1406 ms |
| Desktop 100,000, CPU update + submit, concurrent validation load | 180 frames | 17.243 ms |
| Final desktop build, 100,000, CPU update + submit | 1,200 frames | 16.035 ms |
| Software WebGPU 30,000, CPU update + submit | 64 frames | 754.6 ms |
| Software WebGPU 30,000, rAF interval | 64 frames | 766.7 ms |
| Software WebGL2 30,000, CPU update + submit | 61 frames | 6.4 ms |
| Software WebGL2 30,000, rAF interval | 61 frames | 1,250 ms |

Browser runs overlapped other validation work and establish compatibility only.
Software rendering does not meet the eventual 60Hz presentation target. The
simulation retains accumulated ticks under overload and limits catch-up work
per rendered frame. No collision/grazing costs are included at M0.

The final desktop run rendered 1,320 frames including warmup and exited 0.
Both browser backends were rechecked against the final build; WebGPU readback,
audio unlock, pause, and all 100,000 browser/native trace hashes passed.

## Publication

`cargo publish --dry-run --locked` passed. The generated package contains 22
files, 130.4 KiB uncompressed / 37.3 KiB compressed. Both its default build and
all-feature/all-target compilation were verified from the packaged sources.

After the account owner verified their email, the authorized
`cargo publish --locked --registry crates-io` succeeded and Cargo confirmed
registry availability. Published source commit:
`5f7aa34dc9a5a2a5f1b1b80c34c1de68ed1bbce6` (the validated runtime plus its report).

An independent consumer at `/tmp/grazer-consumer` downloaded the registry
package with `grazer = { version = "=0.1.0-alpha.1", features = ["ffi"] }`.
Its release build passed Rust API checks, C ABI version checks and the
100,000-tick golden trace. The consumer lockfile confirms the crates.io registry
source and package checksum:
`f1d943e20de606aee49934f223655cb3aa42ed5170010ceebac4eb5b45623978`.

## Reproduction

Use the commands in README for core, C, motion, desktop and WASM tests. Generate
`target/native-trace.txt` before browser checks. Start an isolated browser using
the following test-only configurations (the unsafe flags are limited to these
local test profiles):

```sh
# WebGPU software Vulkan
google-chrome-stable --headless=new --user-data-dir=/tmp/grazer-webgpu-test --remote-debugging-port=9225 --enable-unsafe-webgpu --use-webgpu-adapter=swiftshader --use-angle=vulkan --use-vulkan=swiftshader --enable-features=Vulkan --disable-vulkan-surface about:blank
node scripts/check-browser.mjs webgpu 30000 9225

# WebGL2 software ANGLE
google-chrome-stable --headless=new --user-data-dir=/tmp/grazer-webgl-test --remote-debugging-port=9223 --use-angle=swiftshader --enable-unsafe-swiftshader about:blank
node scripts/check-browser.mjs webgl 30000 9223
```

See [Chromium's SwiftShader documentation](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/docs/gpu/swiftshader.md)
and [Chrome's headless GPU testing guide](https://developer.chrome.com/blog/supercharge-web-ai-testing).

Generated screenshots and JSON measurements go to ignored `target/`. The
published crate includes source, examples, C header, licenses and README. Local
design documents are excluded from both Git and the Cargo package.
