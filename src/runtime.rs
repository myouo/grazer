use crate::Fixed;

/// Changes whenever authoritative update or hash semantics change.
pub const PROTOCOL_VERSION: u32 = 1;
pub const TICK_RATE: u32 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidConfig,
    InvalidInput,
    InvalidBullet,
    Capacity,
    Exhausted,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidConfig => "invalid playfield or capacity (maximum 1,000,000)",
            Self::InvalidInput => "input axes must be -1, 0 or 1",
            Self::InvalidBullet => "bullet must be inside playfield with positive radius",
            Self::Capacity => "bullet capacity reached",
            Self::Exhausted => "tick or entity identifier exhausted",
        })
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub width: Fixed,
    pub height: Fixed,
    pub capacity: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            width: Fixed::from_bits(640 << 16),
            height: Fixed::from_bits(480 << 16),
            capacity: 100_000,
        }
    }
}

/// Persistent desired axes, applied by the next `step`. Diagonal movement is
/// intentionally unnormalized in protocol 1; each axis moves two units/tick.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Input {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct Bullet {
    pub x: Fixed,
    pub y: Fixed,
    /// Units per tick, not units per second.
    pub vx: Fixed,
    pub vy: Fixed,
    pub radius: Fixed,
    /// Packed 0xRRGGBBAA, independent of host endianness.
    pub rgba: u32,
}

/// Owned presentation data. ID zero is reserved for the player. M0 uses solid
/// circles; texture/material resources and lasers are later protocol additions.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DrawSprite {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub rgba: u32,
}

#[derive(Clone)]
struct Entity {
    id: u32,
    bullet: Bullet,
}

/// Single-thread-driven, headless simulation. No clocks, OS RNG or graphics.
/// `Clone` is an in-process checkpoint, not a stable serialized save format.
#[derive(Clone)]
pub struct Runtime {
    config: Config,
    tick: u64,
    rng: u64,
    next_id: u32,
    input: Input,
    player: [Fixed; 2],
    bullets: Vec<Entity>,
}

impl Runtime {
    pub fn new(config: Config, seed: u64) -> Result<Self, Error> {
        if config.width.bits() <= 0
            || config.height.bits() <= 0
            || config.capacity == 0
            || config.capacity > 1_000_000
        {
            return Err(Error::InvalidConfig);
        }
        Ok(Self {
            config,
            tick: 0,
            rng: seed,
            next_id: 1,
            input: Input::default(),
            player: [
                Fixed::from_bits(config.width.bits() / 2),
                Fixed::from_bits(config.height.bits() / 2),
            ],
            bullets: Vec::with_capacity(config.capacity as usize),
        })
    }
    pub fn config(&self) -> Config {
        self.config
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn bullet_count(&self) -> usize {
        self.bullets.len()
    }
    pub fn set_input(&mut self, input: Input) -> Result<(), Error> {
        if !(-1..=1).contains(&input.x) || !(-1..=1).contains(&input.y) {
            return Err(Error::InvalidInput);
        }
        self.input = input;
        Ok(())
    }
    /// SplitMix64, specified with wrapping u64 arithmetic on every platform.
    pub fn random_u32(&mut self) -> u32 {
        self.rng = self.rng.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        ((z ^ (z >> 31)) >> 32) as u32
    }
    pub fn spawn(&mut self, bullet: Bullet) -> Result<u32, Error> {
        if bullet.x.bits() < 0
            || bullet.x >= self.config.width
            || bullet.y.bits() < 0
            || bullet.y >= self.config.height
            || bullet.radius.bits() <= 0
        {
            return Err(Error::InvalidBullet);
        }
        if self.bullets.len() >= self.config.capacity as usize {
            return Err(Error::Capacity);
        }
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or(Error::Exhausted)?;
        self.bullets.push(Entity { id, bullet });
        Ok(id)
    }
    /// Validate the entire batch before changing state. Returns the first ID;
    /// IDs are consecutive. An empty batch returns the next available ID.
    pub fn spawn_batch(&mut self, bullets: &[Bullet]) -> Result<u32, Error> {
        let count = u32::try_from(bullets.len()).map_err(|_| Error::Capacity)?;
        if bullets.len() > self.config.capacity as usize - self.bullets.len() {
            return Err(Error::Capacity);
        }
        self.next_id.checked_add(count).ok_or(Error::Exhausted)?;
        for b in bullets {
            if b.x.bits() < 0
                || b.x >= self.config.width
                || b.y.bits() < 0
                || b.y >= self.config.height
                || b.radius.bits() <= 0
            {
                return Err(Error::InvalidBullet);
            }
        }
        let first = self.next_id;
        for &b in bullets {
            self.spawn(b)?;
        }
        Ok(first)
    }
    /// Protocol 1: validate tick, apply persistent input, move bullets in spawn
    /// order with Euclidean wrapping, then increment tick. No per-tick allocation.
    pub fn step(&mut self) -> Result<(), Error> {
        let tick = self.tick.checked_add(1).ok_or(Error::Exhausted)?;
        for (axis, extent) in [self.config.width, self.config.height]
            .into_iter()
            .enumerate()
        {
            let input = if axis == 0 {
                self.input.x
            } else {
                self.input.y
            };
            let pos = i64::from(self.player[axis].bits()) + i64::from(input) * 2 * 65536;
            self.player[axis] = Fixed::from_bits(pos.clamp(0, i64::from(extent.bits()) - 1) as i32);
        }
        let width = i64::from(self.config.width.bits());
        let height = i64::from(self.config.height.bits());
        for entity in &mut self.bullets {
            let b = &mut entity.bullet;
            b.x = Fixed::from_bits(
                (i64::from(b.x.bits()) + i64::from(b.vx.bits())).rem_euclid(width) as i32,
            );
            b.y = Fixed::from_bits(
                (i64::from(b.y.bits()) + i64::from(b.vy.bits())).rem_euclid(height) as i32,
            );
        }
        self.tick = tick;
        Ok(())
    }
    pub fn sprites(&self) -> impl ExactSizeIterator<Item = DrawSprite> + '_ {
        (0..self.bullets.len() + 1).map(|i| {
            if i == 0 {
                return DrawSprite {
                    id: 0,
                    x: self.player[0].to_f32(),
                    y: self.player[1].to_f32(),
                    radius: 4.0,
                    rgba: 0xffffffff,
                };
            }
            let e = &self.bullets[i - 1];
            DrawSprite {
                id: e.id,
                x: e.bullet.x.to_f32(),
                y: e.bullet.y.to_f32(),
                radius: e.bullet.radius.to_f32(),
                rgba: e.bullet.rgba,
            }
        })
    }
    /// FNV-1a over protocol-tagged, fixed-width little-endian state. This is a
    /// divergence diagnostic, not a cryptographic integrity or security check.
    pub fn state_hash(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        let mut bytes = |value: &[u8]| {
            for byte in value {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        };
        bytes(&PROTOCOL_VERSION.to_le_bytes());
        bytes(&self.config.width.bits().to_le_bytes());
        bytes(&self.config.height.bits().to_le_bytes());
        bytes(&self.config.capacity.to_le_bytes());
        bytes(&self.tick.to_le_bytes());
        bytes(&self.rng.to_le_bytes());
        bytes(&self.next_id.to_le_bytes());
        bytes(&self.input.x.to_le_bytes());
        bytes(&self.input.y.to_le_bytes());
        for pos in self.player {
            bytes(&pos.bits().to_le_bytes());
        }
        bytes(&(self.bullets.len() as u32).to_le_bytes());
        for e in &self.bullets {
            bytes(&e.id.to_le_bytes());
            for v in [
                e.bullet.x,
                e.bullet.y,
                e.bullet.vx,
                e.bullet.vy,
                e.bullet.radius,
            ] {
                bytes(&v.bits().to_le_bytes());
            }
            bytes(&e.bullet.rgba.to_le_bytes());
        }
        hash
    }
}
