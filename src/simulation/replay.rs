//! Minimal versioned input/command replay. Recording and byte encoding allocate;
//! playback steps reuse simulation buffers. Asset metadata and serialized
//! checkpoints belong to M5. Every frame verifies its recorded state hash.

use super::{
    BoundsBehavior, Collider, Enemy, EntityHandle, EntityKind, Faction, PlayerConfig, Projectile,
    SIMULATION_PROTOCOL_VERSION, Simulation, SimulationConfig, SimulationError, Vec2,
    geometry::Shape, validate_input,
};
use crate::{Fixed, Input};

pub const REPLAY_VERSION: u32 = 1;
pub const MAX_REPLAY_FRAMES: usize = 1_000_000;
pub const MAX_COMMANDS_PER_FRAME: usize = 1_000_000;
const MAGIC: &[u8; 8] = b"GRZREP01";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    SpawnProjectile(Projectile),
    SpawnEnemy(Enemy),
    Despawn(EntityHandle),
    /// Record every host RNG draw that affects authoritative state.
    RandomU32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandResult {
    Spawned(EntityHandle),
    Despawned,
    RandomU32(u32),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayFrame {
    pub input: Input,
    pub commands: Vec<Command>,
    pub state_hash: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputReplay {
    config: SimulationConfig,
    seed: u64,
    frames: Vec<ReplayFrame>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayError {
    Simulation(SimulationError),
    UnsupportedVersion,
    InvalidData,
    TooLarge,
    PendingCommands,
    HashMismatch {
        tick: u64,
        expected: u64,
        actual: u64,
    },
    Stopped,
}
impl From<SimulationError> for ReplayError {
    fn from(error: SimulationError) -> Self {
        Self::Simulation(error)
    }
}
impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Simulation(error) => write!(f, "replay simulation: {error}"),
            Self::UnsupportedVersion => {
                f.write_str("unsupported replay or simulation protocol version")
            }
            Self::InvalidData => f.write_str("invalid or truncated replay data"),
            Self::TooLarge => f.write_str("replay frame or command limit exceeded"),
            Self::PendingCommands => {
                f.write_str("step once to record pending commands before finishing")
            }
            Self::HashMismatch {
                tick,
                expected,
                actual,
            } => write!(
                f,
                "replay diverged at tick {tick}: expected {expected:016x}, got {actual:016x}"
            ),
            Self::Stopped => f.write_str("replay playback stopped after an error"),
        }
    }
}
impl std::error::Error for ReplayError {}

pub(super) fn apply(
    world: &mut Simulation,
    command: Command,
) -> Result<CommandResult, SimulationError> {
    match command {
        Command::SpawnProjectile(spec) => world.spawn_projectile(spec).map(CommandResult::Spawned),
        Command::SpawnEnemy(spec) => world.spawn_enemy(spec).map(CommandResult::Spawned),
        Command::Despawn(handle) => world.despawn(handle).map(|()| CommandResult::Despawned),
        Command::RandomU32 => Ok(CommandResult::RandomU32(world.random_u32())),
    }
}

pub struct ReplayRecorder {
    replay: InputReplay,
    world: Simulation,
    pending: Vec<Command>,
}
impl ReplayRecorder {
    pub fn new(config: SimulationConfig, seed: u64) -> Result<Self, ReplayError> {
        Ok(Self {
            replay: InputReplay {
                config,
                seed,
                frames: Vec::new(),
            },
            world: Simulation::new(config, seed)?,
            pending: Vec::new(),
        })
    }
    pub fn simulation(&self) -> &Simulation {
        &self.world
    }
    /// Execute a boundary command and log it only if accepted. Returned handles
    /// can be used by subsequent despawn commands and reproduce in playback.
    pub fn command(&mut self, command: Command) -> Result<CommandResult, ReplayError> {
        if self.pending.len() == MAX_COMMANDS_PER_FRAME
            || self.replay.frames.len() == MAX_REPLAY_FRAMES
        {
            return Err(ReplayError::TooLarge);
        }
        let result = apply(&mut self.world, command)?;
        self.pending.push(command);
        Ok(result)
    }
    pub fn step(&mut self, input: Input) -> Result<u64, ReplayError> {
        if self.replay.frames.len() == MAX_REPLAY_FRAMES {
            return Err(ReplayError::TooLarge);
        }
        self.world.step_with_input(input)?;
        let state_hash = self.world.state_hash();
        self.replay.frames.push(ReplayFrame {
            input,
            commands: std::mem::take(&mut self.pending),
            state_hash,
        });
        Ok(state_hash)
    }
    pub fn finish(self) -> Result<InputReplay, ReplayError> {
        if !self.pending.is_empty() {
            return Err(ReplayError::PendingCommands);
        }
        Ok(self.replay)
    }
}

pub struct ReplayPlayer<'a> {
    replay: &'a InputReplay,
    world: Simulation,
    next: usize,
    failed: bool,
}
impl<'a> ReplayPlayer<'a> {
    pub fn new(replay: &'a InputReplay) -> Result<Self, ReplayError> {
        Ok(Self {
            replay,
            world: Simulation::new(replay.config, replay.seed)?,
            next: 0,
            failed: false,
        })
    }
    pub fn simulation(&self) -> &Simulation {
        &self.world
    }
    /// Returns false at EOF. A malformed command or hash mismatch stops the
    /// player permanently; the failing frame can have partially applied commands.
    pub fn step(&mut self) -> Result<bool, ReplayError> {
        if self.failed {
            return Err(ReplayError::Stopped);
        }
        let Some(frame) = self.replay.frames.get(self.next) else {
            return Ok(false);
        };
        let result = (|| {
            for &command in &frame.commands {
                apply(&mut self.world, command)?;
            }
            self.world.step_with_input(frame.input)?;
            let actual = self.world.state_hash();
            if actual != frame.state_hash {
                return Err(ReplayError::HashMismatch {
                    tick: self.world.tick(),
                    expected: frame.state_hash,
                    actual,
                });
            }
            Ok(true)
        })();
        if result.is_err() {
            self.failed = true;
        } else {
            self.next += 1;
        }
        result
    }
}
impl InputReplay {
    pub fn config(&self) -> SimulationConfig {
        self.config
    }
    pub fn seed(&self) -> u64 {
        self.seed
    }
    pub fn frames(&self) -> &[ReplayFrame] {
        &self.frames
    }
    pub fn play(&self) -> Result<Simulation, ReplayError> {
        let mut player = ReplayPlayer::new(self)?;
        while player.step()? {}
        Ok(player.world)
    }
    /// Little-endian binary format; independent replay and simulation versions.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Writer(Vec::new());
        out.0.extend_from_slice(MAGIC);
        out.u32(REPLAY_VERSION);
        out.u32(SIMULATION_PROTOCOL_VERSION);
        out.config(self.config);
        out.u64(self.seed);
        out.u32(self.frames.len() as u32);
        for frame in &self.frames {
            out.i32(frame.input.x);
            out.i32(frame.input.y);
            out.u32(frame.commands.len() as u32);
            for command in &frame.commands {
                match command {
                    Command::SpawnProjectile(spec) => {
                        out.u8(0);
                        out.projectile(spec);
                    }
                    Command::SpawnEnemy(spec) => {
                        out.u8(1);
                        out.enemy(spec);
                    }
                    Command::Despawn(handle) => {
                        out.u8(2);
                        out.u8(handle.kind as u8);
                        out.u32(handle.slot);
                        out.u32(handle.generation);
                    }
                    Command::RandomU32 => out.u8(3),
                }
            }
            out.u64(frame.state_hash);
        }
        out.0
    }
    /// Rejects incompatible versions, invalid values, oversized declared counts,
    /// truncation and trailing bytes. Playback also validates handles and hashes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ReplayError> {
        let mut input = Reader { bytes, offset: 0 };
        if input.take(8)? != MAGIC {
            return Err(ReplayError::InvalidData);
        }
        if input.u32()? != REPLAY_VERSION || input.u32()? != SIMULATION_PROTOCOL_VERSION {
            return Err(ReplayError::UnsupportedVersion);
        }
        let config = input.config()?;
        config.validate().map_err(|_| ReplayError::InvalidData)?;
        let seed = input.u64()?;
        let count = input.u32()? as usize;
        if count > MAX_REPLAY_FRAMES {
            return Err(ReplayError::TooLarge);
        }
        // Every frame needs at least axes, command count and hash (20 bytes).
        if count > input.remaining() / 20 {
            return Err(ReplayError::InvalidData);
        }
        let mut frames = Vec::with_capacity(count);
        for _ in 0..count {
            let axes = Input {
                x: input.i32()?,
                y: input.i32()?,
            };
            validate_input(axes).map_err(|_| ReplayError::InvalidData)?;
            let command_count = input.u32()? as usize;
            if command_count > MAX_COMMANDS_PER_FRAME {
                return Err(ReplayError::TooLarge);
            }
            if command_count > input.remaining().saturating_sub(8) {
                return Err(ReplayError::InvalidData);
            }
            let mut commands = Vec::new();
            for _ in 0..command_count {
                let command = match input.u8()? {
                    0 => {
                        let spec = input.projectile()?;
                        spec.validate(config)
                            .map_err(|_| ReplayError::InvalidData)?;
                        Command::SpawnProjectile(spec)
                    }
                    1 => {
                        let spec = input.enemy()?;
                        spec.validate(config)
                            .map_err(|_| ReplayError::InvalidData)?;
                        Command::SpawnEnemy(spec)
                    }
                    2 => {
                        let kind = match input.u8()? {
                            1 => EntityKind::Enemy,
                            2 => EntityKind::Projectile,
                            _ => return Err(ReplayError::InvalidData),
                        };
                        let slot = input.u32()?;
                        let generation = input.u32()?;
                        let capacity = if kind == EntityKind::Enemy {
                            config.enemy_capacity
                        } else {
                            config.projectile_capacity
                        };
                        if slot >= capacity || generation == 0 {
                            return Err(ReplayError::InvalidData);
                        }
                        Command::Despawn(EntityHandle {
                            kind,
                            slot,
                            generation,
                        })
                    }
                    3 => Command::RandomU32,
                    _ => return Err(ReplayError::InvalidData),
                };
                commands.push(command);
            }
            frames.push(ReplayFrame {
                input: axes,
                commands,
                state_hash: input.u64()?,
            });
        }
        if input.remaining() != 0 {
            return Err(ReplayError::InvalidData);
        }
        Ok(Self {
            config,
            seed,
            frames,
        })
    }
}

struct Writer(Vec<u8>);
impl Writer {
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }
    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn i32(&mut self, value: i32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn fixed(&mut self, value: Fixed) {
        self.i32(value.bits());
    }
    fn vector(&mut self, v: Vec2) {
        self.fixed(v.x);
        self.fixed(v.y);
    }
    fn config(&mut self, c: SimulationConfig) {
        self.fixed(c.width);
        self.fixed(c.height);
        self.u32(c.projectile_capacity);
        self.u32(c.enemy_capacity);
        self.vector(c.player.position);
        self.fixed(c.player.radius);
        self.fixed(c.player.graze_radius);
        self.fixed(c.player.speed);
        self.u32(c.player.health);
        self.u32(c.player.invulnerability_ticks);
    }
    fn collider(&mut self, collider: &Collider) {
        self.fixed(collider.radius());
        match &collider.shape {
            Shape::Circle => self.u8(0),
            Shape::Capsule { start, end } => {
                self.u8(1);
                self.vector(*start);
                self.vector(*end);
            }
            Shape::Curve { points, len } => {
                self.u8(2);
                self.u8(*len);
                for &point in &points[..usize::from(*len)] {
                    self.vector(point);
                }
            }
        }
    }
    fn projectile(&mut self, p: &Projectile) {
        self.vector(p.position);
        self.vector(p.velocity);
        self.collider(&p.collider);
        self.u8(p.faction as u8);
        self.u32(p.damage);
        self.u32(p.lifetime);
        self.u8(p.bounds as u8);
        self.u32(p.rgba);
    }
    fn enemy(&mut self, e: &Enemy) {
        self.vector(e.position);
        self.vector(e.velocity);
        self.fixed(e.radius);
        self.u32(e.health);
        self.u32(e.contact_damage);
        self.u32(e.lifetime);
        self.u8(e.bounds as u8);
        self.u32(e.rgba);
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8], ReplayError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(ReplayError::InvalidData)?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(ReplayError::InvalidData)?;
        self.offset = end;
        Ok(bytes)
    }
    fn u8(&mut self) -> Result<u8, ReplayError> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, ReplayError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four bytes"),
        ))
    }
    fn i32(&mut self) -> Result<i32, ReplayError> {
        Ok(i32::from_le_bytes(
            self.take(4)?.try_into().expect("four bytes"),
        ))
    }
    fn u64(&mut self) -> Result<u64, ReplayError> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("eight bytes"),
        ))
    }
    fn fixed(&mut self) -> Result<Fixed, ReplayError> {
        Ok(Fixed::from_bits(self.i32()?))
    }
    fn vector(&mut self) -> Result<Vec2, ReplayError> {
        Ok(Vec2::new(self.fixed()?, self.fixed()?))
    }
    fn config(&mut self) -> Result<SimulationConfig, ReplayError> {
        Ok(SimulationConfig {
            width: self.fixed()?,
            height: self.fixed()?,
            projectile_capacity: self.u32()?,
            enemy_capacity: self.u32()?,
            player: PlayerConfig {
                position: self.vector()?,
                radius: self.fixed()?,
                graze_radius: self.fixed()?,
                speed: self.fixed()?,
                health: self.u32()?,
                invulnerability_ticks: self.u32()?,
            },
        })
    }
    fn collider(&mut self) -> Result<Collider, ReplayError> {
        let radius = self.fixed()?;
        let collider = match self.u8()? {
            0 => Collider::circle(radius),
            1 => Collider::capsule(self.vector()?, self.vector()?, radius),
            2 => {
                let len = usize::from(self.u8()?);
                if !(2..=super::MAX_CURVE_POINTS).contains(&len) {
                    return Err(ReplayError::InvalidData);
                }
                let mut points = [Vec2::ZERO; super::MAX_CURVE_POINTS];
                for point in &mut points[..len] {
                    *point = self.vector()?;
                }
                Collider::curve(&points[..len], radius)
            }
            _ => return Err(ReplayError::InvalidData),
        };
        collider.map_err(|_| ReplayError::InvalidData)
    }
    fn bounds(&mut self) -> Result<BoundsBehavior, ReplayError> {
        match self.u8()? {
            0 => Ok(BoundsBehavior::Despawn),
            1 => Ok(BoundsBehavior::Keep),
            _ => Err(ReplayError::InvalidData),
        }
    }
    fn projectile(&mut self) -> Result<Projectile, ReplayError> {
        Ok(Projectile {
            position: self.vector()?,
            velocity: self.vector()?,
            collider: self.collider()?,
            faction: match self.u8()? {
                0 => Faction::Player,
                1 => Faction::Enemy,
                _ => return Err(ReplayError::InvalidData),
            },
            damage: self.u32()?,
            lifetime: self.u32()?,
            bounds: self.bounds()?,
            rgba: self.u32()?,
        })
    }
    fn enemy(&mut self) -> Result<Enemy, ReplayError> {
        Ok(Enemy {
            position: self.vector()?,
            velocity: self.vector()?,
            radius: self.fixed()?,
            health: self.u32()?,
            contact_damage: self.u32()?,
            lifetime: self.u32()?,
            bounds: self.bounds()?,
            rgba: self.u32()?,
        })
    }
}
