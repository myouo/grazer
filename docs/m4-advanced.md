# M4 advanced creation

The desktop and browser default to **Prism Passage**,
`assets/demo/advanced_showcase.graze`: 70 waves over seven minutes, then three
one-minute Boss phases. Ring/fan/aimed patterns form phase 1; spirals and straight
lasers form phase 2; composed motion and curved lasers form phase 3. Killing a
phase earns a bonus and clears its shots; surviving its timer also advances it.
The coordinator waits until each boundary, so victory commits at tick 36,001
(10 minutes + one tick). Normal play starts with three lives and three Bombs.

```sh
cargo run --release --example play --features desktop -- --difficulty normal
cargo run --release --example play --features desktop -- --script assets/examples/curve_laser.graze
cargo run --release --example advanced
cargo run --release --example script -- assets/demo/advanced_showcase.graze
cargo run --release --example script -- compile assets/demo/advanced_showcase.graze target/showcase.gzb
```

The browser has a difficulty selector. `?script=./examples/fan.graze` loads a
focused example; `?script=./first_sortie.graze` loads the historical M3 stage.
`--health 10000` and browser `?health=10000` are controlled validation options.

## Motion and emission

Coordinates, speeds, accelerations and angles are signed Q16.16. Angles are
**turns**, clockwise on downward-positive Y: 0 right, 0.25 down, 0.5 left,
0.75 up, 1 wraps to 0. A fixed 129-entry quarter-sine table with integer linear
interpolation supplies polar/rotation results. Cardinal directions are exact.
No float, platform trig, wall clock or unseeded RNG enters simulation.

| Builtin | Signature and behavior |
| --- | --- |
| `polar` | `(speed:fixed,angle:fixed) -> vec`; nonnegative speed |
| `ring` | `(origin:vec,count:int,speed:fixed,angle:fixed,radius:fixed,lifetime:int)`; full ring, no duplicate closing spoke |
| `fan` | `(origin,count,speed,angle,spread,radius,lifetime)`; centered fan including endpoints; a single bullet uses the center |
| `aimed` | `(origin:vec,target:vec,count:int,speed:fixed,spread:fixed,radius:fixed,lifetime:int)`; centered fan aimed with integer quadrant search |
| `spiral` | `(origin,count,speed,angle,step,radius,lifetime)`; successive spokes; advance `angle` between volleys for a rotating emitter |
| `compose` | `(entity,acceleration:vec,turn_per_tick:fixed)`; integrate current velocity, then add acceleration, then rotate next velocity |
| `colour` | `(entity,rgba:int)`; enemy/projectile tint, numeric `0xRRGGBBAA` |

Patterns create hostile bullets with damage 1 in stable spoke order. Counts are
1..512, radius positive, lifetime nonnegative (zero unlimited until culling).
Negative spread/step reverses ordering. Patterns preflight capacity/arguments
before inserting any projectile. Each emitted bullet consumes one VM command
budget unit: a ring of 20 costs 20. A resource/argument/budget fault is visible
and stops the Game without silently dropping bullets.

Composition supports enemies, regular projectiles and translating lasers. It
replaces the acceleration/turn controller; `move(entity,velocity)` still sets
base velocity. Position and next-velocity arithmetic is checked before a step
mutates input/events/tick. Collider shapes translate rigidly; turning velocity
does not rotate/deform local geometry. Rust equivalents are
`advanced::{Pattern,PatternShot,Motion,polar,rotate,angle_to}` and
`Simulation::{emit_pattern,compose_motion,colour}`.

## Lasers

| Builtin | Signature |
| --- | --- |
| `laser` | `(origin:vec,end_offset:vec,radius:fixed,warmup:int,active:int,fade:int) -> entity` |
| `curve_laser` | `(origin:vec,control_offset:vec,end_offset:vec,radius:fixed,warmup:int,active:int,fade:int) -> entity` |

Straight lasers are capsules. Curved lasers sample a quadratic Bezier into 16
integer points; collision is the union of 15 exact segment capsules with round
joins/endcaps. Control and endpoint are **offsets from origin**. Rust may supply
arbitrary 2..16-point `Collider::curve` shapes to `spawn_laser`.

`warmup >= 0`, `active > 0`, `fade >= 0`; their sum must fit `u32`. Warning and
fade are harmless; only active ticks collide/graze. Active beams remain alive
after a hit, including blocked damage during invulnerability. Each beam grazes
once per generation. Swept relative collision applies to moving beams and the
player. Expiry follows the final collision pass. A boundary snapshot phase
describes the upcoming interval: after W harmless steps, the next A steps are
active, followed by F harmless steps.

Lasers occupy the projectile pool; at most 64 may be live. Bombs/cancellation
clear all hostile lasers, including warning/fade. Rust uses `LaserTiming` and
`Simulation::spawn_laser`. `Game::laser_segments()` is separate from sprite
snapshots: handle/generation, segment index, phase (0 warning/1 active/2 fading),
endpoints, full collision width and tint. The shared renderer draws rotated
strips, round caps and active cores on desktop, WebGPU and WebGL2.

## Drops, score and difficulty

`drop(position:vec,kind:int,value:int) -> entity` creates a pickup;
`enemy_drop(enemy:entity,kind:int,value:int)` sets one death reward. Kind 0 is
points, 1 power, 2 Bomb. Values are positive. Only health-depleted enemies drop
rewards; timeout, bounds cleanup and explicit/stage despawn do not. Tagged
enemies reserve slots that manual drops cannot consume. Capacity failure occurs
at the command boundary before death commit. Drop handles are generational,
kind 3, in a separate bounded pool (default 512).

Drops fall 1.5 units/tick, attract at max-axis speed 8 within 64 units, and all
attract when the player enters the top quarter. Pickup uses swept contact with
radius 20 (drop 6 + pickup 14). Off-field drops and drops aged 600 ticks expire.
Rewards apply after collision; power affects subsequent shots. Power caps at 4;
twin-shot damage is `1 + power`. Bomb drops replenish stock to 9 and preserve a
higher custom starting stock.

`difficulty() -> int` reports 0 Easy, 1 Normal or 2 Hard. The showcase explicitly
scales count/speed/cadence and Boss HP; custom scripts choose how to use it.
Score multiplier M is difficulty + 1. Scores/counters saturate rather than wrap.

| Event | Score |
| --- | --- |
| Graze | `10 * M` |
| Enemy defeat | `100 * M` |
| Point pickup | `value * M` |
| Rewarded cancellation | `5 * cancelled_projectile_count * M` |
| Boss phase defeat | Additional `5000 * M` |

`cancel_shots(reward:bool) -> int` returns hostile projectile count, including
one per laser. It records cancellation metrics and optionally score; friendly
shots remain. Bombs give no cancel-score bonus. Legacy `clear_shots()` remains
silent cleanup without these metrics/rewards.

## Boss phases and extension mode

`boss(entity,max_health)` sets the shared Boss HUD. `phase(entity,index,duration)`
records positive index/duration and countdown; scripts schedule the actual
defeat/timeout transition. `despawn(entity)` removes a live actor/drop. Attached
shooters cancel immediately at owner death. Keep an encounter coordinator
unattached if it needs to observe defeat and issue rewards. Task return/cancel
removes its children.

M4 Games end on `complete()` instead of any Boss death, permitting another phase
or intermission. Player death wins over simultaneous completion. The shipped
coordinator uses fixed one-minute slots, cancels phase tasks, cleans remaining
entities/hostile shots, and completes after exactly three phases.

```rust
use grazer::{advanced::Difficulty, game::showcase};
let game = showcase::game(Difficulty::Normal, 0).unwrap();
assert_eq!(game.protocol_version(), 4);
let hud = game.advanced_hud().unwrap();
// difficulty, power, drops, phase/timer, collected/cancelled, phase bonus
```

`ScriptStage::showcase(seed)` loads the source. Any new builtin makes a program
declare `Stage::advanced_config`, so `Game::with_stage` preallocates M4 storage.
`Game::with_advanced_stage` overrides difficulty/drop capacity. Direct
`Simulation`/`Vm` hosts must call `enable_advanced` before the first spawn/tick.
Native stages declare the same config on `Stage`.
`Game::presentation_capacity()` gives a safe renderer preallocation bound.

## Examples and compatibility

| Example under `assets/examples/` | Ability |
| --- | --- |
| `ring.graze`, `fan.graze`, `aimed.graze`, `spiral.graze` | Four reusable emission patterns |
| `motion.graze` | Polar velocity + acceleration + turning + tint |
| `straight_laser.graze`, `curve_laser.graze` | Warning/active/fade and both beam shapes |
| `boss_phases.graze` | Three defeat/timeout phases and explicit completion |
| `drops_score.graze` | Point/power/Bomb pickup, death reward, difficulty, cancel score |

`Game::new`, M2 C creation and `ScriptStage::builtin` keep First Sortie. M4
Game/Simulation use protocol 4; opt-out worlds retain Game/Simulation protocols
3/2. New builtin IDs append at 27..41. Programs using them have bytecode/VM
protocol 2; old programs still emit/load bytecode 1 and retain VM protocol 1 and
fingerprints. VM snapshot structure remains format 1 with a program-specific
protocol header. Restore a matching world checkpoint along with VM bytes; full
serialized Game replay/debug tools remain M5 work.

Game ABI v2 and existing HUD/sprite structs keep their layouts. Additive M4 API
v1 adds `grazer_game_create_advanced`, `grazer_game_advanced_hud` (48 bytes) and
`grazer_game_lasers` (40-byte segment). Loading and no-partial-write bulk queries
match M3. Null+zero source selects Prism Passage; difficulty is explicit.
Browser exports `create_with_difficulty`, a 64-bit advanced HUD and bulk beam
geometry. Rendering/audio never feed back into physics.
