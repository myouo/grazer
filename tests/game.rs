use grazer::{
    BoundsBehavior, Collider, Enemy, Faction, Fixed, Projectile, Simulation, SimulationError, Vec2,
    game::{
        FrameClock, Game, GameConfig, GameInput, GamePhase, NOMINAL_STAGE_TICKS, Stage,
        StageStatus, conformance_game, conformance_input,
    },
    resources::{ResourceError, ResourcePack},
};
use std::time::Duration;
fn units(value: i32) -> Fixed {
    Fixed::from_int(value).unwrap()
}
#[derive(Clone, Default)]
struct Empty;
impl Stage for Empty {
    const CONTENT_ID: u64 = 1;
    fn update(&mut self, _: &mut Simulation) -> Result<StageStatus, SimulationError> {
        Ok(StageStatus::default())
    }
    fn state_hash(&self) -> u64 {
        0
    }
}
fn empty() -> Game<Empty> {
    Game::with_stage(GameConfig::default(), 42, ResourcePack::builtin(), Empty).unwrap()
}
#[derive(Clone, Default)]
struct Target {
    spawned: bool,
    boss: bool,
    lethal: bool,
    handle: Option<grazer::EntityHandle>,
}
impl Stage for Target {
    const CONTENT_ID: u64 = 2;
    fn update(&mut self, world: &mut Simulation) -> Result<StageStatus, SimulationError> {
        if !self.spawned {
            let player = world.player().position;
            let position = if self.lethal {
                player
            } else {
                Vec2::new(player.x, player.y.checked_sub(units(26)).unwrap())
            };
            self.handle = Some(world.spawn_enemy(Enemy {
                position,
                velocity: Vec2::ZERO,
                radius: units(12),
                health: 1,
                contact_damage: 1,
                lifetime: 0,
                bounds: BoundsBehavior::Keep,
                rgba: 0xffffffff,
            })?);
            world.spawn_projectile(Projectile {
                position: player,
                velocity: Vec2::ZERO,
                collider: Collider::circle(units(3)).unwrap(),
                faction: Faction::Enemy,
                damage: 1,
                lifetime: 200,
                bounds: BoundsBehavior::Keep,
                rgba: 0xffffffff,
            })?;
            self.spawned = true;
        }
        Ok(StageStatus {
            boss: if self.boss { self.handle } else { None },
            boss_max_health: u32::from(self.boss),
            ..StageStatus::default()
        })
    }
    fn state_hash(&self) -> u64 {
        u64::from(self.spawned) | u64::from(self.boss) << 1 | u64::from(self.lethal) << 2
    }
}
#[test]
fn held_fire_has_fixed_cadence_and_shared_resource_sprites() {
    let mut game = empty();
    for tick in 0..7 {
        game.step(GameInput {
            fire: true,
            ..GameInput::default()
        })
        .unwrap();
        assert_eq!(
            game.simulation().projectile_count(),
            if tick < 6 { 2 } else { 4 }
        );
    }
    assert!(
        game.sprites()
            .any(|s| s.resource_id == grazer::resources::PLAYER_SHOT)
    );
    assert!(
        game.sprites()
            .all(|s| game.resources().sprite(s.resource_id).is_some())
    );
    assert_eq!(game.audio_events()[0].resource_id, 1);
    assert_eq!(game.audio_events()[0].tick, 7);
}
#[test]
fn focus_halves_speed_and_invalid_flags_do_not_mutate_state() {
    let mut fast = empty();
    let mut slow = empty();
    let origin = fast.simulation().player().position.x.bits();
    fast.step(GameInput {
        x: 1,
        ..GameInput::default()
    })
    .unwrap();
    slow.step(GameInput {
        x: 1,
        focus: true,
        ..GameInput::default()
    })
    .unwrap();
    assert_eq!(
        fast.simulation().player().position.x.bits() - origin,
        2 * (slow.simulation().player().position.x.bits() - origin)
    );
    let hash = fast.state_hash();
    assert!(
        fast.step(GameInput {
            x: 2,
            ..GameInput::default()
        })
        .is_err()
    );
    assert_eq!(hash, fast.state_hash());
    assert!(GameInput::from_flags(0, 0, 16).is_err());
    assert_eq!(GameInput::from_flags(1, -1, 15).unwrap().flags(), 15);
}
#[test]
fn bomb_is_edge_triggered_clears_shots_and_kills_before_body_contact() {
    let mut game = Game::with_stage(
        GameConfig::default(),
        42,
        ResourcePack::builtin(),
        Target {
            lethal: true,
            ..Target::default()
        },
    )
    .unwrap();
    for _ in 0..5 {
        game.step(GameInput {
            bomb: true,
            ..GameInput::default()
        })
        .unwrap();
    }
    assert_eq!(game.hud().bombs, 2);
    assert_eq!(game.hud().health, 3);
    assert_eq!(game.hud().score, 100);
    assert_eq!(game.simulation().enemy_count(), 0);
    assert_eq!(game.simulation().projectile_count(), 0);
    assert!(game.simulation().player().invulnerable_ticks > 0);
    game.step(GameInput::default()).unwrap();
    game.step(GameInput {
        bomb: true,
        ..GameInput::default()
    })
    .unwrap();
    assert_eq!(game.hud().bombs, 1);
}
#[test]
fn death_freezes_world_and_restart_restores_initial_run() {
    let mut config = GameConfig::default();
    config.simulation.player.health = 1;
    let mut game = Game::with_stage(
        config,
        42,
        ResourcePack::builtin(),
        Target {
            lethal: true,
            ..Target::default()
        },
    )
    .unwrap();
    let initial = game.state_hash();
    game.step(GameInput::default()).unwrap();
    assert_eq!(game.phase(), GamePhase::GameOver);
    assert!(game.audio_events().iter().any(|e| e.resource_id == 8));
    let tick = game.hud().tick;
    game.step(GameInput {
        fire: true,
        ..GameInput::default()
    })
    .unwrap();
    assert_eq!(game.hud().tick, tick);
    assert!(game.audio_events().is_empty());
    game.restart().unwrap();
    assert_eq!(initial, game.state_hash());
    assert_eq!(game.hud().health, 1);
}
#[test]
fn boss_damage_victory_and_hud_have_one_shared_state() {
    let mut game = Game::with_stage(
        GameConfig::default(),
        42,
        ResourcePack::builtin(),
        Target {
            boss: true,
            ..Target::default()
        },
    )
    .unwrap();
    game.step(GameInput {
        fire: true,
        ..GameInput::default()
    })
    .unwrap();
    assert_eq!(game.phase(), GamePhase::Cleared);
    assert_eq!(game.hud().boss_health, 0);
    assert_eq!(game.hud().score, 100);
    assert!(game.audio_events().iter().any(|e| e.resource_id == 7));
    assert!(
        game.audio_events()
            .iter()
            .enumerate()
            .all(|(i, e)| e.sequence == i as u32 && e.tick == game.hud().tick)
    );
}
#[derive(Clone)]
struct Broken;
impl Stage for Broken {
    const CONTENT_ID: u64 = 3;
    fn update(&mut self, _: &mut Simulation) -> Result<StageStatus, SimulationError> {
        Err(SimulationError::Capacity)
    }
    fn state_hash(&self) -> u64 {
        0
    }
}
#[test]
fn stage_errors_stop_until_restart() {
    let mut game =
        Game::with_stage(GameConfig::default(), 0, ResourcePack::builtin(), Broken).unwrap();
    assert!(game.step(GameInput::default()).is_err());
    assert_eq!(game.phase(), GamePhase::Faulted);
    assert!(game.step(GameInput::default()).is_err());
    game.restart().unwrap();
    assert_eq!(game.phase(), GamePhase::Playing);
}
#[test]
fn complete_native_stage_reaches_boss_and_clear_in_about_three_minutes() {
    let mut game = conformance_game();
    let mut saw_boss = false;
    for frame in 0..NOMINAL_STAGE_TICKS {
        game.step(conformance_input(frame)).unwrap();
        saw_boss |= game.hud().boss_max_health == 1000;
        if game.phase() == GamePhase::Cleared {
            break;
        }
    }
    assert!(saw_boss);
    assert_eq!(game.phase(), GamePhase::Cleared);
    assert!((9000..=NOMINAL_STAGE_TICKS).contains(&game.hud().tick));
}
#[test]
fn cloned_game_and_recorded_inputs_reproduce_hud_audio_and_sprites() {
    let mut live = conformance_game();
    let mut replay = conformance_game();
    for frame in 0..1200 {
        let input = conformance_input(frame);
        live.step(input).unwrap();
        replay.step(input).unwrap();
        assert_eq!(live.state_hash(), replay.state_hash());
        assert_eq!(live.hud(), replay.hud());
        assert_eq!(live.audio_events(), replay.audio_events());
        assert_eq!(
            live.sprites().collect::<Vec<_>>(),
            replay.sprites().collect::<Vec<_>>()
        );
    }
    let mut clone = live.clone();
    for frame in 1200..2500 {
        for game in [&mut live, &mut clone] {
            game.step(conformance_input(frame)).unwrap();
        }
        assert_eq!(live.state_hash(), clone.state_hash());
    }
}
#[test]
fn rational_host_clock_retains_backlog_and_discards_paused_time() {
    let mut clock = FrameClock::default();
    let mut ticks = clock.advance(Duration::from_secs(1), true);
    assert_eq!(ticks, 8);
    loop {
        let next = clock.advance(Duration::ZERO, true);
        if next == 0 {
            break;
        }
        ticks += next;
    }
    assert_eq!(ticks, 60);
    clock.advance(Duration::from_secs(1), true);
    assert_eq!(clock.advance(Duration::from_secs(10), false), 0);
    assert_eq!(clock.advance(Duration::ZERO, true), 0);
    let mut count = 0;
    for _ in 0..1000 {
        count += clock.advance(Duration::from_millis(1), true);
    }
    assert_eq!(count, 60);
}
#[test]
fn resource_data_is_validated_and_ids_are_canonical() {
    let pack = ResourcePack::builtin();
    let mut sprites = pack.sprites().to_vec();
    sprites.reverse();
    let mut sounds = pack.sounds().to_vec();
    sounds.reverse();
    let reordered = ResourcePack::new(
        1,
        pack.width(),
        pack.height(),
        pack.atlas().to_vec(),
        sprites.clone(),
        sounds.clone(),
    )
    .unwrap();
    assert_eq!(pack.content_hash(), reordered.content_hash());
    assert!(matches!(
        ResourcePack::new(
            9,
            128,
            128,
            pack.atlas().to_vec(),
            sprites.clone(),
            sounds.clone()
        ),
        Err(ResourceError::Version)
    ));
    sprites.push(sprites[0]);
    assert!(matches!(
        ResourcePack::new(1, 128, 128, pack.atlas().to_vec(), sprites, sounds),
        Err(ResourceError::Duplicate(_))
    ));
    assert!(matches!(
        ResourcePack::new(1, 128, 128, vec![0], vec![], vec![]),
        Err(ResourceError::Atlas)
    ));
    assert_eq!(
        include_bytes!("../assets/demo/sprites.rgba").as_slice(),
        pack.atlas()
    );
}

#[test]
fn protocol_three_full_game_golden_trace() {
    assert_eq!(
        grazer::game::trace(100000).last().copied(),
        Some(0xb1b54e555b1dfae4)
    );
}
#[cfg(feature = "resources")]
#[test]
fn json_pack_matches_embedded_resources_and_rejects_bad_paths() {
    let pack = ResourcePack::builtin();
    let json = include_str!("../assets/demo/project.json");
    assert_eq!(
        ResourcePack::from_json(json, pack.atlas().to_vec())
            .unwrap()
            .content_hash(),
        pack.content_hash()
    );
    assert!(
        ResourcePack::from_json(
            &json.replace("sprites.rgba", "../sprites.rgba"),
            pack.atlas().to_vec()
        )
        .is_err()
    );
    assert!(ResourcePack::from_json("{", pack.atlas().to_vec()).is_err());
    assert!(ResourcePack::from_json(json, vec![]).is_err());
}
