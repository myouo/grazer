use grazer::{
    Game, GameConfig, GameInput, Simulation, SimulationConfig,
    game::{DemoStage, GamePhase},
    language::{DiagnosticKind, Program, ScriptStage, Vm, VmLimits},
    resources::ResourcePack,
};
use std::sync::Arc;
fn machine(source: &str) -> (Vm, Simulation) {
    let program = Arc::new(Program::compile("test.gz", source).unwrap());
    let vm = Vm::new(program, VmLimits::default(), 42).unwrap();
    let world = Simulation::new(
        SimulationConfig {
            projectile_capacity: 128,
            enemy_capacity: 16,
            ..SimulationConfig::default()
        },
        42,
    )
    .unwrap();
    (vm, world)
}
fn advance(vm: &mut Vm, world: &mut Simulation) {
    vm.update(world).unwrap();
    world.step().unwrap();
    vm.prune_dead_owners(world);
}

#[test]
fn typed_functions_branches_loops_and_short_circuit() {
    let (vm, mut world) = machine(
        "fn twice(n: int) -> int { if n > 0 { return n * 2; } else { return 0; } } task main() { let n: int = 0; while n < 3 { n = n + 1; } if false && (1 / 0 == 0) { wave(99); } else { wave(twice(n)); } if true || (1 / 0 == 0) { wait(1); } complete(); }",
    );
    let mut vm = vm;
    assert_eq!(vm.update(&mut world).unwrap().wave, 6);
    assert!(!vm.status().complete);
    world.step().unwrap();
    assert!(vm.update(&mut world).unwrap().complete);
    assert_eq!(vm.task_count(), 0);
}
#[test]
fn fixed_constants_vectors_and_native_motion_are_integer_exact() {
    let (mut vm, mut world) = machine(
        "task main() { let n: fixed = 1.0 / 3.0; let p: vec = vec(100.0 + n, 200.0); let ship: entity = enemy(p, vec(0.0, 0.0), 3.0, 5, 30, 0xffffffff); boss(ship, 5); move(ship, vec(0.5, -0.25)); wait(1); }",
    );
    let status = vm.update(&mut world).unwrap();
    let ship = status.boss.unwrap();
    assert_eq!(
        world.enemy(ship).unwrap().position.x.bits(),
        (100 << 16) + 21845
    );
    world.step().unwrap();
    assert_eq!(
        world.enemy(ship).unwrap().position.x.bits(),
        (100 << 16) + 21845 + 32768
    );
    assert_eq!(
        world.enemy(ship).unwrap().position.y.bits(),
        (200 << 16) - 16384
    );
}
#[test]
fn malformed_source_types_arity_recursion_and_capabilities_are_rejected() {
    for source in [
        "task main( {",
        "task main() { let n: int = 1.0; }",
        "task main() { let n: int = 1; n = true; }",
        "task main() { wait(true); }",
        "task main() { vec(1.0); }",
        "fn f() -> int { } task main() {}",
        "fn f() { f(); } task main() { f(); }",
        "task main() { system_time(); }",
        "task main() { read_file(); }",
        "fn f() { wait(1); } task main() {}",
        "task child() {} task main() { child(); }",
        "task main() { let n: fixed = 32768.0; }",
        "task main() { let n: int = 2147483648; }",
        "task main() { unknown = 1; }",
        "task main() { let n = 1; let n = 2; }",
    ] {
        assert!(
            Program::compile("bad.gz", source).is_err(),
            "accepted {source}"
        );
    }
    let nested = format!(
        "task main() {{ let n = {}1{}; }}",
        "(".repeat(70),
        ")".repeat(70)
    );
    assert!(Program::compile("nested.gz", &nested).is_err());
    assert!(Program::compile("comment.gz", "/*").is_err());
}
#[test]
fn runtime_arithmetic_errors_have_source_function_and_task() {
    for (source, kind) in [
        (
            "task main() {\n let n: int = 1 / 0;\n}",
            DiagnosticKind::DivisionByZero,
        ),
        (
            "task main() {\n let n: int = 2147483647 + 1;\n}",
            DiagnosticKind::Overflow,
        ),
        (
            "task main() {\n let n: fixed = -32768.0 / -1.0;\n}",
            DiagnosticKind::Overflow,
        ),
        ("task main() {\n wait(0);\n}", DiagnosticKind::Argument),
    ] {
        let (mut vm, mut world) = machine(source);
        let error = vm.update(&mut world).unwrap_err();
        assert_eq!(error.kind, kind);
        assert_eq!(error.file, "test.gz");
        assert_eq!(error.line, 2);
        assert!(error.column > 1);
        assert_eq!(error.function, "main");
        assert!(error.task.is_some());
        let hash = vm.state_hash();
        assert_eq!(vm.update(&mut world).unwrap_err(), error);
        assert_eq!(vm.state_hash(), hash);
        let restored = Vm::restore(
            Arc::new(Program::compile("test.gz", source).unwrap()),
            &vm.save(),
        )
        .unwrap();
        assert_eq!(restored.state_hash(), hash);
        assert_eq!(restored.diagnostic(), Some(&error));
    }
}
#[test]
fn execution_command_birth_and_call_depth_budgets_pause() {
    for (source, mut limits, kind) in [
        (
            "task main() { while true { let n = 1; } }",
            VmLimits::default(),
            DiagnosticKind::InstructionBudget,
        ),
        (
            "task main() { while true { wave(1); } }",
            VmLimits {
                commands_per_tick: 2,
                ..VmLimits::default()
            },
            DiagnosticKind::CommandBudget,
        ),
        (
            "task child() { wait(1); } task main() { fork child(); fork child(); }",
            VmLimits {
                births_per_tick: 1,
                ..VmLimits::default()
            },
            DiagnosticKind::TaskBudget,
        ),
        (
            "fn a() { b(); } fn b() { c(); } fn c() {} task main() { a(); }",
            VmLimits {
                call_depth: 2,
                ..VmLimits::default()
            },
            DiagnosticKind::CallDepth,
        ),
    ] {
        limits.instructions_per_task = 64;
        let program = Arc::new(Program::compile("budget.gz", source).unwrap());
        let mut vm = Vm::new(program, limits, 0).unwrap();
        let mut world = Simulation::new(SimulationConfig::default(), 0).unwrap();
        let error = vm.update(&mut world).unwrap_err();
        assert_eq!(error.kind, kind);
        assert!(vm.last_instruction_count() <= limits.instructions_per_tick);
        assert!(error.to_string().contains("budget.gz:1:"));
    }
}
#[test]
fn fork_order_join_and_scope_cancellation_are_stable() {
    let (mut vm, mut world) = machine(
        "task child(n: int) { wave(n); wait(2); } task main() { let a: task = fork child(1); let b: task = fork child(2); join(a); wave(3); cancel(b); complete(); }",
    );
    assert_eq!(vm.update(&mut world).unwrap().wave, 2);
    assert_eq!(vm.task_count(), 3);
    world.step().unwrap();
    advance(&mut vm, &mut world);
    advance(&mut vm, &mut world);
    assert_eq!(vm.task_count(), 1);
    assert_eq!(vm.update(&mut world).unwrap().wave, 3);
    assert_eq!(vm.task_count(), 0);
    let (mut vm, mut world) =
        machine("task child() { wave(99); wait(1); } task main() { fork child(); return; }");
    assert_eq!(vm.update(&mut world).unwrap().wave, 0);
    assert_eq!(vm.task_count(), 0);
}
#[test]
fn parent_cancel_and_entity_owner_death_remove_whole_subtrees() {
    let (mut vm, mut world) = machine(
        "task leaf() { wait(50); } task worker(ship: entity) { attach(ship); fork leaf(); wait(50); } task main() { let ship = enemy(vec(30.0, 30.0), vec(0.0, 0.0), 2.0, 3, 1, 0xffffffff); fork worker(ship); wait(50); }",
    );
    vm.update(&mut world).unwrap();
    assert_eq!(vm.task_count(), 3);
    world.step().unwrap();
    vm.prune_dead_owners(&world);
    assert_eq!(vm.task_count(), 1);
    let root = vm.task_handles().next().unwrap();
    vm.cancel(root);
    assert_eq!(vm.task_count(), 0);
    vm.cancel(root);
    assert_eq!(vm.task_count(), 0);
}
#[test]
fn invalid_self_join_and_stale_entity_stop_with_diagnostics() {
    let (mut vm, mut world) = machine("task main() { join(current_task()); }");
    assert_eq!(
        vm.update(&mut world).unwrap_err().kind,
        DiagnosticKind::Task
    );
    let (mut vm, mut world) = machine(
        "task main() { let ship = enemy(vec(30.0, 30.0), vec(0.0, 0.0), 2.0, 3, 1, 0xffffffff); wait(1); move(ship, vec(0.0, 1.0)); }",
    );
    advance(&mut vm, &mut world);
    assert_eq!(
        vm.update(&mut world).unwrap_err().kind,
        DiagnosticKind::Host
    );
}
#[test]
fn bytecode_roundtrip_and_invalid_binary_are_rejected() {
    let program =
        Program::compile("test.gz", "task main() { wave(2); wait(3); complete(); }").unwrap();
    let bytes = program.to_bytes();
    let restored = Program::from_bytes(&bytes).unwrap();
    assert_eq!(program.content_hash(), restored.content_hash());
    for end in 0..bytes.len() {
        assert!(
            Program::from_bytes(&bytes[..end]).is_err(),
            "accepted prefix {end}"
        );
    }
    let mut corrupt = bytes.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(Program::from_bytes(&corrupt).is_err());
    let mut extra = bytes;
    extra.push(0);
    assert!(Program::from_bytes(&extra).is_err());
}
#[test]
fn serialized_rng_wait_locals_and_frames_resume_identically() {
    let source = "fn noise(n: int) -> int { return random() % n; } task child() { let n: int = 0; while n < 20 { wave(noise(1000)); n = n + 1; wait(3); } } task main() { let t = fork child(); join(t); complete(); }";
    let (mut a, mut world_a) = machine(source);
    for _ in 0..17 {
        advance(&mut a, &mut world_a);
    }
    let program = Arc::new(Program::from_bytes(&a.program().to_bytes()).unwrap());
    let mut b = Vm::restore(program, &a.save()).unwrap();
    let mut world_b = world_a.clone();
    assert_eq!(a.state_hash(), b.state_hash());
    for _ in 0..100 {
        advance(&mut a, &mut world_a);
        advance(&mut b, &mut world_b);
        assert_eq!(a.state_hash(), b.state_hash());
        assert_eq!(world_a.state_hash(), world_b.state_hash());
    }
    let bytes = a.save();
    for end in 0..bytes.len() {
        assert!(
            Vm::restore(
                Arc::new(Program::compile("test.gz", source).unwrap()),
                &bytes[..end]
            )
            .is_err()
        );
    }
    let wrong = Arc::new(Program::compile("wrong.gz", "task main() {}").unwrap());
    assert!(Vm::restore(wrong, &bytes).is_err());
}
#[test]
fn script_stage_errors_are_visible_to_game_and_restart_resets_vm() {
    let stage = ScriptStage::compile(
        "loop.gz",
        "task main() { while true {} }",
        VmLimits::default(),
        42,
    )
    .unwrap();
    let mut game =
        Game::with_stage(GameConfig::default(), 42, ResourcePack::builtin(), stage).unwrap();
    let initial = game.state_hash();
    let error = game.step(GameInput::default()).unwrap_err();
    assert!(error.to_string().contains("loop.gz:1:"));
    assert_eq!(game.phase(), GamePhase::Faulted);
    assert!(game.diagnostic().is_some());
    game.restart().unwrap();
    assert_eq!(initial, game.state_hash());
}
#[test]
fn migrated_stage_preserves_every_native_world_tick_hud_audio_and_sprite() {
    let mut config = GameConfig::default();
    config.simulation.projectile_capacity = 512;
    config.simulation.player.health = 10000;
    let mut native =
        Game::with_stage(config, 42, ResourcePack::builtin(), DemoStage::default()).unwrap();
    let mut scripted = Game::with_stage(
        config,
        42,
        ResourcePack::builtin(),
        ScriptStage::builtin(42).unwrap(),
    )
    .unwrap();
    for frame in 0..10800 {
        let input = GameInput {
            fire: true,
            ..GameInput::default()
        };
        native.step(input).unwrap();
        scripted.step(input).unwrap();
        assert_eq!(
            native.simulation().state_hash(),
            scripted.simulation().state_hash(),
            "world diverged at frame {frame}"
        );
        assert_eq!(native.hud(), scripted.hud());
        assert_eq!(native.audio_events(), scripted.audio_events());
        assert_eq!(
            native.sprites().collect::<Vec<_>>(),
            scripted.sprites().collect::<Vec<_>>()
        );
        if native.phase() == GamePhase::Cleared {
            break;
        }
    }
    assert_eq!(scripted.phase(), GamePhase::Cleared);
    assert_eq!(scripted.hud().tick, 10195);
}

#[test]
fn full_script_game_golden_trace() {
    assert_eq!(
        grazer::language::trace(100000).last().copied(),
        Some(0x291dc8eab41e48d0)
    );
}
