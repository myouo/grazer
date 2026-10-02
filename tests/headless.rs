use grazer::{
    BoundsBehavior, Collider, Enemy, EntityHandle, Event, Faction, Fixed, Input, PlayerConfig,
    Projectile, Simulation, SimulationConfig, SimulationError, Vec2,
    simulation::{
        DespawnReason, demo,
        replay::{Command, CommandResult, InputReplay, ReplayError, ReplayPlayer, ReplayRecorder},
    },
};

fn fixed(value: i32) -> Fixed {
    Fixed::from_int(value).unwrap()
}
fn point(x: i32, y: i32) -> Vec2 {
    Vec2::new(fixed(x), fixed(y))
}
fn config() -> SimulationConfig {
    SimulationConfig {
        width: fixed(100),
        height: fixed(100),
        projectile_capacity: 8,
        enemy_capacity: 4,
        player: PlayerConfig {
            position: point(50, 50),
            radius: fixed(2),
            graze_radius: fixed(10),
            speed: fixed(2),
            health: 10,
            invulnerability_ticks: 2,
        },
    }
}
fn world() -> Simulation {
    Simulation::new(config(), 42).unwrap()
}
fn projectile(position: Vec2, velocity: Vec2) -> Projectile {
    Projectile {
        position,
        velocity,
        collider: Collider::circle(Fixed::ONE).unwrap(),
        faction: Faction::Enemy,
        damage: 1,
        lifetime: 0,
        bounds: BoundsBehavior::Despawn,
        rgba: 0xffffffff,
    }
}
fn enemy(position: Vec2) -> Enemy {
    Enemy {
        position,
        velocity: Vec2::ZERO,
        radius: fixed(3),
        health: 3,
        contact_damage: 1,
        lifetime: 0,
        bounds: BoundsBehavior::Despawn,
        rgba: 0xff0000ff,
    }
}

#[test]
fn geometry_tangency_endpoints_and_one_raw_bit_separation() {
    let circle = Collider::circle(fixed(2)).unwrap();
    assert!(circle.intersects_circle(point(0, 0), point(3, 0), Fixed::ONE));
    assert!(!circle.intersects_circle(
        point(0, 0),
        Vec2::new(Fixed::from_bits((3 << 16) + 1), Fixed::ZERO),
        Fixed::ONE
    ));
    let capsule = Collider::capsule(point(-10, 0), point(10, 0), fixed(2)).unwrap();
    assert!(capsule.intersects_circle(Vec2::ZERO, point(0, 3), Fixed::ONE));
    assert!(capsule.intersects_circle(Vec2::ZERO, point(13, 0), Fixed::ONE));
    assert!(!capsule.intersects_circle(Vec2::ZERO, point(14, 0), Fixed::ONE));
    let zero_length = Collider::capsule(Vec2::ZERO, Vec2::ZERO, fixed(2)).unwrap();
    assert!(zero_length.intersects_circle(Vec2::ZERO, point(3, 0), Fixed::ONE));
    assert!(!circle.intersects_circle(Vec2::ZERO, Vec2::ZERO, fixed(-1)));
}

#[test]
fn curves_include_round_joins_but_not_their_bounding_box_interior() {
    let curve = Collider::curve(&[point(-10, 0), point(0, 10), point(10, 0)], Fixed::ONE).unwrap();
    assert!(curve.intersects_circle(Vec2::ZERO, point(0, 12), Fixed::ONE));
    assert!(curve.intersects_circle(Vec2::ZERO, point(5, 5), Fixed::ONE));
    assert!(!curve.intersects_circle(Vec2::ZERO, Vec2::ZERO, Fixed::ONE));
    assert!(Collider::curve(&[Vec2::ZERO], Fixed::ONE).is_err());
    assert!(Collider::curve(&[Vec2::ZERO; 17], Fixed::ONE).is_err());
    assert!(Collider::circle(Fixed::ZERO).is_err());
}

#[test]
fn continuous_relative_motion_for_circles_capsules_and_curves() {
    let shapes = [
        Collider::circle(Fixed::ONE).unwrap(),
        Collider::capsule(point(0, -5), point(0, 5), Fixed::ONE).unwrap(),
        Collider::curve(&[point(-5, -5), point(0, 5), point(5, -5)], Fixed::ONE).unwrap(),
    ];
    for shape in shapes {
        assert!(shape.swept_contact(
            point(10, 50),
            point(90, 50),
            point(50, 50),
            point(50, 50),
            fixed(2)
        ));
        assert!(shape.swept_contact(
            point(20, 50),
            point(80, 50),
            point(80, 50),
            point(20, 50),
            fixed(2)
        ));
        assert!(!shape.swept_contact(
            point(10, 10),
            point(90, 10),
            point(50, 50),
            point(50, 50),
            fixed(2)
        ));
    }
    let capsule = Collider::capsule(point(-10, 0), point(10, 0), Fixed::ONE).unwrap();
    assert!(capsule.swept_contact(
        point(50, 10),
        point(50, 90),
        point(50, 50),
        point(50, 50),
        Fixed::ONE
    ));
    // Disjoint collinear paths must not be classified as segment intersection.
    assert!(!capsule.swept_contact(
        point(0, 0),
        point(2, 0),
        point(100, 0),
        point(101, 0),
        Fixed::ONE
    ));
}

#[test]
fn full_q16_range_geometry_uses_exact_wide_products() {
    let min = Fixed::from_bits(i32::MIN);
    let max = Fixed::from_bits(i32::MAX);
    let capsule = Collider::capsule(Vec2::new(min, min), Vec2::new(max, max), max).unwrap();
    // The cross-product square here exceeds u128; an unchecked implementation
    // would overflow or produce a false hit despite the broad-phase overlap.
    assert!(!capsule.intersects_circle(Vec2::new(min, max), Vec2::new(max, min), Fixed::ONE));
    assert!(capsule.intersects_circle(Vec2::ZERO, Vec2::ZERO, Fixed::ZERO));
}

#[test]
fn high_speed_hit_resolves_before_offscreen_despawn() {
    let mut world = world();
    let handle = world
        .spawn_projectile(projectile(point(10, 50), point(200, 0)))
        .unwrap();
    world.step().unwrap();
    assert_eq!(world.player().health, 9);
    assert!(world.projectile(handle).is_none());
    assert_eq!(
        world.events(),
        &[
            Event::Hit {
                source: handle,
                target: EntityHandle::PLAYER,
                damage: 1
            },
            Event::Destroyed {
                entity: handle,
                reason: DespawnReason::Hit
            },
        ]
    );
}

#[test]
fn player_motion_and_enemy_motion_are_both_swept() {
    let mut config = config();
    config.player.speed = fixed(40);
    let mut world = Simulation::new(config, 0).unwrap();
    world
        .spawn_projectile(projectile(point(70, 50), Vec2::ZERO))
        .unwrap();
    world.step_with_input(Input { x: 1, y: 0 }).unwrap();
    assert_eq!(world.player().position, point(90, 50));
    assert_eq!(world.player().health, 9);
    let mut world = Simulation::new(config, 0).unwrap();
    world
        .spawn_enemy(Enemy {
            velocity: point(80, 0),
            ..enemy(point(10, 50))
        })
        .unwrap();
    world.step().unwrap();
    assert_eq!(world.player().health, 9);
    assert_eq!(world.enemy_count(), 1);
}

#[test]
fn grazing_is_once_per_generation_and_hits_take_precedence() {
    let mut world = world();
    let handle = world
        .spawn_projectile(projectile(point(50, 60), Vec2::ZERO))
        .unwrap();
    world.step().unwrap();
    assert_eq!(world.events(), &[Event::Grazed { projectile: handle }]);
    for _ in 0..4 {
        world.step().unwrap();
        assert!(world.events().is_empty());
    }
    assert_eq!(world.player().grazes, 1);
    assert_eq!(world.player().health, 10);
    world.despawn(handle).unwrap();
    let next = world
        .spawn_projectile(projectile(point(50, 60), Vec2::ZERO))
        .unwrap();
    assert_eq!(next.slot(), handle.slot());
    assert_ne!(next.generation(), handle.generation());
    world.step().unwrap();
    assert_eq!(world.player().grazes, 2);
    world
        .spawn_projectile(projectile(point(50, 50), Vec2::ZERO))
        .unwrap();
    world.step().unwrap();
    assert_eq!(world.player().grazes, 2);
    assert!(
        !world
            .events()
            .iter()
            .any(|event| matches!(event, Event::Grazed { .. }))
    );
}

#[test]
fn high_speed_graze_is_detected_without_a_hit() {
    let mut world = world();
    let handle = world
        .spawn_projectile(projectile(point(10, 58), point(80, 0)))
        .unwrap();
    world.step().unwrap();
    assert_eq!(world.player().health, 10);
    assert_eq!(world.player().grazes, 1);
    assert_eq!(world.events(), &[Event::Grazed { projectile: handle }]);
}

#[test]
fn invulnerability_protects_exactly_the_configured_subsequent_ticks() {
    let mut world = world();
    world.spawn_enemy(enemy(point(50, 50))).unwrap();
    for (health, timer) in [(9, 2), (9, 1), (9, 0), (8, 2)] {
        world.step().unwrap();
        assert_eq!(
            (world.player().health, world.player().invulnerable_ticks),
            (health, timer)
        );
    }
}

#[test]
fn damage_saturates_and_death_is_emitted_once() {
    let mut config = config();
    config.player.health = 2;
    config.player.invulnerability_ticks = 0;
    let mut world = Simulation::new(config, 0).unwrap();
    for _ in 0..3 {
        world
            .spawn_projectile(Projectile {
                damage: u32::MAX,
                ..projectile(point(50, 50), Vec2::ZERO)
            })
            .unwrap();
    }
    world.step().unwrap();
    assert_eq!(world.player().health, 0);
    assert_eq!(
        world
            .events()
            .iter()
            .filter(|event| **event == Event::PlayerDied)
            .count(),
        1
    );
    let position = world.player().position;
    world.step_with_input(Input { x: 1, y: 1 }).unwrap();
    assert_eq!(world.player().position, position);
    assert!(world.events().is_empty());
}

#[test]
fn contacts_and_compaction_follow_spawn_order_after_slot_reuse() {
    let mut world = world();
    let removed = world.spawn_enemy(enemy(point(30, 30))).unwrap();
    let first = world
        .spawn_enemy(Enemy {
            health: 1,
            ..enemy(point(30, 30))
        })
        .unwrap();
    world.despawn(removed).unwrap();
    let second = world.spawn_enemy(enemy(point(30, 30))).unwrap();
    assert_eq!(second.slot(), removed.slot());
    let shot = Projectile {
        faction: Faction::Player,
        damage: 5,
        ..projectile(point(30, 30), Vec2::ZERO)
    };
    let a = world.spawn_projectile(shot).unwrap();
    let b = world.spawn_projectile(shot).unwrap();
    world.step().unwrap();
    assert_eq!(
        &world.events()[..2],
        &[
            Event::Hit {
                source: a,
                target: first,
                damage: 1
            },
            Event::Hit {
                source: b,
                target: first,
                damage: 0
            },
        ]
    );
    assert!(world.enemy(first).is_none());
    assert_eq!(world.enemy(second).unwrap().health, 3);
    assert_eq!(
        world
            .snapshots()
            .map(|snapshot| snapshot.handle)
            .collect::<Vec<_>>(),
        vec![EntityHandle::PLAYER, second]
    );
    let c = world.spawn_projectile(shot).unwrap();
    world.step().unwrap();
    assert_eq!(
        world.events()[0],
        Event::Hit {
            source: c,
            target: second,
            damage: 3
        }
    );
}

#[test]
fn stale_or_wrong_kind_handles_and_rejected_spawns_are_atomic() {
    let mut world = world();
    let old = world
        .spawn_projectile(projectile(point(10, 10), Vec2::ZERO))
        .unwrap();
    world.despawn(old).unwrap();
    let live = world
        .spawn_projectile(projectile(point(10, 10), Vec2::ZERO))
        .unwrap();
    assert_eq!(old.slot(), live.slot());
    assert!(world.projectile(old).is_none());
    assert!(world.enemy(live).is_none());
    let hash = world.state_hash();
    assert_eq!(world.despawn(old), Err(SimulationError::InvalidHandle));
    assert_eq!(
        world.despawn(EntityHandle::PLAYER),
        Err(SimulationError::InvalidHandle)
    );
    assert_eq!(
        world.spawn_projectile(projectile(point(-1, 10), Vec2::ZERO)),
        Err(SimulationError::InvalidEntity)
    );
    assert_eq!(
        world.spawn_enemy(Enemy {
            health: 0,
            ..enemy(point(10, 10))
        }),
        Err(SimulationError::InvalidEntity)
    );
    assert_eq!(
        world.step_with_input(Input { x: 2, y: 0 }),
        Err(SimulationError::InvalidInput)
    );
    assert_eq!(hash, world.state_hash());
    assert_ne!(
        world.state_hash(),
        Simulation::new(config(), 42).unwrap().state_hash()
    );
}

#[test]
fn checked_motion_failure_preserves_every_entity_input_and_events() {
    let mut config = config();
    config.width = Fixed::from_bits(i32::MAX);
    let mut world = Simulation::new(config, 0).unwrap();
    world
        .spawn_projectile(projectile(point(10, 10), point(1, 0)))
        .unwrap();
    world
        .spawn_projectile(projectile(
            Vec2::new(Fixed::from_bits(i32::MAX - 1), fixed(10)),
            point(1, 0),
        ))
        .unwrap();
    let hash = world.state_hash();
    let snapshots: Vec<_> = world.snapshots().collect();
    assert_eq!(
        world.step_with_input(Input { x: 1, y: 1 }),
        Err(SimulationError::ArithmeticOverflow)
    );
    assert_eq!(hash, world.state_hash());
    assert_eq!(snapshots, world.snapshots().collect::<Vec<_>>());
    assert_eq!(world.input(), Input::default());
}

#[test]
fn expiry_gets_a_final_collision_pass_and_capacity_is_reusable() {
    let mut config = config();
    config.projectile_capacity = 1;
    let mut world = Simulation::new(config, 0).unwrap();
    let p = Projectile {
        lifetime: 1,
        ..projectile(point(10, 58), point(80, 0))
    };
    let handle = world.spawn_projectile(p).unwrap();
    let hash = world.state_hash();
    assert_eq!(world.spawn_projectile(p), Err(SimulationError::Capacity));
    assert_eq!(hash, world.state_hash());
    world.step().unwrap();
    assert_eq!(
        world.events(),
        &[
            Event::Grazed { projectile: handle },
            Event::Destroyed {
                entity: handle,
                reason: DespawnReason::Lifetime
            },
        ]
    );
    let next = world.spawn_projectile(p).unwrap();
    assert_eq!(next.generation(), handle.generation() + 1);
}

#[test]
fn field_edges_cull_whole_shapes_and_player_clamps_without_overflow() {
    let mut world = world();
    let handle = world
        .spawn_projectile(projectile(point(99, 10), point(1, 0)))
        .unwrap();
    world.step().unwrap();
    assert!(world.projectile(handle).is_some());
    world.step().unwrap();
    assert_eq!(
        world.events(),
        &[Event::Destroyed {
            entity: handle,
            reason: DespawnReason::OutOfBounds
        }]
    );
    for _ in 0..50 {
        world.step_with_input(Input { x: 1, y: -1 }).unwrap();
    }
    assert_eq!(world.player().position.x.bits(), (100 << 16) - 1);
    assert_eq!(world.player().position.y, Fixed::ZERO);
}

#[test]
fn config_validation_rng_and_checkpoint_continuation() {
    let mut config = config();
    config.player.graze_radius = Fixed::ONE;
    assert!(matches!(
        Simulation::new(config, 0),
        Err(SimulationError::InvalidConfig)
    ));
    let mut rng = world();
    let mut zero_seed = Simulation::new(rng.config(), 0).unwrap();
    assert_eq!(zero_seed.random_u32(), 0xe220a839);
    assert_ne!(rng.state_hash(), zero_seed.state_hash());
    let mut a = demo::fixture();
    for _ in 0..99 {
        demo::fixture_step(&mut a).unwrap();
    }
    let mut b = a.clone();
    for _ in 0..2000 {
        demo::fixture_step(&mut a).unwrap();
        demo::fixture_step(&mut b).unwrap();
        assert_eq!(a.state_hash(), b.state_hash());
        assert_eq!(a.events(), b.events());
        assert_eq!(
            a.snapshots().collect::<Vec<_>>(),
            b.snapshots().collect::<Vec<_>>()
        );
    }
    rng.set_input(Input { x: 1, y: 0 }).unwrap();
    assert_ne!(rng.state_hash(), world().state_hash());
}

#[test]
fn input_and_commands_replay_from_encoded_bytes_at_every_tick() {
    let replay = demo::replay(2000).unwrap();
    let decoded = InputReplay::from_bytes(&replay.to_bytes()).unwrap();
    assert_eq!(replay, decoded);
    let mut playback = ReplayPlayer::new(&decoded).unwrap();
    let mut live = demo::fixture();
    for frame in replay.frames() {
        demo::fixture_step(&mut live).unwrap();
        assert!(playback.step().unwrap());
        assert_eq!(live.state_hash(), frame.state_hash);
        assert_eq!(live.state_hash(), playback.simulation().state_hash());
        assert_eq!(live.events(), playback.simulation().events());
    }
    assert!(!playback.step().unwrap());
    assert_eq!(decoded.play().unwrap().state_hash(), live.state_hash());
}

#[test]
fn rejected_recording_commands_do_not_enter_the_replay() {
    let mut recorder = ReplayRecorder::new(config(), 42).unwrap();
    let handle = match recorder
        .command(Command::SpawnProjectile(projectile(
            point(10, 10),
            Vec2::ZERO,
        )))
        .unwrap()
    {
        CommandResult::Spawned(handle) => handle,
        _ => panic!("expected handle"),
    };
    recorder.command(Command::Despawn(handle)).unwrap();
    assert_eq!(
        recorder.command(Command::Despawn(handle)),
        Err(ReplayError::Simulation(SimulationError::InvalidHandle))
    );
    recorder.command(Command::RandomU32).unwrap();
    let before = recorder.simulation().state_hash();
    assert_eq!(
        recorder.step(Input { x: 3, y: 0 }),
        Err(ReplayError::Simulation(SimulationError::InvalidInput))
    );
    assert_eq!(before, recorder.simulation().state_hash());
    recorder.step(Input::default()).unwrap();
    let hash = recorder.simulation().state_hash();
    let replay = recorder.finish().unwrap();
    assert_eq!(replay.frames()[0].commands.len(), 3);
    assert_eq!(replay.play().unwrap().state_hash(), hash);
    let mut pending = ReplayRecorder::new(config(), 0).unwrap();
    pending.command(Command::RandomU32).unwrap();
    assert_eq!(pending.finish(), Err(ReplayError::PendingCommands));
}

#[test]
fn malformed_and_incompatible_replays_are_rejected_and_divergence_stops_playback() {
    let bytes = demo::replay(2).unwrap().to_bytes();
    for end in 0..bytes.len() {
        assert!(
            InputReplay::from_bytes(&bytes[..end]).is_err(),
            "accepted prefix {end}"
        );
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(
        InputReplay::from_bytes(&trailing),
        Err(ReplayError::InvalidData)
    );
    for offset in [8, 12] {
        let mut bad_version = bytes.clone();
        bad_version[offset] = 0xff;
        assert_eq!(
            InputReplay::from_bytes(&bad_version),
            Err(ReplayError::UnsupportedVersion)
        );
    }
    let mut count = bytes.clone();
    count[68..72].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(InputReplay::from_bytes(&count), Err(ReplayError::TooLarge));
    let mut corrupt = bytes.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    let replay = InputReplay::from_bytes(&corrupt).unwrap();
    let mut player = ReplayPlayer::new(&replay).unwrap();
    assert!(player.step().unwrap());
    assert!(matches!(
        player.step(),
        Err(ReplayError::HashMismatch { tick: 2, .. })
    ));
    assert_eq!(player.step(), Err(ReplayError::Stopped));
}

#[test]
fn protocol_two_golden_trace() {
    assert_eq!(
        demo::trace(100_000).last().copied(),
        Some(0x436de57247bc84be)
    );
}
