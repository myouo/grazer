//! Complete Game/world/stage checkpoints with explicit resource/content binding.
use super::*;
use crate::{
    TICK_RATE,
    checkpoint::{CHECKPOINT_VERSION, CheckpointError as E, MAX_CHECKPOINT_BYTES, Reader, Writer},
    language::{Program, ScriptStage, Vm},
    resources::RESOURCE_VERSION,
};
const MAGIC: &[u8; 8] = b"GZGAME01";
const SCRIPT_MAGIC: &[u8; 8] = b"GZSTAGE1";
const NATIVE_MAGIC: &[u8; 8] = b"GZDEMO01";

/// Native stages implement this codec to participate in disk replay/checkpoints.
/// Include every stage field; use a new CONTENT_ID when changing its semantics.
pub trait CheckpointStage: Stage {
    fn save_stage(&self) -> Result<Vec<u8>, E>;
    fn restore_stage(bytes: &[u8]) -> Result<Self, E>;
    fn content_hash(&self) -> u64 {
        Self::CONTENT_ID
    }
    fn validate_world(&self, _world: &Simulation, _faulted: bool) -> Result<(), E> {
        Ok(())
    }
}
impl CheckpointStage for DemoStage {
    fn save_stage(&self) -> Result<Vec<u8>, E> {
        let mut out = Writer::new(NATIVE_MAGIC);
        out.u32(CHECKPOINT_VERSION);
        out.optional_handle(self.boss);
        out.finish(1024)
    }
    fn restore_stage(bytes: &[u8]) -> Result<Self, E> {
        let mut r = Reader::new(bytes, NATIVE_MAGIC, 1024)?;
        if r.u32()? != CHECKPOINT_VERSION {
            return Err(E::Version);
        }
        let boss = r.optional_handle()?;
        if boss.is_some_and(|h| h.kind() != EntityKind::Enemy) {
            return Err(E::Stage);
        }
        r.finish()?;
        Ok(Self { boss })
    }
}
impl CheckpointStage for ScriptStage {
    fn save_stage(&self) -> Result<Vec<u8>, E> {
        let mut out = Writer::new(SCRIPT_MAGIC);
        out.u32(CHECKPOINT_VERSION);
        out.blob(&self.vm().program().to_bytes());
        out.blob(&self.save());
        out.finish(MAX_CHECKPOINT_BYTES)
    }
    fn restore_stage(bytes: &[u8]) -> Result<Self, E> {
        let mut r = Reader::new(bytes, SCRIPT_MAGIC, MAX_CHECKPOINT_BYTES)?;
        if r.u32()? != CHECKPOINT_VERSION {
            return Err(E::Version);
        }
        let program = Arc::new(Program::from_bytes(r.blob(16 * 1024 * 1024)?).map_err(E::Script)?);
        let vm = Vm::restore(program, r.blob(32 * 1024 * 1024)?).map_err(E::Script)?;
        r.finish()?;
        Ok(Self::from_vm(vm))
    }
    fn content_hash(&self) -> u64 {
        self.vm().program().content_hash()
    }
    fn validate_world(&self, world: &Simulation, faulted: bool) -> Result<(), E> {
        if !faulted
            && self.vm().last_tick.map_or(world.tick() != 0, |t| {
                t.checked_add(1) != Some(world.tick())
            })
        {
            return Err(E::Data("VM/world tick pairing"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceSummary {
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub atlas_bytes: u32,
    pub sprites: u32,
    pub sounds: u32,
    pub content_hash: u64,
}
impl ResourceSummary {
    pub fn of(pack: &ResourcePack) -> Self {
        Self {
            version: RESOURCE_VERSION,
            width: pack.width(),
            height: pack.height(),
            atlas_bytes: pack.atlas().len() as u32,
            sprites: pack.sprites().len() as u32,
            sounds: pack.sounds().len() as u32,
            content_hash: pack.content_hash(),
        }
    }
    pub(crate) fn write(self, w: &mut Writer) {
        for n in [
            self.version,
            self.width,
            self.height,
            self.atlas_bytes,
            self.sprites,
            self.sounds,
        ] {
            w.u32(n);
        }
        w.u64(self.content_hash);
    }
    pub(crate) fn read(r: &mut Reader<'_>) -> Result<Self, E> {
        let s = Self {
            version: r.u32()?,
            width: r.u32()?,
            height: r.u32()?,
            atlas_bytes: r.u32()?,
            sprites: r.u32()?,
            sounds: r.u32()?,
            content_hash: r.u64()?,
        };
        if s.version != RESOURCE_VERSION {
            return Err(E::Version);
        }
        if s.width == 0
            || s.height == 0
            || s.width > 4096
            || s.height > 4096
            || u64::from(s.width) * u64::from(s.height) * 4 != u64::from(s.atlas_bytes)
            || s.sprites == 0
            || s.sounds == 0
        {
            return Err(E::Data("resource summary"));
        }
        Ok(s)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameIdentity {
    pub game_protocol: u32,
    pub simulation_protocol: u32,
    pub stage_id: u64,
    pub content_hash: u64,
    pub resources: ResourceSummary,
    pub config: GameConfig,
    pub advanced: Option<AdvancedConfig>,
    pub seed: u64,
}
impl GameIdentity {
    pub(crate) fn write(self, w: &mut Writer) {
        w.u32(self.game_protocol);
        w.u32(self.simulation_protocol);
        w.u64(self.stage_id);
        w.u64(self.content_hash);
        self.resources.write(w);
        w.config(self.config.simulation);
        w.u32(self.config.bombs);
        w.u32(self.config.shot_interval);
        w.u32(self.config.bomb_damage);
        w.bool(self.advanced.is_some());
        if let Some(c) = self.advanced {
            w.u32(c.difficulty as u32);
            w.u32(c.drop_capacity);
        }
        w.u64(self.seed);
    }
    pub(crate) fn read(r: &mut Reader<'_>) -> Result<Self, E> {
        let game_protocol = r.u32()?;
        let simulation_protocol = r.u32()?;
        let stage_id = r.u64()?;
        let content_hash = r.u64()?;
        let resources = ResourceSummary::read(r)?;
        let config = GameConfig {
            simulation: r.config()?,
            bombs: r.u32()?,
            shot_interval: r.u32()?,
            bomb_damage: r.u32()?,
        };
        if config.bombs > 99 || config.shot_interval == 0 {
            return Err(E::Data("game configuration"));
        }
        let advanced = if r.bool()? {
            let difficulty =
                crate::advanced::Difficulty::from_u32(r.u32()?).ok_or(E::Data("difficulty"))?;
            let drop_capacity = r.u32()?;
            if drop_capacity == 0 || drop_capacity > crate::simulation::MAX_ENTITY_CAPACITY {
                return Err(E::TooLarge);
            }
            Some(AdvancedConfig {
                difficulty,
                drop_capacity,
            })
        } else {
            None
        };
        if (game_protocol, simulation_protocol) != if advanced.is_some() { (4, 4) } else { (3, 2) }
        {
            return Err(E::Version);
        }
        Ok(Self {
            game_protocol,
            simulation_protocol,
            stage_id,
            content_hash,
            resources,
            config,
            advanced,
            seed: r.u64()?,
        })
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StateHashes {
    pub game: u64,
    pub world: u64,
    pub stage: u64,
    pub player: u64,
    pub enemies: u64,
    pub projectiles: u64,
    pub drops: u64,
}
impl StateHashes {
    pub(crate) fn write(self, w: &mut Writer) {
        for n in [
            self.game,
            self.world,
            self.stage,
            self.player,
            self.enemies,
            self.projectiles,
            self.drops,
        ] {
            w.u64(n);
        }
    }
    pub(crate) fn read(r: &mut Reader<'_>) -> Result<Self, E> {
        Ok(Self {
            game: r.u64()?,
            world: r.u64()?,
            stage: r.u64()?,
            player: r.u64()?,
            enemies: r.u64()?,
            projectiles: r.u64()?,
            drops: r.u64()?,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckpointInfo {
    pub identity: GameIdentity,
    pub tick: u64,
    pub hashes: StateHashes,
}
impl CheckpointInfo {
    pub fn inspect(bytes: &[u8]) -> Result<Self, E> {
        let mut r = Reader::new(bytes, MAGIC, MAX_CHECKPOINT_BYTES)?;
        Self::read(&mut r)
    }
    fn read(r: &mut Reader<'_>) -> Result<Self, E> {
        if r.u32()? != CHECKPOINT_VERSION || r.u32()? != TICK_RATE {
            return Err(E::Version);
        }
        Ok(Self {
            identity: GameIdentity::read(r)?,
            tick: r.u64()?,
            hashes: StateHashes::read(r)?,
        })
    }
}
impl<S: CheckpointStage> Game<S> {
    pub fn identity(&self) -> GameIdentity {
        GameIdentity {
            game_protocol: self.protocol_version(),
            simulation_protocol: self.world.protocol_version(),
            stage_id: S::CONTENT_ID,
            content_hash: self.stage.content_hash(),
            resources: ResourceSummary::of(&self.resources),
            config: self.config,
            advanced: self.world.advanced_config(),
            seed: self.seed,
        }
    }
    pub fn state_hashes(&self) -> StateHashes {
        let [player, enemies, projectiles, drops] = self.world.component_hashes();
        StateHashes {
            game: self.state_hash(),
            world: self.world.state_hash(),
            stage: self.stage.state_hash(),
            player,
            enemies,
            projectiles,
            drops,
        }
    }
    pub fn checkpoint(&self) -> Result<Vec<u8>, E> {
        let mut w = Writer::new(MAGIC);
        w.u32(CHECKPOINT_VERSION);
        w.u32(TICK_RATE);
        self.identity().write(&mut w);
        w.u64(self.world.tick());
        self.state_hashes().write(&mut w);
        w.blob(&self.stage.save_stage()?);
        w.blob(&self.initial_stage.save_stage()?);
        w.blob(&self.world.checkpoint()?);
        w.u32(self.status.wave);
        w.optional_handle(self.status.boss);
        w.u32(self.status.boss_max_health);
        w.bool(self.status.complete);
        w.u32(self.phase as u32);
        w.u32(self.input.x as u32);
        w.u32(self.input.y as u32);
        w.u32(self.input.flags());
        for n in [self.bombs, self.shot_cooldown, self.flash, self.power] {
            w.u32(n);
        }
        w.u64(self.score);
        w.u64(self.phase_bonus);
        w.u32(self.audio.len() as u32);
        for e in &self.audio {
            w.u32(e.resource_id);
            w.u32(e.sequence);
            w.u64(e.tick);
        }
        w.finish(MAX_CHECKPOINT_BYTES)
    }
    pub fn restore_checkpoint(resources: Arc<ResourcePack>, bytes: &[u8]) -> Result<Self, E> {
        let mut r = Reader::new(bytes, MAGIC, MAX_CHECKPOINT_BYTES)?;
        let info = CheckpointInfo::read(&mut r)?;
        let identity = info.identity;
        if identity.stage_id != S::CONTENT_ID {
            return Err(E::Stage);
        }
        if identity.resources != ResourceSummary::of(&resources) {
            return Err(E::Resource);
        }
        let stage = S::restore_stage(r.blob(MAX_CHECKPOINT_BYTES)?)?;
        let initial_stage = S::restore_stage(r.blob(MAX_CHECKPOINT_BYTES)?)?;
        if stage.content_hash() != identity.content_hash
            || initial_stage.content_hash() != identity.content_hash
        {
            return Err(E::Stage);
        }
        let world = Simulation::restore_checkpoint(r.blob(MAX_CHECKPOINT_BYTES)?)?;
        if world.config() != identity.config.simulation
            || world.advanced_config() != identity.advanced
            || world.tick() != info.tick
        {
            return Err(E::Data("world identity/tick"));
        }
        let status = StageStatus {
            wave: r.u32()?,
            boss: r.optional_handle()?,
            boss_max_health: r.u32()?,
            complete: r.bool()?,
        };
        if status.boss.is_some_and(|h| {
            h.kind() != EntityKind::Enemy || h.slot() >= identity.config.simulation.enemy_capacity
        }) {
            return Err(E::Data("Boss handle"));
        }
        let phase = match r.u32()? {
            0 => GamePhase::Playing,
            1 => GamePhase::GameOver,
            2 => GamePhase::Cleared,
            3 => GamePhase::Faulted,
            _ => return Err(E::Data("game phase")),
        };
        let input = GameInput::from_flags(r.u32()? as i32, r.u32()? as i32, r.u32()?)
            .map_err(|_| E::Data("game input"))?;
        let bombs = r.u32()?;
        let shot_cooldown = r.u32()?;
        let flash = r.u32()?;
        let power = r.u32()?;
        let score = r.u64()?;
        let phase_bonus = r.u64()?;
        if bombs > identity.config.bombs.max(9)
            || shot_cooldown > identity.config.shot_interval
            || flash > 30
            || power > 4
            || phase_bonus > score
            || identity.advanced.is_none() && (power != 0 || phase_bonus != 0)
        {
            return Err(E::Data("game counters"));
        }
        let capacity = identity.config.simulation.enemy_capacity as usize + 16;
        let n = r.count(capacity, 16)?;
        let mut audio = Vec::with_capacity(capacity);
        for i in 0..n {
            let e = AudioEvent {
                resource_id: r.u32()?,
                sequence: r.u32()?,
                tick: r.u64()?,
            };
            if e.sequence != i as u32
                || e.tick != world.tick()
                || !(1..=8).contains(&e.resource_id)
                || resources.sound(e.resource_id).is_none()
            {
                return Err(E::Data("audio event"));
            }
            audio.push(e);
        }
        r.finish()?;
        stage.validate_world(&world, phase == GamePhase::Faulted)?;
        let game = Self {
            config: identity.config,
            seed: identity.seed,
            resources,
            world,
            stage,
            initial_stage,
            status,
            phase,
            input,
            bombs,
            score,
            shot_cooldown,
            flash,
            audio,
            power,
            phase_bonus,
        };
        if game.state_hashes() != info.hashes {
            return Err(E::Fingerprint);
        }
        Ok(game)
    }
}
