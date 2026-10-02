# M3 language and VM

M3 adds a dependency-free compiler/VM under `grazer::language`. The playable
desktop and web runners now use [First Sortie source](../assets/demo/first_sortie.graze).
The M2 native `DemoStage`, its C constructor and old traces remain references.
The scripted stage reproduces each native world's state, HUD, audio and sprites;
its Game hash additionally binds the program and VM/task state.

## Authoring and loading

```sh
cargo run --release --example script
cargo run --release --example script -- compile assets/demo/first_sortie.graze target/first_sortie.gzb
cargo run --release --example play --features desktop -- --script assets/demo/first_sortie.graze
cargo run --release --example play --features desktop -- --script target/first_sortie.gzb
sh scripts/build-web.sh
python3 -m http.server 8080 --bind 127.0.0.1 --directory web
```

The browser loads `first_sortie.graze` beside the project manifest. Override it
with `?script=./other_stage.graze`; script and assets must use the page origin.
Compilation errors display file/line/column. Runtime errors leave the simulation
faulted at that tick and display the function/task and source location; desktop
keeps its window with the error and allows restart. A native Game stage callback
has no filesystem/network/clock interface exposed through this language.

```text
fn shot(p: vec, v: vec) {
    emit(p, v, 3.0, 1, 420);
}
task gun(ship: entity) {
    attach(ship);
    while alive(ship) {
        shot(position(ship), vec(0.0, 2.5));
        wait(60);
    }
}
task main() {
    let ship: entity = enemy(vec(240.0, 20.0), vec(0.0, 1.25),
        12.0, 12, 440, 0xff9292ff);
    let gun_task: task = fork gun(ship);
    wait(120);
    cancel(gun_task);
    complete();
}
```

Syntax uses ASCII identifiers, `//`/non-nested `/* */` comments, braces and
semicolon-terminated statements. Types are `int`, `bool`, `fixed`, `vec`, `entity`,
`task`, and function-only `unit`. `let name[: type] = expression` initializes a
mutable local; annotations are optional. Assignments preserve type. Block scope
can shadow outer locals. `if condition { } else { }`, `while condition { }` and
`return [expression];` are supported. Ordinary `fn` can return an annotated type;
`task` returns unit and `task main()` is the entrypoint. No global mutable storage,
arrays/heap objects, strings, modules, arbitrary recursion, break/continue or I/O
capabilities are provided at this milestone.

Decimal integers are checked signed i32, including `-2147483648`. Decimal fixed
literals contain a dot or `f` suffix, with up to nine fractional decimal digits;
they convert with integer arithmetic, truncating toward zero to Q16.16. Hex
integer literals carry raw u32 bits, useful for RGBA (e.g. `0xff9292ff`). Numeric
overflow and division by zero stop execution. `+ - * / %`, comparisons, unary
`- !`, and short-circuit `&& ||` have static types. Vectors support addition/
subtraction/negation and multiplication/division by fixed scalars. There is no
implicit int/fixed conversion; use `fixed(int)` and `int(fixed)` explicitly.

## Builtins

| Operation | Signature/result |
| --- | --- |
| `vec`, `x`, `y` | `vec(fixed,fixed) -> vec`; component accessors return fixed |
| `fixed`, `int` | Explicit checked integer/fixed conversion |
| `width`, `height`, `player` | Field extents as fixed; player position as vec |
| `tick`, `random` | World tick as checked int; seeded nonnegative SplitMix64 int |
| `aim` | `(from:vec,to:vec,speed:fixed) -> vec`, integer max-axis normalization |
| `enemy` | `(position,velocity,radius,hp,lifetime,rgba) -> entity` |
| `emit` | `(position,velocity,radius,damage,lifetime) -> hostile entity` |
| `alive`, `position` | Safe lifetime query; position rejects stale handles |
| `move` | `(entity,velocity)` updates native batched motion |
| `clear_enemies`, `clear_shots` | Boundary cleanup; shots means hostile shots |
| `wave`, `boss`, `complete` | Stage/HUD status; `boss(entity,max_health)` |
| `wait`, `join` | Positive tick wait; wait for task termination |
| `cancel`, `cancel_children` | Idempotent task/subtree cancellation |
| `attach`, `current_task` | Bind task subtree to entity lifetime; current token |

World/entity commands validate bounds, positive collider/health and capacities.
Enemy lifetime zero means Keep/unlimited (the Boss); positive lifetime uses normal
culling. Hostile emission uses the demo bullet resource/color, and player shots
remain Game controls. The advanced pattern/resource/laser API is M4 work.

Functions run synchronously on a bounded call stack; tasks can fork only tasks
and functions can call only functions. Functions cannot wait/fork/control tasks.
Every fork copies its parameters and records a generational parent token. Tasks
run in creation order; new children run later in that same VM update. `wait(N)`
wakes at T+N. A join whose child terminates later in the update resumes on the
next tick. Self/ancestor joins fail explicitly. Task return cancels descendants.
`attach(entity)` cancels the task subtree immediately when the entity disappears,
including after the Game's collision commit. Dead-owner tasks do not fire again.
Slot reuse increments generation; exhausted generations retire.

## Budgets, bytecode and snapshots

Default limits: 32 task slots, 8 call frames/task, 16,384 instructions/tick,
4,096 instructions/task/tick, 512 host commands/tick and 16 births/tick. Limits
are configurable within bounded ceilings (256 tasks, 16 frames, 1,000,000 tick
instructions, 8,192 commands). Function registers and parameters are capped at
64 and 8. Source is at most 1 MiB/65,536 tokens, syntax nesting at most 64,
functions at most 128 and total instructions at most 65,536.

All budget exhaustion is a persistent, source-localized fault; no instruction,
task or command is silently dropped. Some commands can have executed before a
fault; the Game stops before its simulation step and requires restart/reload.
The shipped VM reserves tasks/order/free-list/call-frame storage at construction.
Normal execution, spawning, waiting, cancellation and hashing allocate zero times.
Compilation, loading, cloning, faults and save/restore may allocate.

Bytecode format 1 (`GZCODE01`) encodes source/mappings, typed function/register
metadata, calls/jumps/constants and a content fingerprint. Loading checks sizes,
UTF-8/type/opcode tags, spans, arity/kinds, targets, register initialization on
every reachable CFG path, returns and recursion. Malformed/incompatible/corrupt
input is rejected before it executes. Source filename is diagnostic metadata and
does not change the program fingerprint; source text/mappings/code are bound.

VM protocol/state format 1 (`GZVMST01`) saves program identity, limits, RNG,
last tick/status, slot generations, parent/owner/wait/join state, all active call
frames/registers/PCs, order/free list and fault state. Restore verifies program,
versions, handles, frame/register types, call continuations, ownership structure,
ordering and fingerprint. Restore the matching `Simulation` checkpoint too;
VM updates require the next matching world tick. Full disk-serialized Game/world
checkpoints and replay metadata remain M5 scope. FNV fingerprints diagnose
divergence and are not security signatures.

```rust
use grazer::language::{Program, Vm, VmLimits};
use std::sync::Arc;
let program = Arc::new(Program::compile("stage.graze", "task main() { wait(1); complete(); }").unwrap());
let vm = Vm::new(program.clone(), VmLimits::default(), 42).unwrap();
let bytes = vm.save();
let restored = Vm::restore(program, &bytes).unwrap();
assert_eq!(vm.state_hash(), restored.state_hash());
```

## C and validation

Script API 1 is additive to game ABI v2: `grazer_game_create_script` accepts
source (format 0), checked bytecode (format 1), or null/zero for the shipped stage.
It returns the same opaque Game handle and sprite/HUD/audio/asset outputs.
Compilation returns `GRAZER_SCRIPT_ERROR` with location fields; runtime faults
use `GRAZER_RUNTIME_ERROR` and the diagnostic/info text queries. The legacy
`grazer_game_create` still selects M2 native content. All usual buffer/ownership/
alignment rules apply; undersized outputs never partially write.

```sh
cargo run --release --example trace --locked -- 100000 m3 > target/native-script-trace.txt
cargo run --release --example script --locked -- restore-check > target/native-vm-restore.txt
target/c_game target/native-script-trace.txt script # compile as in M2 docs
cargo build --release -p grazer-wasm-check --target wasm32-unknown-unknown --locked
node scripts/check-wasm.mjs # generate historical M0/M1/M2 traces first
```

See [M3 validation](m3-validation.md) for actual host execution and limitations.
