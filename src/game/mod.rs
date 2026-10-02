//! Playable native-stage SDK. Protocol 3 wraps the protocol-2 simulation with
//! stage state, six input controls, score, bombs and deterministic audio events.
//! Presentation and wall-clock scheduling remain outside authoritative state.
//!
//! ```
//! use grazer::{Game, GameInput};
//! let mut game = Game::new(42)?;
//! game.step(GameInput { fire: true, ..GameInput::default() })?;
//! assert_eq!(game.hud().tick, 1);
//! assert!(game.sprites().any(|sprite| sprite.resource_id == 4));
//! # Ok::<(), grazer::GameError>(())
//! ```
mod clock;
pub mod showcase;
mod stage;
use crate::advanced::{AdvancedConfig, DropKind, LaserSegment};
use crate::{
    BoundsBehavior, Collider, EntityKind, Event, Faction, Fixed, Input, PlayerConfig, Projectile,
    Simulation, SimulationConfig, SimulationError, Vec2,
    resources::{self, Fingerprint, ResourceError, ResourcePack},
};
pub use clock::FrameClock;
pub use stage::{DemoStage, Stage, StageStatus};
use std::sync::Arc;
pub const GAME_PROTOCOL_VERSION: u32 = 3;
pub const NOMINAL_STAGE_TICKS: u64 = 180 * 60;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameInput {
    pub x: i32,
    pub y: i32,
    pub fire: bool,
    pub bomb: bool,
    pub focus: bool,
    pub restart: bool,
}
impl GameInput {
    pub fn from_flags(x: i32, y: i32, flags: u32) -> Result<Self, GameError> {
        if !(-1..=1).contains(&x) || !(-1..=1).contains(&y) || flags & !15 != 0 {
            return Err(GameError::Input);
        }
        Ok(Self {
            x,
            y,
            fire: flags & 1 != 0,
            bomb: flags & 2 != 0,
            focus: flags & 4 != 0,
            restart: flags & 8 != 0,
        })
    }
    pub fn flags(self) -> u32 {
        u32::from(self.fire)
            | u32::from(self.bomb) << 1
            | u32::from(self.focus) << 2
            | u32::from(self.restart) << 3
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum GamePhase {
    Playing = 0,
    GameOver = 1,
    Cleared = 2,
    Faulted = 3,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameError {
    Script(crate::language::Diagnostic),
    Input,
    Resources(ResourceError),
    Simulation(SimulationError),
    Faulted,
}
impl From<ResourceError> for GameError {
    fn from(error: ResourceError) -> Self {
        Self::Resources(error)
    }
}
impl From<SimulationError> for GameError {
    fn from(error: SimulationError) -> Self {
        Self::Simulation(error)
    }
}
impl std::fmt::Display for GameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Script(diagnostic) => write!(f, "{diagnostic}"),
            Self::Input => f.write_str("game axes must be -1, 0 or 1"),
            Self::Resources(error) => write!(f, "{error}"),
            Self::Simulation(error) => write!(f, "{error}"),
            Self::Faulted => f.write_str("game stopped after a stage error; restart to recover"),
        }
    }
}
impl std::error::Error for GameError {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameConfig {
    pub simulation: SimulationConfig,
    pub bombs: u32,
    pub shot_interval: u32,
    pub bomb_damage: u32,
}
impl Default for GameConfig {
    fn default() -> Self {
        Self {
            simulation: SimulationConfig {
                width: units(480),
                height: units(640),
                projectile_capacity: 8192,
                enemy_capacity: 64,
                player: PlayerConfig {
                    position: point(240, 560),
                    radius: units(2),
                    graze_radius: units(12),
                    speed: units(3),
                    health: 3,
                    invulnerability_ticks: 150,
                },
            },
            bombs: 3,
            shot_interval: 6,
            bomb_damage: 80,
        }
    }
}
/// POD bulk output shared by Rust, browser and C hosts. No GPU handles.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameSprite {
    pub kind: u32,
    pub slot: u32,
    pub generation: u32,
    pub resource_id: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rgba: u32,
    pub layer: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AudioEvent {
    pub resource_id: u32,
    pub sequence: u32,
    pub tick: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hud {
    pub tick: u64,
    pub score: u64,
    pub grazes: u64,
    pub health: u32,
    pub bombs: u32,
    pub phase: u32,
    pub wave: u32,
    pub boss_health: u32,
    pub boss_max_health: u32,
    pub projectiles: u32,
    pub enemies: u32,
    pub bomb_flash: u32,
}
/// Additive M4 HUD; the existing HUD and C ABI keep their layouts.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AdvancedHud {
    pub difficulty: u32,
    pub power: u32,
    pub drops: u32,
    pub boss_phase: u32,
    pub phase_ticks: u32,
    pub phases_started: u32,
    pub collected: u64,
    pub cancelled: u64,
    pub phase_bonus: u64,
}
pub struct Game<S: Stage = DemoStage> {
    config: GameConfig,
    seed: u64,
    resources: Arc<ResourcePack>,
    world: Simulation,
    stage: S,
    initial_stage: S,
    status: StageStatus,
    phase: GamePhase,
    input: GameInput,
    bombs: u32,
    score: u64,
    shot_cooldown: u32,
    flash: u32,
    audio: Vec<AudioEvent>,
    power: u32,
    phase_bonus: u64,
}
impl<S: Stage> Clone for Game<S> {
    fn clone(&self) -> Self {
        let mut audio = Vec::with_capacity(self.audio.capacity());
        audio.extend_from_slice(&self.audio);
        Self {
            config: self.config,
            seed: self.seed,
            resources: self.resources.clone(),
            world: self.world.clone(),
            stage: self.stage.clone(),
            initial_stage: self.initial_stage.clone(),
            status: self.status,
            phase: self.phase,
            input: self.input,
            bombs: self.bombs,
            score: self.score,
            shot_cooldown: self.shot_cooldown,
            flash: self.flash,
            audio,
            power: self.power,
            phase_bonus: self.phase_bonus,
        }
    }
}
impl Game<DemoStage> {
    pub fn new(seed: u64) -> Result<Self, GameError> {
        Self::with_stage(
            GameConfig::default(),
            seed,
            ResourcePack::builtin(),
            DemoStage::default(),
        )
    }
}
impl<S: Stage> Game<S> {
    pub fn with_stage(
        config: GameConfig,
        seed: u64,
        resources: ResourcePack,
        stage: S,
    ) -> Result<Self, GameError> {
        let advanced = stage.advanced_config();
        Self::with_stage_options(config, seed, resources, stage, advanced)
    }
    pub fn with_advanced_stage(
        config: GameConfig,
        seed: u64,
        resources: ResourcePack,
        stage: S,
        advanced: AdvancedConfig,
    ) -> Result<Self, GameError> {
        Self::with_stage_options(config, seed, resources, stage, Some(advanced))
    }
    fn with_stage_options(
        config: GameConfig,
        seed: u64,
        resources: ResourcePack,
        stage: S,
        advanced: Option<AdvancedConfig>,
    ) -> Result<Self, GameError> {
        if config.shot_interval == 0 || config.bombs > 99 {
            return Err(SimulationError::InvalidConfig.into());
        }
        for id in 1..=9 {
            if resources.sprite(id).is_none() {
                return Err(ResourceError::Sprite(id).into());
            }
        }
        for id in 1..=8 {
            if resources.sound(id).is_none() {
                return Err(ResourceError::Sound(id).into());
            }
        }
        let mut world = Simulation::new(config.simulation, seed)?;
        if let Some(options) = advanced {
            world.enable_advanced(options)?;
        }
        Ok(Self {
            config,
            seed,
            resources: Arc::new(resources),
            world,
            initial_stage: stage.clone(),
            stage,
            status: StageStatus::default(),
            phase: GamePhase::Playing,
            input: GameInput::default(),
            bombs: config.bombs,
            score: 0,
            shot_cooldown: 0,
            flash: 0,
            audio: Vec::with_capacity(config.simulation.enemy_capacity as usize + 16),
            power: 0,
            phase_bonus: 0,
        })
    }
    pub fn simulation(&self) -> &Simulation {
        &self.world
    }
    pub fn protocol_version(&self) -> u32 {
        if self.world.advanced_config().is_some() {
            crate::advanced::ADVANCED_PROTOCOL_VERSION
        } else {
            GAME_PROTOCOL_VERSION
        }
    }
    pub fn stage(&self) -> &S {
        &self.stage
    }
    pub fn diagnostic(&self) -> Option<&crate::language::Diagnostic> {
        self.stage.diagnostic()
    }
    pub fn resources(&self) -> &ResourcePack {
        &self.resources
    }
    pub fn phase(&self) -> GamePhase {
        self.phase
    }
    pub fn audio_events(&self) -> &[AudioEvent] {
        &self.audio
    }
    pub fn hud(&self) -> Hud {
        Hud {
            tick: self.world.tick(),
            score: self.score,
            grazes: self.world.player().grazes,
            health: self.world.player().health,
            bombs: self.bombs,
            phase: self.phase as u32,
            wave: self.status.wave,
            boss_health: self
                .status
                .boss
                .and_then(|h| self.world.enemy(h))
                .map_or(0, |e| e.health),
            boss_max_health: self.status.boss_max_health,
            projectiles: self.world.projectile_count() as u32,
            enemies: self.world.enemy_count() as u32,
            bomb_flash: self.flash,
        }
    }
    pub fn advanced_hud(&self) -> Option<AdvancedHud> {
        let m = self.world.advanced_metrics()?;
        Some(AdvancedHud {
            difficulty: self.world.difficulty()? as u32,
            power: self.power,
            drops: m.drops,
            boss_phase: m.boss_phase,
            phase_ticks: m.phase_ticks,
            phases_started: m.phases_started,
            collected: m.collected,
            cancelled: m.cancelled,
            phase_bonus: self.phase_bonus,
        })
    }
    pub fn laser_segments(&self) -> impl Iterator<Item = LaserSegment> + '_ {
        self.world.laser_segments()
    }
    /// Worst-case preallocation for beams, drop sprites and HUD composition.
    pub fn presentation_capacity(&self) -> usize {
        let cfg = self.world.config();
        cfg.projectile_capacity as usize
            + if self.world.advanced_config().is_some() {
                crate::advanced::MAX_LASERS * 4 * (crate::simulation::MAX_CURVE_POINTS - 1)
            } else {
                0
            }
            + cfg.enemy_capacity as usize
            + self
                .world
                .advanced_config()
                .map_or(0, |c| c.drop_capacity as usize)
            + 1024
    }
    /// Restart allocates fresh pools; ordinary successful ticks do not allocate.
    pub fn restart(&mut self) -> Result<(), GameError> {
        let advanced = self.world.advanced_config();
        self.world = Simulation::new(self.config.simulation, self.seed)?;
        if let Some(options) = advanced {
            self.world.enable_advanced(options)?;
        }
        self.stage = self.initial_stage.clone();
        self.status = StageStatus::default();
        self.phase = GamePhase::Playing;
        self.input = GameInput::default();
        self.bombs = self.config.bombs;
        self.score = 0;
        self.shot_cooldown = 0;
        self.flash = 0;
        self.audio.clear();
        self.power = 0;
        self.phase_bonus = 0;
        Ok(())
    }
    /// Invalid input is rejected atomically. A native stage/command failure
    /// stops the game visibly; partial stage commands are retained until restart.
    pub fn step(&mut self, input: GameInput) -> Result<(), GameError> {
        if !(-1..=1).contains(&input.x) || !(-1..=1).contains(&input.y) {
            return Err(GameError::Input);
        }
        if input.restart && !self.input.restart {
            self.restart()?;
            self.input = input;
            return Ok(());
        }
        if self.phase == GamePhase::Faulted {
            return Err(GameError::Faulted);
        }
        self.audio.clear();
        let bomb_edge = input.bomb && !self.input.bomb;
        self.input = input;
        if self.phase != GamePhase::Playing {
            return Ok(());
        }
        if let Err(error) = self.active_step(input, bomb_edge) {
            self.phase = GamePhase::Faulted;
            self.audio.clear();
            return Err(error);
        }
        Ok(())
    }
    fn emit(&mut self, id: u32) {
        self.audio.push(AudioEvent {
            resource_id: id,
            sequence: self.audio.len() as u32,
            tick: 0,
        });
    }
    fn active_step(&mut self, input: GameInput, bomb_edge: bool) -> Result<(), GameError> {
        let old_status = self.status;
        self.status = match self.stage.update(&mut self.world) {
            Ok(status) => status,
            Err(error) => {
                return Err(self.stage.diagnostic().map_or_else(
                    || GameError::Simulation(error),
                    |diagnostic| GameError::Script(diagnostic.clone()),
                ));
            }
        };
        let advanced = self.world.advanced_config().is_some();
        let multiplier = self.world.difficulty().map_or(1, |d| d.score_multiplier());
        if self.status.boss.is_some()
            && (old_status.boss.is_none() || advanced && old_status.boss != self.status.boss)
        {
            self.emit(6);
        }
        self.shot_cooldown = self.shot_cooldown.saturating_sub(1);
        self.flash = self.flash.saturating_sub(1);
        if bomb_edge && self.bombs > 0 {
            self.bombs -= 1;
            if advanced {
                self.world.cancel_shots(false)?;
            } else {
                self.world.clear_hostile_projectiles();
            }
            self.world.protect_player(90);
            self.world.damage_enemies(self.config.bomb_damage);
            self.flash = 30;
            self.emit(2);
        }
        if input.fire && self.shot_cooldown == 0 {
            let p = self.world.player().position;
            let cfg = self.world.config();
            // Keep the twin shot atomic if a custom stage fills the pool.
            if self.world.projectile_count() + 2 > cfg.projectile_capacity as usize {
                return Err(SimulationError::Capacity.into());
            }
            for offset in [-5, 5] {
                let x = (i64::from(p.x.bits()) + i64::from(offset) * 65536)
                    .clamp(0, i64::from(cfg.width.bits()) - 1) as i32;
                let y = (p.y.bits() - 14 * 65536).max(0);
                self.world.spawn_projectile(Projectile {
                    position: Vec2::new(Fixed::from_bits(x), Fixed::from_bits(y)),
                    velocity: point(0, -12),
                    collider: Collider::circle(units(2)).expect("positive radius"),
                    faction: Faction::Player,
                    damage: 1 + self.power,
                    lifetime: 70,
                    bounds: BoundsBehavior::Despawn,
                    rgba: 0xffed9aff,
                })?;
            }
            self.shot_cooldown = self.config.shot_interval;
            self.emit(1);
        }
        let speed = if input.focus {
            Fixed::from_bits(self.config.simulation.player.speed.bits() / 2)
        } else {
            self.config.simulation.player.speed
        };
        self.world.step_with_speed(
            Input {
                x: input.x,
                y: input.y,
            },
            speed,
        )?;
        self.stage.after_step(&self.world);
        self.score = self
            .score
            .saturating_add(self.world.take_cancel_points().saturating_mul(multiplier));
        let mut hit = false;
        let mut graze = false;
        let mut died = false;
        for index in 0..self.world.events().len() {
            match self.world.events()[index] {
                Event::Collected { kind, value, .. } => match kind {
                    DropKind::Point => {
                        self.score = self
                            .score
                            .saturating_add(u64::from(value).saturating_mul(multiplier))
                    }
                    DropKind::Power => self.power = self.power.saturating_add(value).min(4),
                    DropKind::Bomb => {
                        if self.bombs < 9 {
                            self.bombs = self.bombs.saturating_add(value).min(9);
                        }
                    }
                },
                Event::Hit { target, damage, .. }
                    if target.kind() == EntityKind::Player && damage > 0 =>
                {
                    hit = true
                }
                Event::Grazed { .. } => {
                    graze = true;
                    self.score = self.score.saturating_add(10 * multiplier);
                }
                Event::Destroyed {
                    entity,
                    reason: crate::simulation::DespawnReason::HealthDepleted,
                } if entity.kind() == EntityKind::Enemy => {
                    self.score = self.score.saturating_add(100 * multiplier);
                    if advanced && Some(entity) == self.status.boss {
                        let bonus = 5000 * multiplier;
                        self.score = self.score.saturating_add(bonus);
                        self.phase_bonus = self.phase_bonus.saturating_add(bonus);
                    }
                    self.emit(4);
                }
                Event::PlayerDied => died = true,
                _ => {}
            }
        }
        if hit {
            self.emit(3);
        }
        if graze {
            self.emit(5);
        }
        if died {
            self.phase = GamePhase::GameOver;
            self.emit(8);
        } else if self.status.complete
            || !advanced
                && self
                    .status
                    .boss
                    .is_some_and(|handle| self.world.enemy(handle).is_none())
        {
            self.phase = GamePhase::Cleared;
            self.emit(7);
        }
        for event in &mut self.audio {
            event.tick = self.world.tick();
        }
        Ok(())
    }
    /// Same owned sprite layout in all hosts. M4 drops use layer 25 and the
    /// player layer 30. Lasers use the separate `laser_segments` snapshot.
    /// Presentation floats never return to authoritative simulation.
    pub fn sprites(&self) -> impl Iterator<Item = GameSprite> + '_ {
        self.world.snapshots().filter_map(|e| {
            let (resource_id, width, height, rgba, layer) = match e.handle.kind() {
                EntityKind::Player => {
                    if self.world.player().health == 0 {
                        return None;
                    }
                    let rgba = if self.world.player().invulnerable_ticks > 0
                        && (self.world.tick() / 4).is_multiple_of(2)
                    {
                        0x95ffffff
                    } else {
                        0x79deffff
                    };
                    (resources::PLAYER, 24.0, 28.0, rgba, 30)
                }
                EntityKind::Enemy if Some(e.handle) == self.status.boss => {
                    (resources::BOSS, 72.0, 64.0, e.rgba, 10)
                }
                EntityKind::Enemy => (resources::ENEMY, 28.0, 28.0, e.rgba, 10),
                EntityKind::Projectile => {
                    if self.world.laser_phase(e.handle).is_some() {
                        return None;
                    }
                    let friendly = self
                        .world
                        .projectile(e.handle)
                        .expect("live snapshot")
                        .faction
                        == Faction::Player;
                    if friendly {
                        (resources::PLAYER_SHOT, 7.0, 18.0, e.rgba, 20)
                    } else {
                        (resources::ENEMY_SHOT, 12.0, 12.0, e.rgba, 20)
                    }
                }
                EntityKind::Drop => {
                    let drop = self.world.drop_snapshot(e.handle).expect("live drop");
                    (
                        match drop.reward.kind {
                            DropKind::Point => resources::STAR,
                            DropKind::Power => resources::HEART,
                            DropKind::Bomb => resources::BOMB,
                        },
                        16.0,
                        16.0,
                        e.rgba,
                        25,
                    )
                }
            };
            Some(GameSprite {
                kind: e.handle.kind() as u32,
                slot: e.handle.slot(),
                generation: e.handle.generation(),
                resource_id,
                x: e.position.x.to_f32(),
                y: e.position.y.to_f32(),
                width,
                height,
                rgba,
                layer,
            })
        })
    }
    pub fn state_hash(&self) -> u64 {
        let mut hash = Fingerprint::new();
        hash.u32(self.protocol_version());
        hash.u64(self.resources.content_hash());
        hash.u64(S::CONTENT_ID);
        hash.u64(self.stage.state_hash());
        hash.u64(self.initial_stage.state_hash());
        hash.u64(self.seed);
        hash.u64(self.world.state_hash());
        for v in [
            self.config.bombs,
            self.config.shot_interval,
            self.config.bomb_damage,
            self.bombs,
            self.shot_cooldown,
            self.flash,
            self.phase as u32,
            self.status.wave,
            self.status.boss_max_health,
            u32::from(self.status.complete),
        ] {
            hash.u32(v);
        }
        if let Some(h) = self.status.boss {
            hash.u32(h.kind() as u32);
            hash.u32(h.slot());
            hash.u32(h.generation());
        } else {
            hash.u32(u32::MAX);
        }
        hash.u64(self.score);
        hash.u32(self.input.x as u32);
        hash.u32(self.input.y as u32);
        for v in [
            self.input.fire,
            self.input.bomb,
            self.input.focus,
            self.input.restart,
        ] {
            hash.u32(u32::from(v));
        }
        if self.world.advanced_config().is_some() {
            hash.u32(self.power);
            hash.u64(self.phase_bonus);
        }
        hash.finish()
    }
}
pub(super) fn units(value: i32) -> Fixed {
    Fixed::from_int(value).expect("small content coordinate")
}
pub(super) fn point(x: i32, y: i32) -> Vec2 {
    Vec2::new(units(x), units(y))
}

/// Headless conformance uses the same stage with a larger health budget, so it
/// can exercise boss victory and repeated restarts without a human dodger.
pub fn conformance_game() -> Game {
    let mut config = GameConfig::default();
    config.simulation.projectile_capacity = 512;
    config.simulation.player.health = 10000;
    Game::with_stage(config, 42, ResourcePack::builtin(), DemoStage::default())
        .expect("bounded stage")
}
pub fn conformance_input(frame: u64) -> GameInput {
    GameInput {
        fire: true,
        focus: frame % 200 < 100,
        bomb: frame % 2400 == 100,
        restart: frame % 12000 == 11999,
        ..GameInput::default()
    }
}
pub fn trace(frames: u32) -> Vec<u64> {
    let mut game = conformance_game();
    (0..frames)
        .map(|frame| {
            game.step(conformance_input(u64::from(frame)))
                .expect("bounded content");
            game.state_hash()
        })
        .collect()
}
