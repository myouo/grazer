use grazer::{
    BoundsBehavior, Collider, Enemy, EntityKind, Event, Faction, Fixed, Game, GameConfig,
    GameInput, GamePhase, Input, Projectile, Simulation, SimulationConfig, SimulationError, Vec2,
    advanced::{
        self, AdvancedConfig, Difficulty, DropKind, DropReward, LaserPhase, LaserTiming, Motion,
        Pattern, PatternShot,
    },
    language::{DiagnosticKind, Program, ScriptStage, Vm, VmLimits},
    resources::ResourcePack,
};
use std::sync::Arc;
fn f(n: i32) -> Fixed {
    Fixed::from_int(n).unwrap()
}
fn v(x: i32, y: i32) -> Vec2 {
    Vec2::new(f(x), f(y))
}
fn world(capacity: u32, drops: u32) -> Simulation {
    let mut config = SimulationConfig {
        projectile_capacity: capacity,
        enemy_capacity: 8,
        ..SimulationConfig::default()
    };
    config.player.position = v(320, 400);
    config.player.health = 100;
    config.player.invulnerability_ticks = 1;
    let mut world = Simulation::new(config, 42).unwrap();
    world
        .enable_advanced(AdvancedConfig {
            drop_capacity: drops,
            ..AdvancedConfig::default()
        })
        .unwrap();
    world
}
fn shot() -> PatternShot {
    PatternShot {
        origin: v(320, 100),
        count: 4,
        speed: f(2),
        radius: f(3),
        lifetime: 60,
        rgba: 0xffffffff,
    }
}
fn enemy() -> Enemy {
    Enemy {
        position: v(320, 100),
        velocity: Vec2::ZERO,
        radius: f(12),
        health: 1,
        contact_damage: 0,
        lifetime: 0,
        bounds: BoundsBehavior::Keep,
        rgba: 0xffffffff,
    }
}
fn game(source: &str, difficulty: Difficulty) -> Game<ScriptStage> {
    let mut config = GameConfig::default();
    config.simulation.projectile_capacity = 1024;
    config.simulation.player.health = 10000;
    Game::with_advanced_stage(
        config,
        42,
        ResourcePack::builtin(),
        ScriptStage::compile("test.graze", source, VmLimits::default(), 42).unwrap(),
        AdvancedConfig {
            difficulty,
            ..AdvancedConfig::default()
        },
    )
    .unwrap()
}

#[test]
fn integer_angles_have_exact_cardinals_and_wrap_in_all_quadrants() {
    for (bits, expected) in [
        (0, v(2, 0)),
        (16384, v(0, 2)),
        (32768, v(-2, 0)),
        (49152, v(0, -2)),
        (-16384, v(0, -2)),
        (65536, v(2, 0)),
    ] {
        assert_eq!(
            advanced::polar(f(2), Fixed::from_bits(bits)).unwrap(),
            expected
        );
        assert_eq!(
            advanced::rotate(v(2, 0), Fixed::from_bits(bits)).unwrap(),
            expected
        );
    }
    for (target, expected) in [
        (v(1, 0), 0),
        (v(0, 1), 16384),
        (v(-1, 0), 32768),
        (v(0, -1), 49152),
    ] {
        assert_eq!(advanced::angle_to(Vec2::ZERO, target).bits(), expected);
    }
    for angle in (0..65536).step_by(127) {
        let direction = advanced::polar(Fixed::ONE, Fixed::from_bits(angle)).unwrap();
        let recovered = advanced::angle_to(Vec2::ZERO, direction).bits();
        let distance = (recovered - angle)
            .rem_euclid(65536)
            .min((angle - recovered).rem_euclid(65536));
        assert!(distance <= 2, "angle {angle} recovered {recovered}");
    }
    assert_eq!(
        advanced::polar(Fixed::from_bits(i32::MAX), Fixed::ZERO)
            .unwrap()
            .x
            .bits(),
        i32::MAX
    );
    assert!(advanced::polar(f(-1), Fixed::ZERO).is_err());
    let min = Vec2::new(Fixed::from_bits(i32::MIN), Fixed::from_bits(i32::MIN));
    let max = Vec2::new(Fixed::from_bits(i32::MAX), Fixed::from_bits(i32::MAX));
    assert!((advanced::angle_to(min, max).bits() - 8192).abs() <= 1);
}

#[test]
fn ring_fan_aimed_spiral_order_and_capacity_failure_are_deterministic() {
    let mut w = world(4, 8);
    w.emit_pattern(Pattern::Ring { angle: Fixed::ZERO }, shot())
        .unwrap();
    let velocities: Vec<_> = w
        .snapshots()
        .filter(|s| s.handle.kind() == EntityKind::Projectile)
        .map(|s| w.projectile(s.handle).unwrap().velocity)
        .collect();
    assert_eq!(velocities, vec![v(2, 0), v(0, 2), v(-2, 0), v(0, -2)]);
    let hash = w.state_hash();
    assert_eq!(
        w.emit_pattern(Pattern::Ring { angle: Fixed::ZERO }, shot()),
        Err(SimulationError::Capacity)
    );
    assert_eq!(w.state_hash(), hash);
    w.cancel_shots(false).unwrap();
    let mut s = shot();
    s.count = 3;
    w.emit_pattern(
        Pattern::Fan {
            angle: Fixed::from_bits(16384),
            spread: Fixed::from_bits(16384),
        },
        s,
    )
    .unwrap();
    let velocities: Vec<_> = w
        .snapshots()
        .filter(|s| s.handle.kind() == EntityKind::Projectile)
        .map(|s| w.projectile(s.handle).unwrap().velocity)
        .collect();
    assert!(velocities[0].x.bits() > 0);
    assert_eq!(velocities[1], v(0, 2));
    assert!(velocities[2].x.bits() < 0);
    w.cancel_shots(false).unwrap();
    s.count = 1;
    w.emit_pattern(
        Pattern::Aimed {
            target: v(320, 400),
            spread: f(1),
        },
        s,
    )
    .unwrap();
    let h = w
        .snapshots()
        .find(|s| s.handle.kind() == EntityKind::Projectile)
        .unwrap()
        .handle;
    assert_eq!(w.projectile(h).unwrap().velocity, v(0, 2));
    w.cancel_shots(false).unwrap();
    w.emit_pattern(
        Pattern::Spiral {
            angle: Fixed::ZERO,
            step: Fixed::from_bits(16384),
        },
        shot(),
    )
    .unwrap();
    let velocities: Vec<_> = w
        .snapshots()
        .filter(|s| s.handle.kind() == EntityKind::Projectile)
        .map(|s| w.projectile(s.handle).unwrap().velocity)
        .collect();
    assert_eq!(velocities, vec![v(2, 0), v(0, 2), v(-2, 0), v(0, -2)]);
    w.cancel_shots(false).unwrap();
    s.count = 0;
    let hash = w.state_hash();
    assert!(
        w.emit_pattern(Pattern::Ring { angle: Fixed::ZERO }, s)
            .is_err()
    );
    assert_eq!(w.state_hash(), hash);
}

#[test]
fn composed_motion_integrates_then_accelerates_then_turns_and_overflow_is_atomic() {
    let mut w = world(4, 8);
    let h = w
        .spawn_enemy(Enemy {
            velocity: v(1, 0),
            ..enemy()
        })
        .unwrap();
    w.compose_motion(
        h,
        Motion {
            acceleration: v(1, 0),
            turn_per_tick: Fixed::from_bits(16384),
        },
    )
    .unwrap();
    w.step().unwrap();
    assert_eq!(w.enemy(h).unwrap().position, v(321, 100));
    assert_eq!(w.enemy(h).unwrap().velocity, v(0, 2));
    w.step().unwrap();
    assert_eq!(w.enemy(h).unwrap().position, v(321, 102));
    assert_eq!(w.enemy(h).unwrap().velocity, v(-2, 1));
    w.compose_motion(
        h,
        Motion {
            acceleration: Vec2::new(Fixed::from_bits(i32::MIN), Fixed::ZERO),
            turn_per_tick: Fixed::ZERO,
        },
    )
    .unwrap();
    let hash = w.state_hash();
    let events = w.events().to_vec();
    assert_eq!(
        w.step_with_input(Input { x: 1, y: 0 }),
        Err(SimulationError::ArithmeticOverflow)
    );
    assert_eq!(w.state_hash(), hash);
    assert_eq!(w.events(), events);
}

#[test]
fn beam_warning_and_fade_are_harmless_active_damage_persists_through_immunity() {
    let mut w = world(8, 8);
    let h = w
        .spawn_laser(
            v(320, 100),
            Vec2::ZERO,
            Collider::capsule(Vec2::ZERO, v(0, 320), f(4)).unwrap(),
            LaserTiming {
                warmup: 2,
                active: 4,
                fade: 2,
            },
            0x65dfffff,
        )
        .unwrap();
    for _ in 0..2 {
        w.step().unwrap();
        assert_eq!(w.player().health, 100);
        assert!(w.projectile(h).is_some());
    }
    assert_eq!(w.laser_phase(h), Some(LaserPhase::Active));
    for health in [99, 99, 98, 98] {
        w.step().unwrap();
        assert_eq!(w.player().health, health);
        assert!(w.projectile(h).is_some());
    }
    assert_eq!(w.laser_phase(h), Some(LaserPhase::Fading));
    w.step().unwrap();
    assert_eq!(w.player().health, 98);
    w.step().unwrap();
    assert_eq!(w.player().health, 98);
    assert!(w.projectile(h).is_none());
    assert_eq!(w.projectile_count(), 0);
}

#[test]
fn beam_graze_is_once_per_generation_curve_matches_its_polyline_and_bulk_segments() {
    let mut w = world(8, 8);
    let h = w
        .spawn_laser(
            v(330, 100),
            Vec2::ZERO,
            Collider::capsule(Vec2::ZERO, v(0, 320), f(3)).unwrap(),
            LaserTiming {
                warmup: 0,
                active: 4,
                fade: 0,
            },
            0xffffffff,
        )
        .unwrap();
    for _ in 0..4 {
        w.step().unwrap();
    }
    assert_eq!(w.player().grazes, 1);
    assert!(w.projectile(h).is_none());
    let collider = advanced::bezier_collider(v(100, 100), v(0, 200), f(3)).unwrap();
    let segments: Vec<_> = collider.segments().collect();
    assert_eq!(segments.len(), 15);
    assert_eq!(segments[0].0, Vec2::ZERO);
    assert_eq!(segments[14].1, v(0, 200));
    assert!(collider.intersects_circle(Vec2::ZERO, segments[7].0, f(1)));
    assert!(!collider.intersects_circle(Vec2::ZERO, v(0, 100), f(1)));
    let h = w
        .spawn_laser(
            v(100, 100),
            v(1, 0),
            collider,
            LaserTiming {
                warmup: 0,
                active: 30,
                fade: 0,
            },
            0xffffffff,
        )
        .unwrap();
    assert_eq!(w.laser_segments().count(), 15);
    w.step().unwrap();
    let first = w.laser_segments().next().unwrap();
    assert_eq!((first.x1, first.y1, first.phase), (101.0, 100.0, 1));
    let checkpoint = w.clone();
    w.step().unwrap();
    let mut restored = checkpoint;
    restored.step().unwrap();
    assert_eq!(w.state_hash(), restored.state_hash());
    assert_eq!(w.cancel_shots(true).unwrap(), 1);
    assert!(w.projectile(h).is_none());
    assert_eq!(w.advanced_metrics().unwrap().cancelled, 1);
}

#[test]
fn drop_capacity_reserves_enemy_death_rewards_and_pickups_use_swept_contact() {
    let mut w = world(4, 1);
    let h = w.spawn_enemy(enemy()).unwrap();
    w.enemy_drop(
        h,
        DropReward {
            kind: DropKind::Point,
            value: 100,
        },
    )
    .unwrap();
    let hash = w.state_hash();
    assert_eq!(
        w.drop_item(
            v(100, 100),
            DropReward {
                kind: DropKind::Bomb,
                value: 1
            }
        ),
        Err(SimulationError::Capacity)
    );
    assert_eq!(w.state_hash(), hash);
    w.spawn_projectile(Projectile {
        position: v(320, 110),
        velocity: v(0, -20),
        collider: Collider::circle(f(2)).unwrap(),
        faction: Faction::Player,
        damage: 1,
        lifetime: 10,
        bounds: BoundsBehavior::Keep,
        rgba: 0xffffffff,
    })
    .unwrap();
    w.step().unwrap();
    assert!(w.enemy(h).is_none());
    assert_eq!(w.drops().count(), 1);
    let d = w.drops().next().unwrap().handle;
    w.despawn(d).unwrap();
    assert!(w.drop_snapshot(d).is_none());
    let next = w
        .drop_item(
            v(320, 390),
            DropReward {
                kind: DropKind::Power,
                value: 1,
            },
        )
        .unwrap();
    assert_ne!(next, d);
    w.step_with_input(Input { x: 0, y: -1 }).unwrap();
    assert_eq!(w.drops().count(), 0);
    assert!(w.events().iter().any(
        |e| matches!(e,Event::Collected {entity,kind:DropKind::Power,value:1} if *entity==next)
    ));
}

#[test]
fn drop_score_power_bombs_and_difficulty_affect_gameplay_and_restart() {
    let source = "task main() { drop(player(),0,500); drop(player(),1,9); drop(player(),2,20); wait(1); ring(vec(240.0,100.0),4,2.0,0.0,3.0,100); cancel_shots(true); wait(100); }";
    for difficulty in [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard] {
        let mut g = game(source, difficulty);
        let initial = g.state_hash();
        g.step(GameInput::default()).unwrap();
        assert_eq!(g.advanced_hud().unwrap().power, 4);
        assert_eq!(g.hud().bombs, 9);
        assert_eq!(g.hud().score, 500 * difficulty.score_multiplier());
        g.step(GameInput {
            fire: true,
            ..GameInput::default()
        })
        .unwrap();
        assert_eq!(g.hud().score, 520 * difficulty.score_multiplier());
        assert_eq!(g.advanced_hud().unwrap().cancelled, 4);
        let p = g
            .simulation()
            .snapshots()
            .find(|e| e.handle.kind() == EntityKind::Projectile)
            .unwrap();
        assert_eq!(g.simulation().projectile(p.handle).unwrap().damage, 5);
        g.restart().unwrap();
        assert_eq!(g.state_hash(), initial);
        assert_eq!(g.advanced_hud().unwrap().power, 0);
        assert_eq!(g.hud().bombs, 3);
    }
}

#[test]
fn pattern_work_counts_against_vm_command_budget_and_versions_roundtrip() {
    let program = Arc::new(
        Program::compile(
            "budget.graze",
            "task main() { ring(vec(100.0,100.0),4,1.0,0.0,3.0,100); }",
        )
        .unwrap(),
    );
    assert_eq!(program.bytecode_version(), 2);
    assert_eq!(
        Program::from_bytes(&program.to_bytes())
            .unwrap()
            .content_hash(),
        program.content_hash()
    );
    let mut vm = Vm::new(
        program.clone(),
        VmLimits {
            commands_per_tick: 3,
            ..VmLimits::default()
        },
        42,
    )
    .unwrap();
    let mut w = world(8, 8);
    let error = vm.update(&mut w).unwrap_err();
    assert_eq!(error.kind, DiagnosticKind::CommandBudget);
    assert_eq!(w.projectile_count(), 0);
    let restored = Vm::restore(program, &vm.save()).unwrap();
    assert_eq!(restored.state_hash(), vm.state_hash());
    assert_eq!(
        ScriptStage::builtin(42)
            .unwrap()
            .vm()
            .program()
            .bytecode_version(),
        1
    );
}

#[test]
fn all_creation_examples_compile_roundtrip_and_execute_without_faults() {
    for (name, source) in [
        ("ring", include_str!("../assets/examples/ring.graze")),
        ("fan", include_str!("../assets/examples/fan.graze")),
        ("aimed", include_str!("../assets/examples/aimed.graze")),
        ("spiral", include_str!("../assets/examples/spiral.graze")),
        ("motion", include_str!("../assets/examples/motion.graze")),
        (
            "straight",
            include_str!("../assets/examples/straight_laser.graze"),
        ),
        (
            "curve",
            include_str!("../assets/examples/curve_laser.graze"),
        ),
        (
            "phases",
            include_str!("../assets/examples/boss_phases.graze"),
        ),
        (
            "drops",
            include_str!("../assets/examples/drops_score.graze"),
        ),
    ] {
        let program = Program::compile(name, source).unwrap();
        assert_eq!(
            Program::from_bytes(&program.to_bytes())
                .unwrap()
                .content_hash(),
            program.content_hash()
        );
        let mut g = game(source, Difficulty::Normal);
        for _ in 0..800 {
            g.step(GameInput {
                fire: true,
                ..GameInput::default()
            })
            .unwrap();
        }
        assert_ne!(g.phase(), GamePhase::Faulted, "{name}");
        if name == "phases" {
            assert_eq!(g.phase(), GamePhase::Cleared);
            assert_eq!(g.advanced_hud().unwrap().phases_started, 3);
            assert_eq!(g.hud().tick, 721);
            assert!(g.advanced_hud().unwrap().phase_bonus > 0);
        }
    }
}

#[test]
fn showcase_runs_ten_minutes_with_three_phases_both_lasers_drops_and_vm_checkpoint() {
    let mut g = grazer::game::showcase::conformance_game();
    let mut saw = [false; 3];
    let mut straight = false;
    let mut curve = false;
    for frame in 0..36010 {
        g.step(grazer::game::showcase::input(frame)).unwrap();
        let h = g.advanced_hud().unwrap();
        if h.boss_phase > 0 {
            saw[h.boss_phase as usize - 1] = true;
        }
        for s in g.laser_segments() {
            if s.segment == 0 {
                straight = true;
            }
            if s.segment == 14 {
                curve = true;
            }
        }
        if frame == 29000 {
            let mut restored = Vm::restore(
                Arc::new(g.stage().vm().program().clone()),
                &g.stage().save(),
            )
            .unwrap();
            let mut original = g.stage().vm().clone();
            let mut a = g.simulation().clone();
            let mut b = a.clone();
            for _ in 0..500 {
                assert_eq!(
                    original.update(&mut a).unwrap(),
                    restored.update(&mut b).unwrap()
                );
                a.step().unwrap();
                b.step().unwrap();
                assert_eq!(original.state_hash(), restored.state_hash());
                assert_eq!(a.state_hash(), b.state_hash());
            }
        }
    }
    assert_eq!(g.phase(), GamePhase::Cleared);
    assert_eq!(g.hud().tick, 36001);
    assert_eq!(saw, [true; 3]);
    assert!(straight && curve);
    let h = g.advanced_hud().unwrap();
    assert!(h.power > 0 && h.collected > 0 && h.cancelled > 0);
    assert_eq!(h.phases_started, 3);
    assert!(g.hud().score > 1000);
}

#[test]
fn protocol_four_full_game_golden_trace() {
    let trace = grazer::game::showcase::trace(100000);
    assert_eq!(trace.len(), 100000);
    // Filled from the native trace, then cross-checked by the C and WASM hosts.
    assert_eq!(*trace.last().unwrap(), 0xf45a50e4211ec63f);
}

#[test]
fn invalid_laser_timing_shape_and_laser_limit_reject_without_partial_spawns() {
    let mut w = world(65, 8);
    assert_eq!(w.protocol_version(), 4);
    let collider = Collider::capsule(Vec2::ZERO, v(0, 100), f(3)).unwrap();
    let initial = w.state_hash();
    for timing in [
        LaserTiming {
            warmup: 0,
            active: 0,
            fade: 0,
        },
        LaserTiming {
            warmup: 1,
            active: u32::MAX,
            fade: 0,
        },
    ] {
        assert_eq!(
            w.spawn_laser(v(100, 100), Vec2::ZERO, collider, timing, 0xffffffff),
            Err(SimulationError::InvalidEntity)
        );
        assert_eq!(w.state_hash(), initial);
    }
    let timing = LaserTiming {
        warmup: 0,
        active: 1,
        fade: 0,
    };
    assert!(
        w.spawn_laser(
            v(100, 100),
            Vec2::ZERO,
            Collider::circle(f(3)).unwrap(),
            timing,
            0xffffffff
        )
        .is_err()
    );
    assert_eq!(w.state_hash(), initial);
    for _ in 0..advanced::MAX_LASERS {
        w.spawn_laser(v(100, 100), Vec2::ZERO, collider, timing, 0xffffffff)
            .unwrap();
    }
    let full = w.state_hash();
    assert_eq!(
        w.spawn_laser(v(100, 100), Vec2::ZERO, collider, timing, 0xffffffff),
        Err(SimulationError::Capacity)
    );
    assert_eq!(w.state_hash(), full);
    w.step().unwrap();
    assert_eq!(w.projectile_count(), 0);
    w.spawn_laser(v(100, 100), Vec2::ZERO, collider, timing, 0xffffffff)
        .unwrap();
}

#[test]
fn player_death_wins_over_completion_and_advanced_restart_preserves_options() {
    let source = "task main() { laser(vec(240.0,0.0),vec(0.0,640.0),5.0,0,3,0); complete(); }";
    let mut config = GameConfig::default();
    config.simulation.player.health = 1;
    let stage = ScriptStage::compile("death.graze", source, VmLimits::default(), 42).unwrap();
    let mut g = Game::with_advanced_stage(
        config,
        42,
        ResourcePack::builtin(),
        stage,
        AdvancedConfig {
            difficulty: Difficulty::Hard,
            drop_capacity: 16,
        },
    )
    .unwrap();
    let initial = g.state_hash();
    assert_eq!(g.protocol_version(), 4);
    g.step(GameInput::default()).unwrap();
    assert_eq!(g.phase(), GamePhase::GameOver);
    assert_eq!(g.hud().tick, 1);
    assert_eq!(g.laser_segments().count(), 1);
    for _ in 0..5 {
        g.step(GameInput::default()).unwrap();
    }
    assert_eq!(g.hud().tick, 1);
    g.restart().unwrap();
    assert_eq!(g.state_hash(), initial);
    assert_eq!(g.simulation().advanced_config().unwrap().drop_capacity, 16);
    assert_eq!(g.advanced_hud().unwrap().difficulty, 2);
    let cfg = GameConfig {
        bombs: 99,
        ..GameConfig::default()
    };
    let mut g = Game::with_stage(
        cfg,
        42,
        ResourcePack::builtin(),
        ScriptStage::compile(
            "bomb.graze",
            "task main() { drop(player(),2,1); wait(1); }",
            VmLimits::default(),
            42,
        )
        .unwrap(),
    )
    .unwrap();
    g.step(GameInput::default()).unwrap();
    assert_eq!(
        g.hud().bombs,
        99,
        "pickup never reduces a custom initial stock"
    );
}
