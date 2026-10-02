# M1 headless core

`grazer::Simulation` is the protocol-2 core in the repository. It runs at 60
ticks/second when driven by a host. It never reads a clock or OS RNG, and the
default crate has no third-party dependencies. `Runtime` and the existing
C/desktop/browser examples continue to exercise the published M0 protocol 1.
M2 will integrate gameplay with presentation, resources and host interfaces.

## Entities and host boundaries

Construct a world with `Simulation::new(SimulationConfig, seed)`. Configuration
sets a positive playfield, projectile/enemy capacities (1..=1,000,000 each), and
player position, hit radius, outer graze radius, per-axis speed, health and
invulnerability ticks. Spawn centers must lie in `[0, width) × [0, height)`.
Enemy health and all collider radii must be positive. Zero damage is allowed.

`spawn_projectile` and `spawn_enemy` return `EntityHandle` values containing
entity kind, slot and generation. `despawn` accepts live projectile/enemy
handles. `projectile` and `enemy` return `None` for stale or wrong-kind handles.
Slot reuse increments the generation; exhausted generations retire permanently.
Handles belong to their world and its clones. The player has the persistent
`EntityHandle::PLAYER`; death keeps its snapshot and disables movement/contacts.

Commands execute immediately between ticks; the host can inspect returned
handles before the next step. They are visible to that next step. A rejected
command leaves state unchanged. Automatic destruction commits after the final
collision pass. A rejected step preserves input, events and all entities.

Positions, offsets, radii and velocities use checked Q16.16 `Fixed`; velocities
are units per tick. `Vec2::checked_add` checks both components. Player input
axes are persistent integers in `{-1,0,1}`, with intentionally unnormalized
diagonals and center clamping to the half-open field. `step_with_input` replaces
input and advances one tick atomically; `set_input` plus `step` is also supported.
Entity movement overflow stops the step with `ArithmeticOverflow`.

## Collision and settlement

Create `Collider::circle(radius)`, `Collider::capsule(start, end, radius)` or
`Collider::curve(points, radius)`. Offsets are local to the projectile center;
curves contain 2..=16 points and are the union of thick segment capsules with
round joins/end caps. Degenerate segments are supported. These are rigid
translated shapes, with circle players/enemies. Rotation, deformation and
persistent laser damage are later motion/gameplay features.

Collision includes exact tangency and both tick endpoints. Relative linear
motion is swept continuously: fast bullets and moving actors can collide even
when both endpoint snapshots are disjoint. Bounds reject distant candidates;
narrow-phase tests use integer dot/cross products and exact 256-bit product
comparisons. No floats, sampled paths or tolerance values enter simulation.

Each successful step follows this protocol:

1. Check tick exhaustion and every entity's proposed checked movement.
2. Move the living player, then projectiles and enemies; retain previous positions.
3. Gather projectile contacts in spawn order. Hostile projectiles check player
   hit before graze. Friendly projectiles select the first contacted enemy in
   **spawn order**, regardless of geometric time of impact. Then gather enemy
   body contacts in enemy spawn order. Detection uses the pre-damage actor set.
4. Resolve contacts in that order. A projectile hit consumes the projectile,
   including hits against invulnerable actors or simultaneous enemy kills.
   Damage saturates at remaining health; such blocked hits report damage zero.
   An enemy killed this tick can still contribute its previously gathered body
   contact. Invulnerability can suppress that damage.
5. Emit destruction events in projectile spawn order, then enemy spawn order,
   and compact both pools without changing survivor order. Removal reasons
   prefer hit/health depletion, then lifetime, then out-of-bounds. Increment tick.

A successful hit on tick T with N invulnerability ticks blocks damage during
the rest of T and ticks T+1 through T+N when N>0. With N=0, each contact can
damage. `PlayerDied` is emitted once. A hostile projectile can graze once per
generation when its swept path reaches the outer graze radius without reaching
the hit radius. Hit takes precedence over graze, even while invulnerable.
Grazing remains possible while invulnerable, stops after death, and its total
saturates at `u64::MAX`.

`lifetime=0` is unlimited; a positive lifetime includes the final collision pass.
`BoundsBehavior::Despawn` removes a whole shape after it leaves the field;
`Keep` disables culling and still uses checked motion. There is no wrapping.
Explicit host `despawn` is a boundary command; `events()` reports automatic
tick results only and is replaced by the next successful step.

## Snapshots, hashing and checkpoints

`snapshots()` yields player, spawn-ordered enemies, then spawn-ordered
projectiles. Each owned `EntitySnapshot` contains a handle, current/previous
fixed-point position, collider, optional health and RGBA. Reuse a host buffer
with `clear` and `extend`; presentation can convert coordinates to floats.

State hashes use FNV-1a over fixed-width little-endian protocol, configuration,
tick, SplitMix64 state, pending input, player state, pool generations/free-list
order/dense indices, and all ordered entity data. Previous positions, remaining
lifetimes and already-grazed flags are included. Cached geometry and emitted
events/contact scratch are derived and excluded. Hashes diagnose divergence;
they are not cryptographic integrity checks.

`Simulation::clone` preserves allocation capacities and future handle behavior.
World creation and cloning allocate; accepted spawns within capacity, automatic
compaction, step, state hashing and snapshot iteration reuse reserved memory.
Friendly projectile collision currently checks enemies in order with per-pair
bounds rejection; its worst-case cost grows with projectiles × enemies. The
100,000-bullet baseline measures hostile projectiles against one player.

## Minimal input/command replay

```rust
use grazer::{Input, SimulationConfig};
use grazer::simulation::{Command, InputReplay, ReplayRecorder};

let mut recording = ReplayRecorder::new(SimulationConfig::default(), 42).unwrap();
let random = recording.command(Command::RandomU32).unwrap();
recording.step(Input { x: 1, y: 0 }).unwrap();
let expected = recording.simulation().state_hash();
let bytes = recording.finish().unwrap().to_bytes();
let replay = InputReplay::from_bytes(&bytes).unwrap();
assert_eq!(replay.play().unwrap().state_hash(), expected);
```

Use `ReplayRecorder::command` for spawn, despawn and every host RNG draw.
Accepted commands are recorded in call order and returned handles can be used
in subsequent commands. `step(input)` records one frame and its resulting hash.
Rejected commands/steps are not logged. Finish only after a step has recorded
all pending commands. `ReplayPlayer::step` reuses world buffers, verifies every
frame, and returns false at EOF. On a command error or hash divergence it stops
permanently; the failing frame may have partially applied commands.

Binary format 1 starts with `GRZREP01`, replay version, simulation protocol,
configuration, seed and frame count. All multibyte integers are little-endian.
Frames store axes, command count, tagged command data and a u64 state hash.
Decoding rejects unsupported versions, invalid shapes/values, truncation,
trailing bytes and declared counts inconsistent with the input. Limits are
1,000,000 frames and 1,000,000 commands per frame. Recording/serialization can
allocate. This experimental format has no resource metadata or serialized
checkpoints; M5 adds the full replay/debug workflow.

See [M1 validation](m1-validation.md) for reproduction and measured coverage.
