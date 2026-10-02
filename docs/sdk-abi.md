# SDK and ABI baseline

The proposed 0.1.0 baseline covers dependency-free simulation/Game/stage language,
resources, project/checkpoint/replay formats and the C host interfaces. It is a
release candidate until the M6 platform and performance gates pass. The registry
currently has only the M0 `0.1.0-alpha.1`; this repository does not imply that a
new version has been published.

For the 0.1.x release line, compatible additions use new functions or separately
versioned interfaces. Existing C POD layouts/function signatures, protocol
semantics and serialized formats will not be silently reinterpreted. A deliberate
incompatible gameplay change uses a new protocol; incompatible file formats are
rejected explicitly. Rust public APIs are subject to normal pre-1.0 semver rules.
The optional graphics/window/audio layer depends on wgpu/winit/CPAL and its
rendered pixels/timing/device availability are not authoritative gameplay state.

| Family | Version | Scope |
| --- | ---: | --- |
| Runtime | 1 | Historical M0 wrapping fixture |
| Simulation | 2 / 4 | Legacy / advanced checked physics |
| Game | 3 / 4 | Historical / advanced gameplay |
| Language bytecode / VM | 1 / 2 | Original / added M4 builtins |
| VM state | 1 | Register/task continuation, bound to program protocol |
| Resources | 1 | Atlas, sprite IDs, tone definitions and digest |
| Project | 1 | `.grazer` self-contained Game configuration/program/resources |
| Checkpoints / Game replay | 1 / 1 | Complete saved state / input/control history |
| C motion / Game ABI | 1 / 2 | Original and playable opaque handles |
| C script/advanced/checkpoint/project API | 1 each | Additive interface families |

Protocols are independent from Cargo versions, C ABI and resource/archive
versions. Same protocol, program, resources, config/seed and input/state give the
same native/WASM hashes. A resource digest binds actual atlas pixels and all
sprite/tone values. Fingerprints/checksums detect differences, not authenticity.

## Rust ownership and hosts

Use `Simulation` for headless physics or `Game<Stage>` for complete gameplay.
Hosts own the fixed 60Hz clock and input collection. Presentation consumes
snapshots after a successful tick; audio events are processed once per tick.
Never feed render floats, wall-clock time or device behavior into authoritative
commands. `FrameClock` retains overload backlog and discards paused wall time.

`Stage` implementations hash every persistent field and provide stable content
identity. `CheckpointStage` additionally encodes/restores all state for replay.
Scripted projects use verified bytecode and bounded VM/task/command budgets. A
fault pauses the Game and is visible; commands executed before it are preserved
until reset. Existing protocols/golden fixtures are retained in CI.

The default library has no dependencies. `ffi`, `resources`, `graphics`, `desktop`
and `web` opt into their documented capabilities. Core successful ticks and
playback steps reuse reserved buffers; construction, compile/load, snapshot,
seek/reload and explicit inspection may allocate. Renderer buffers are reserved
and grow only when a new debug/content capacity requires it.

## C layout and lifetime

`include/grazer.h` is the source of C declarations. Use default platform C
alignment, IEEE-754 32-bit float and fixed-width integer types; never pack structs.
Use the family version functions and pass `struct_size` where specified. The
library exposes opaque owned handles rather than Rust containers/GPU objects.

| POD | Bytes on supported 64-bit desktop/WASM ABI | Selected offset |
| --- | ---: | --- |
| GrazerConfig | 32 | seed 24 |
| GrazerGameConfig | 24 | seed 8 |
| GrazerGameSprite | 40 | x 16, rgba 32 |
| GrazerAudioEvent | 16 | tick 8 |
| GrazerHud | 64 | health 24, bomb_flash 56 |
| GrazerAdvancedHud | 48 | collected 24 |
| GrazerLaserSegment | 40 | x1 16 |
| GrazerResourceInfo | 32 | content_hash 24 |
| GrazerSpriteAsset / GrazerSoundAsset | 20 each | fixed-width fields |
| GrazerScriptDiagnostic | 32 | eight u32 fields |
| GrazerReplayStatus | 48 | expected 32, actual 40 |

Layout assertions run in Rust and C on the platform matrix. All calls on a handle
are serialized. Callers supply valid aligned readable/writable lengths and
nonoverlapping buffers. Arbitrary/dangling pointers cannot be validated. Destroy
successful owned handles exactly once; null destroy is accepted. Input buffers
are borrowed for the call; successful outputs are owned by the caller. No host
callbacks occur. After a caught panic, destroy the handle. Allocation failure can
abort the process.

Bulk getters support count/query and reject an undersized buffer without partial
writes. Floats in snapshots are presentation only; coordinates/velocities in
authoritative inputs are signed Q16.16. Source/bytecode/project loading and
checkpoint/replay APIs distinguish invalid data, unsupported identity/version and
divergence. See the header and `c_host.c`, `c_game.c`, `c_replay.c`, `c_project.c`.

## Device and window lifecycle

Zero-size/minimized or unfocused surfaces pause tick scheduling; resizing updates
presentation without touching simulation. Surface Lost/Outdated is reconfigured
and retried; timeouts skip a draw. Device loss is distinct: recreate the renderer
against the same Game/resources, reset the host clock and remain paused for
review. The browser exposes Recover graphics; the desktop recreates on detected
loss. Shader/pipeline validation finishes before a renderer is exposed.

Audio can be unavailable and reports its error while gameplay remains usable.
Browsers require a user gesture to unlock audio. Pause/seek/fast-forward do not
alter recorded authoritative events. Hosts suppress intermediate playback sounds
and avoid resubmitting stale end/reset events.
