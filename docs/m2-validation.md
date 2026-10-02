# M2 validation

Date: 2026-10-02. Status: implementation and local acceptance complete.
M2 is repository source work; the published `0.1.0-alpha.1` package remains M0.
See [the playable SDK](m2-playable.md) for controls, APIs and reproduction.

## Correctness and shared-host evidence

- All 49 tests/documentation examples passed with all features: 5 unit tests,
  12 Game tests, 20 M1 tests, 3 allocation tests, 6 M0 tests and 3 documentation
  examples. Formatting, all-target/all-feature Clippy and Rustdoc with warnings
  denied also passed.
- Tests cover held-fire cadence, half-speed focus, Bomb rising edges and
  before-contact damage/clear/protection, player death/freeze/restart, Boss
  damage/clear/HUD/audio, stage errors, exact host-clock backlog/pause behavior,
  resource validation/canonical IDs, input replay and checkpoint continuation.
- The complete Rust-native stage reaches the Boss and clear within the nominal
  180-second boundary. Browser-host validation clears at tick 10,195, or
  169.92 simulated seconds, after 20 waves and a 1,000-health Boss.
- Built-in Game ticks, state hashing, sprite/HUD/audio iteration allocate zero
  times after construction, including a cloned world. Resource loading,
  creation/clone/restart, GPU operations and JS marshaling are outside that claim.
- Native and actual WASM/Node compare every one of 100,000 protocol-3 frame
  hashes, including repeated Bomb use, focus, Boss defeat and restart. Final
  hash: `b1b54e555b1dfae4`. The newline-separated trace SHA-256 is
  `7faf074e8957a8044febc93b30979603e2dbd09cb561ef3b25aa24ae5c8409f5`.
- The real C game host consumes shared sprites, HUD, audio, RGBA atlas and
  rectangle/tone metadata. It compares all 100,000 hashes with the Rust trace,
  completes 8 clears and receives 15,923 audio events. It checks creation/version
  errors, rejected input atomicity, output queries, no partial writes, resource
  layouts, event ordering and restart equivalence.
- Resource format 1 content fingerprint is `afabadb97731dbdc`. Atlas export and
  JSON loading match the embedded pack. Legacy C ABI still reports
  `c93b8966b5256127`; M0/M1 native/WASM golden hashes remain `b07e2bf531c8d412`
  and `436de57247bc84be`.

Full-stage automated runs use the same shipped content with a 10,000-health
fixture so a fixed input stream can test the entire stage without a human
dodger. The C/raw-WASM fixture uses a 512-projectile pool; browser gameplay
uses the normal 8,192 pool. Normal play starts with three health and three Bombs.
Default-health browser death/freeze/restart was tested separately. Automated
clear time does not establish that an undodged normal-health run can win.

## Actual presentation and sound

Host: AMD Ryzen 7 5800H, NVIDIA RTX 3060 Laptop GPU, Linux x86_64/Wayland,
Rust 1.97.1 release + thin LTO. Browser: Chrome 150; raw WASM: Node v24.15.0.

- Linux desktop actually renders through Vulkan at 720 × 1152. The final
  `play --autoplay --frames 6000` run exits 0, advances 631 real-clock ticks,
  schedules and starts 114 audio events, drops zero events and reports no audio
  errors. The CPAL output callback observes peak sample magnitude 0.2308.
  This verifies a running output stream and generated samples, not a separately
  recorded physical-speaker audibility check.
- Its CPU update+submit p95 is 11.223 ms over 1,200 samples after 120 warmup
  frames. This includes surface waiting and host audio submission, not GPU
  execution time. It is an initial-stage compatibility sample with a small
  entity count, not the final 100,000-bullet/1080p performance workload.
- Isolated software WebGL2 and WebGPU browsers both render the same atlas/HUD,
  accept shoot/focus/Bomb controls, unlock audio through a real click, pause
  without advancing ticks, handle default-health death/freeze/restart, render
  Boss/clear scenes and reject missing resource files visibly.
- Both browser backends reproduce all 100,000 Game frame hashes. For a
  10,000-health, centered-fire run at tick 7,201 they report Boss health 998;
  at clear tick 10,195 they report score 3,720 and identical state hash
  `b61012bdbf2c958f`.
- WebGL2 screenshots show actual sprites/text. Software WebGPU pixel readback
  verifies player `[149,255,255,255]`, Boss wing `[94,64,103,255]` and background
  `[4,5,11,255]` with no scoped WebGPU validation error. The Boss probe samples
  its wing to avoid the bullets spawned over the Boss center.

The headless WebGPU compositor can capture a black canvas despite correct GPU
texture output, as observed in M0; pixel readback is the WebGPU presentation
evidence. Browser runs use SwiftShader and establish compatibility, not hardware
browser performance. Local Windows/macOS GUI/audio execution is not claimed.
The existing CI matrix now includes M2 tests, both C hosts and three protocols;
Linux CI explicitly installs the ALSA build dependencies. New remote CI results
are pending the next repository push.

## Package and reproduction

`cargo package --allow-dirty --locked --offline` passed default package
verification. All-feature/all-target compilation of its extracted sources also
passed. The package contains 44 files, 414.8 KiB uncompressed / 85.0 KiB compressed,
including Game/stage/audio/renderer/ABI source, actual JSON/RGBA assets, C/Rust
hosts and tests. Local ignored design files and generated web assets are excluded.
The default dependency tree remains only `grazer`, without third-party crates.
No new registry release was published.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo build --release --features ffi --locked
cargo run --release --example trace --locked -- 100000 m2 > target/native-game-trace.txt
target/c_game target/native-game-trace.txt # compile using m2-playable.md first
cargo build --release -p grazer-wasm-check --target wasm32-unknown-unknown --locked
node scripts/check-wasm.mjs # requires native M0/M1 traces as in README

sh scripts/build-web.sh
python3 -m http.server 8080 --bind 127.0.0.1 --directory web
# In separate terminals, isolated test-only browser profiles:
google-chrome-stable --headless=new --user-data-dir=/tmp/grazer-m2-webgl-browser --remote-debugging-port=9226 --use-angle=swiftshader --enable-unsafe-swiftshader about:blank
node scripts/check-game-browser.mjs webgl 9226
google-chrome-stable --headless=new --user-data-dir=/tmp/grazer-m2-webgpu-browser --remote-debugging-port=9227 --enable-unsafe-webgpu --use-webgpu-adapter=swiftshader --use-angle=vulkan --use-vulkan=swiftshader --enable-features=Vulkan --disable-vulkan-surface about:blank
node scripts/check-game-browser.mjs webgpu 9227

cargo package --allow-dirty --locked --offline
cargo check --manifest-path target/package/grazer-0.1.0-alpha.1/Cargo.toml --all-features --all-targets --locked --offline --target-dir target
```

Screenshots and host/trace/measurement outputs are generated under ignored
`target/`. Final 100,000 desktop / 30,000 browser drawing, total-frame ≤16.7 ms,
advanced patterns/lasers, device-loss recovery and saved gameplay replay/debug
work remain later-milestone validation.
