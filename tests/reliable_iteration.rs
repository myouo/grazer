use grazer::{
    Collider, Fixed, Game, GameConfig, GameInput, GamePhase, Input, Simulation, SimulationConfig,
    Vec2,
    checkpoint::CheckpointError,
    game::{
        checkpoint::{CheckpointInfo, CheckpointStage},
        debug::{DebugSession, PerformancePanel},
        replay::{GameRecorder, GameReplay, RecordingOptions, ReplayError, ReplayPlayer},
        showcase,
    },
    language::{ScriptStage, VmLimits},
    resources::ResourcePack,
};
use std::sync::Arc;
fn script(source: &str) -> Game<ScriptStage> {
    let mut c = GameConfig::default();
    c.simulation.projectile_capacity = 128;
    c.simulation.enemy_capacity = 8;
    c.simulation.player.health = 1000;
    Game::with_stage(
        c,
        42,
        ResourcePack::builtin(),
        ScriptStage::compile("fixture.graze", source, VmLimits::default(), 42).unwrap(),
    )
    .unwrap()
}
fn assert_same<S: CheckpointStage>(a: &Game<S>, b: &Game<S>) {
    assert_eq!(a.state_hashes(), b.state_hashes());
    assert_eq!(a.hud(), b.hud());
    assert_eq!(a.advanced_hud(), b.advanced_hud());
    assert_eq!(a.audio_events(), b.audio_events());
    assert_eq!(a.simulation().events(), b.simulation().events());
    assert_eq!(
        a.sprites().collect::<Vec<_>>(),
        b.sprites().collect::<Vec<_>>()
    );
    assert_eq!(
        a.laser_segments().collect::<Vec<_>>(),
        b.laser_segments().collect::<Vec<_>>()
    );
}
#[test]
fn complete_world_and_game_checkpoints_preserve_rng_pools_vm_outputs_and_continuation() {
    let mut g = script(
        "task main() { drop(player(),1,1); while true { wave(random()%1000); let p=vec(100.0,100.0); let beam=curve_laser(p,vec(80.0,80.0),vec(0.0,200.0),3.0,2,3,1); let shot=emit(p,vec(0.0,1.0),3.0,1,20); compose(shot,vec(0.0,0.01),0.001); wait(10); } }",
    );
    for frame in 0..45 {
        g.step(showcase::input(frame)).unwrap();
    }
    let bytes = g.checkpoint().unwrap();
    let info = CheckpointInfo::inspect(&bytes).unwrap();
    assert_eq!(info.hashes, g.state_hashes());
    let mut restored = Game::<ScriptStage>::restore_checkpoint(g.resource_pack(), &bytes).unwrap();
    assert_same(&g, &restored);
    for frame in 45..200 {
        g.step(showcase::input(frame)).unwrap();
        restored.step(showcase::input(frame)).unwrap();
        assert_same(&g, &restored);
    }
    g.restart().unwrap();
    restored.restart().unwrap();
    assert_same(&g, &restored);
    let mut w = Simulation::new(
        SimulationConfig {
            projectile_capacity: 8,
            enemy_capacity: 2,
            ..SimulationConfig::default()
        },
        42,
    )
    .unwrap();
    for _ in 0..17 {
        w.random_u32();
    }
    w.step_with_input(Input { x: 1, y: -1 }).unwrap();
    let mut b = Simulation::restore_checkpoint(&w.checkpoint().unwrap()).unwrap();
    for _ in 0..20 {
        assert_eq!(w.random_u32(), b.random_u32());
        w.step().unwrap();
        b.step().unwrap();
        assert_eq!(w.state_hash(), b.state_hash());
    }
}
#[test]
fn native_and_faulted_game_checkpoint_restore_are_complete() {
    let mut g = grazer::game::conformance_game();
    for frame in 0..7500 {
        g.step(grazer::game::conformance_input(frame)).unwrap();
    }
    let mut b = Game::<grazer::game::DemoStage>::restore_checkpoint(
        g.resource_pack(),
        &g.checkpoint().unwrap(),
    )
    .unwrap();
    assert_same(&g, &b);
    for frame in 7500..7700 {
        g.step(grazer::game::conformance_input(frame)).unwrap();
        b.step(grazer::game::conformance_input(frame)).unwrap();
        assert_same(&g, &b);
    }
    let mut g = script(
        "task main() { emit(vec(100.0,100.0),vec(0.0,1.0),3.0,1,20); let n=2147483647; n=n+1; }",
    );
    assert!(g.step(GameInput::default()).is_err());
    assert_eq!(g.phase(), GamePhase::Faulted);
    let mut b =
        Game::<ScriptStage>::restore_checkpoint(g.resource_pack(), &g.checkpoint().unwrap())
            .unwrap();
    assert_same(&g, &b);
    assert_eq!(g.diagnostic(), b.diagnostic());
    g.restart().unwrap();
    b.restart().unwrap();
    assert_same(&g, &b);
}
#[test]
fn corrupt_truncated_wrong_resource_stage_and_version_checkpoints_are_rejected() {
    let g = script("task main() { wait(10); }");
    let bytes = g.checkpoint().unwrap();
    for n in [0, 8, 20, bytes.len() - 1] {
        assert!(Game::<ScriptStage>::restore_checkpoint(g.resource_pack(), &bytes[..n]).is_err());
    }
    let mut corrupt = bytes.clone();
    corrupt[20] ^= 1;
    assert!(matches!(
        Game::<ScriptStage>::restore_checkpoint(g.resource_pack(), &corrupt),
        Err(CheckpointError::Fingerprint)
    ));
    assert!(matches!(
        Game::<grazer::game::DemoStage>::restore_checkpoint(g.resource_pack(), &bytes),
        Err(CheckpointError::Stage)
    ));
    let p = ResourcePack::builtin();
    let mut atlas = p.atlas().to_vec();
    atlas[0] ^= 1;
    let changed = ResourcePack::new(
        1,
        p.width(),
        p.height(),
        atlas,
        p.sprites().to_vec(),
        p.sounds().to_vec(),
    )
    .unwrap();
    assert!(matches!(
        Game::<ScriptStage>::restore_checkpoint(Arc::new(changed), &bytes),
        Err(CheckpointError::Resource)
    ));
    corrupt = bytes.clone();
    corrupt[8] = 99;
    resign(&mut corrupt);
    assert!(matches!(
        Game::<ScriptStage>::restore_checkpoint(g.resource_pack(), &corrupt),
        Err(CheckpointError::Version)
    ));
}
fn resign(bytes: &mut [u8]) {
    let end = bytes.len() - 8;
    let mut h = 0xcbf29ce484222325u64;
    for &b in &bytes[..end] {
        h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
    }
    bytes[end..].copy_from_slice(&h.to_le_bytes());
}
#[test]
fn recording_roundtrip_reset_fault_frames_checkpoint_seek_and_fast_forward() {
    let g = script("task main() { while true { wave(random()%1000); wait(1); } }");
    let mut r = GameRecorder::new(
        g,
        RecordingOptions {
            max_frames: 100,
            checkpoint_interval: 10,
        },
        "fixture",
    )
    .unwrap();
    for frame in 0..55 {
        if frame == 25 {
            r.reset().unwrap();
        }
        r.step(GameInput {
            x: if frame < 10 { 1 } else { 0 },
            fire: true,
            ..GameInput::default()
        })
        .unwrap();
    }
    let replay = Arc::new(GameReplay::from_bytes(&r.finish().to_bytes().unwrap()).unwrap());
    let mut p = ReplayPlayer::<ScriptStage>::new(replay.clone(), Arc::new(ResourcePack::builtin()))
        .unwrap();
    while p.step().unwrap() {}
    assert_eq!(p.frame(), 56);
    for frame in [0, 1, 9, 10, 25, 26, 40, 56] {
        p.seek(frame).unwrap();
        let expected = if frame == 0 {
            replay.metadata().initial_hashes
        } else {
            replay.frames()[frame - 1].hashes
        };
        assert_eq!(p.game().state_hashes(), expected);
    }
    let hash = p.game().state_hash();
    assert!(p.seek(57).is_err());
    assert_eq!(p.game().state_hash(), hash);
    p.seek(0).unwrap();
    assert_eq!(p.fast_forward(100).unwrap(), 56);
    let g = script("task main() { let n=1/0; }");
    let mut r = GameRecorder::new(
        g,
        RecordingOptions {
            max_frames: 10,
            checkpoint_interval: 1,
        },
        "fault",
    )
    .unwrap();
    assert!(r.step(GameInput::default()).is_err());
    let replay = Arc::new(GameReplay::from_bytes(&r.finish().to_bytes().unwrap()).unwrap());
    let mut p =
        ReplayPlayer::<ScriptStage>::new(replay, Arc::new(ResourcePack::builtin())).unwrap();
    assert!(p.step().unwrap());
    assert_eq!(p.game().phase(), GamePhase::Faulted);
    p.seek(1).unwrap();
    assert_eq!(p.game().phase(), GamePhase::Faulted);
}
#[test]
fn recording_input_and_frame_limit_rejections_do_not_advance() {
    let g = script("task main() { wait(100); }");
    let mut r = GameRecorder::new(
        g,
        RecordingOptions {
            max_frames: 1,
            checkpoint_interval: 0,
        },
        "limit",
    )
    .unwrap();
    let initial = r.game().state_hash();
    assert!(
        r.step(GameInput {
            x: 2,
            ..GameInput::default()
        })
        .is_err()
    );
    assert_eq!(r.game().state_hash(), initial);
    assert_eq!(r.replay().frames().len(), 0);
    r.step(GameInput::default()).unwrap();
    let hash = r.game().state_hash();
    assert!(matches!(
        r.step(GameInput::default()),
        Err(ReplayError::Limit)
    ));
    assert_eq!(r.game().state_hash(), hash);
}
#[test]
fn session_pause_step_performance_and_inspection_never_change_authoritative_state() {
    let mut s = DebugSession::new(script(
        "task main() { let n=random(); while true { wait(1); } }",
    ))
    .unwrap();
    let hash = s.game().state_hash();
    s.set_paused(true);
    s.set_hitboxes(true);
    s.set_performance_visible(true);
    for _ in 0..20 {
        assert!(
            !s.advance(GameInput {
                fire: true,
                ..GameInput::default()
            })
            .unwrap()
        );
        s.observe_frame(1.0, 2.0, 3.0);
    }
    assert_eq!(s.game().state_hash(), hash);
    let tasks = s.game().stage().vm().inspect_tasks();
    assert_eq!(tasks.len(), 1);
    assert!(!tasks[0].frames[0].registers.is_empty());
    assert_eq!(s.game().state_hash(), hash);
    assert!(s.single_step(GameInput::default()).unwrap());
    assert_eq!(s.game().hud().tick, 1);
    assert!(s.paused());
    s.set_paused(false);
    assert!(s.single_step(GameInput::default()).is_err());
    let mut panel = PerformancePanel::default();
    for i in 1..=100 {
        assert!(panel.observe(i as f64, 2.0, i as f64 + 2.0));
    }
    let summary = panel.summary();
    assert_eq!(summary.update_p95_ms, 95.0);
    assert_eq!(summary.frame_p95_ms, 97.0);
    assert!(!panel.observe(f64::NAN, 0.0, 1.0));
    assert_eq!(panel.summary(), summary);
}
#[test]
fn hot_reload_failure_preserves_old_run_and_success_archives_recording() {
    let mut s = DebugSession::new(script("task main() { wait(100); }")).unwrap();
    s.start_recording(
        RecordingOptions {
            max_frames: 10,
            checkpoint_interval: 2,
        },
        "reload",
    )
    .unwrap();
    s.advance(GameInput::default()).unwrap();
    let hash = s.game().state_hash();
    assert!(
        s.reload(
            "bad.graze",
            "task main() { let n: int = true; }",
            ResourcePack::builtin()
        )
        .is_err()
    );
    assert_eq!(s.game().state_hash(), hash);
    assert!(s.recording());
    let old = s
        .reload(
            "new.graze",
            "task main() { wave(99); wait(100); }",
            ResourcePack::builtin(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(old.frames().len(), 1);
    assert!(s.paused());
    assert_eq!(s.game().hud().tick, 0);
    s.single_step(GameInput::default()).unwrap();
    assert_eq!(s.game().hud().wave, 99);
    let mut p =
        ReplayPlayer::<ScriptStage>::new(Arc::new(old), Arc::new(ResourcePack::builtin())).unwrap();
    p.step().unwrap();
    assert_eq!(p.game().state_hash(), hash);
}
#[test]
fn all_boss_practice_entries_are_normal_health_reproducible_and_reset_to_entry() {
    for phase in 1..=3 {
        let g = showcase::practice(phase, grazer::advanced::Difficulty::Normal, 0).unwrap();
        assert_eq!(g.hud().health, 3);
        assert_eq!(g.hud().bombs, 3);
        assert_eq!(g.advanced_hud().unwrap().power, 4);
        assert_eq!(g.advanced_hud().unwrap().boss_phase, phase);
        assert_eq!(g.hud().tick, 25201 + u64::from(phase - 1) * 3600);
        let hash = g.state_hash();
        let mut s = DebugSession::new(g).unwrap();
        s.set_paused(true);
        for _ in 0..20 {
            s.single_step(GameInput {
                fire: true,
                ..GameInput::default()
            })
            .unwrap();
        }
        s.restart().unwrap();
        assert_eq!(s.game().state_hash(), hash);
    }
}
#[test]
fn advanced_world_shape_snapshot_rejects_invalid_pool_data_and_preserves_graze_generation() {
    let mut w = Simulation::new(
        SimulationConfig {
            projectile_capacity: 8,
            enemy_capacity: 2,
            ..SimulationConfig::default()
        },
        42,
    )
    .unwrap();
    w.enable_advanced(grazer::advanced::AdvancedConfig {
        drop_capacity: 8,
        ..Default::default()
    })
    .unwrap();
    let f = |n| Fixed::from_int(n).unwrap();
    let v = |x, y| Vec2::new(f(x), f(y));
    let h = w
        .spawn_laser(
            v(330, 100),
            Vec2::ZERO,
            Collider::capsule(Vec2::ZERO, v(0, 320), f(3)).unwrap(),
            grazer::advanced::LaserTiming {
                warmup: 0,
                active: 20,
                fade: 2,
            },
            0xffffffff,
        )
        .unwrap();
    w.step().unwrap();
    assert_eq!(w.player().grazes, 1);
    let mut b = Simulation::restore_checkpoint(&w.checkpoint().unwrap()).unwrap();
    for _ in 0..21 {
        w.step().unwrap();
        b.step().unwrap();
        assert_eq!(w.state_hash(), b.state_hash());
    }
    assert!(w.projectile(h).is_none());
    assert_eq!(w.player().grazes, 1);
}
