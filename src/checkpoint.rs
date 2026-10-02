//! Bounded, versioned binary checkpoint support. Fingerprints detect accidental
//! corruption/divergence; they are not cryptographic authenticity checks.
use crate::{
    BoundsBehavior, Collider, Enemy, EntityHandle, Faction, Fixed, PlayerConfig, Projectile,
    SimulationConfig, Vec2,
};
pub const CHECKPOINT_VERSION: u32 = 1;
pub const MAX_CHECKPOINT_BYTES: usize = 64 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckpointError {
    Version,
    TooLarge,
    Data(&'static str),
    Resource,
    Stage,
    Fingerprint,
    Script(crate::language::Diagnostic),
}
impl std::fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Version => f.write_str("unsupported checkpoint/protocol version"),
            Self::TooLarge => f.write_str("checkpoint size or capacity limit exceeded"),
            Self::Data(message) => write!(f, "invalid checkpoint: {message}"),
            Self::Resource => {
                f.write_str("checkpoint resource summary does not match this resource pack")
            }
            Self::Stage => f.write_str("checkpoint stage/program does not match this host"),
            Self::Fingerprint => f.write_str("checkpoint fingerprint mismatch"),
            Self::Script(diagnostic) => write!(f, "checkpoint VM: {diagnostic}"),
        }
    }
}
impl std::error::Error for CheckpointError {}
pub(crate) struct Writer(pub Vec<u8>);
impl Writer {
    pub fn new(magic: &[u8; 8]) -> Self {
        Self(magic.to_vec())
    }
    pub fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    pub fn bool(&mut self, v: bool) {
        self.u8(u8::from(v));
    }
    pub fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn fixed(&mut self, v: Fixed) {
        self.u32(v.bits() as u32);
    }
    pub fn vector(&mut self, v: Vec2) {
        self.fixed(v.x);
        self.fixed(v.y);
    }
    pub fn handle(&mut self, h: EntityHandle) {
        self.u8(h.kind() as u8);
        self.u32(h.slot());
        self.u32(h.generation());
    }
    pub fn optional_handle(&mut self, h: Option<EntityHandle>) {
        self.bool(h.is_some());
        if let Some(h) = h {
            self.handle(h);
        }
    }
    pub fn blob(&mut self, bytes: &[u8]) {
        self.u32(bytes.len() as u32);
        self.0.extend_from_slice(bytes);
    }
    pub fn text(&mut self, value: &str) {
        self.blob(value.as_bytes());
    }
    pub fn config(&mut self, c: SimulationConfig) {
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
    pub fn collider(&mut self, c: Collider) {
        use crate::simulation::geometry::Shape;
        self.fixed(c.radius());
        match c.shape {
            Shape::Circle => self.u8(0),
            Shape::Capsule { start, end } => {
                self.u8(1);
                self.vector(start);
                self.vector(end);
            }
            Shape::Curve { points, len } => {
                self.u8(2);
                self.u8(len);
                for &p in &points[..usize::from(len)] {
                    self.vector(p);
                }
            }
        }
    }
    pub fn projectile(&mut self, p: Projectile) {
        self.vector(p.position);
        self.vector(p.velocity);
        self.collider(p.collider);
        self.u8(p.faction as u8);
        self.u32(p.damage);
        self.u32(p.lifetime);
        self.u8(p.bounds as u8);
        self.u32(p.rgba);
    }
    pub fn enemy(&mut self, e: Enemy) {
        self.vector(e.position);
        self.vector(e.velocity);
        self.fixed(e.radius);
        self.u32(e.health);
        self.u32(e.contact_damage);
        self.u32(e.lifetime);
        self.u8(e.bounds as u8);
        self.u32(e.rgba);
    }
    pub fn finish(mut self, limit: usize) -> Result<Vec<u8>, CheckpointError> {
        if self.0.len() > limit.saturating_sub(8) {
            return Err(CheckpointError::TooLarge);
        }
        let hash = fingerprint(&self.0);
        self.u64(hash);
        Ok(self.0)
    }
}
pub(crate) fn fingerprint(bytes: &[u8]) -> u64 {
    let mut h = crate::resources::Fingerprint::new();
    h.bytes(bytes);
    h.finish()
}
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8], magic: &[u8; 8], limit: usize) -> Result<Self, CheckpointError> {
        if bytes.len() > limit {
            return Err(CheckpointError::TooLarge);
        }
        if bytes.len() < 16 || &bytes[..8] != magic {
            return Err(CheckpointError::Data("magic/truncation"));
        }
        let end = bytes.len() - 8;
        let saved = u64::from_le_bytes(bytes[end..].try_into().expect("eight bytes"));
        if fingerprint(&bytes[..end]) != saved {
            return Err(CheckpointError::Fingerprint);
        }
        Ok(Self {
            bytes: &bytes[..end],
            offset: 8,
        })
    }
    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }
    pub fn finish(&self) -> Result<(), CheckpointError> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(CheckpointError::Data("trailing bytes"))
        }
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], CheckpointError> {
        let end = self
            .offset
            .checked_add(n)
            .ok_or(CheckpointError::TooLarge)?;
        let out = self
            .bytes
            .get(self.offset..end)
            .ok_or(CheckpointError::Data("truncated data"))?;
        self.offset = end;
        Ok(out)
    }
    pub fn u8(&mut self) -> Result<u8, CheckpointError> {
        Ok(self.take(1)?[0])
    }
    pub fn bool(&mut self) -> Result<bool, CheckpointError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CheckpointError::Data("boolean")),
        }
    }
    pub fn u32(&mut self) -> Result<u32, CheckpointError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four bytes"),
        ))
    }
    pub fn u64(&mut self) -> Result<u64, CheckpointError> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("eight bytes"),
        ))
    }
    pub fn fixed(&mut self) -> Result<Fixed, CheckpointError> {
        Ok(Fixed::from_bits(self.u32()? as i32))
    }
    pub fn vector(&mut self) -> Result<Vec2, CheckpointError> {
        Ok(Vec2::new(self.fixed()?, self.fixed()?))
    }
    pub fn handle(&mut self) -> Result<EntityHandle, CheckpointError> {
        let kind = self.u8()?;
        let slot = self.u32()?;
        let generation = self.u32()?;
        EntityHandle::from_parts(u32::from(kind), slot, generation)
            .ok_or(CheckpointError::Data("entity handle"))
    }
    pub fn optional_handle(&mut self) -> Result<Option<EntityHandle>, CheckpointError> {
        if self.bool()? {
            Ok(Some(self.handle()?))
        } else {
            Ok(None)
        }
    }
    pub fn count(&mut self, max: usize, min_bytes: usize) -> Result<usize, CheckpointError> {
        let n = self.u32()? as usize;
        if n > max || n > self.remaining() / min_bytes.max(1) {
            return Err(CheckpointError::TooLarge);
        }
        Ok(n)
    }
    pub fn blob(&mut self, max: usize) -> Result<&'a [u8], CheckpointError> {
        let n = self.count(max, 1)?;
        self.take(n)
    }
    pub fn text(&mut self, max: usize) -> Result<String, CheckpointError> {
        std::str::from_utf8(self.blob(max)?)
            .map(str::to_owned)
            .map_err(|_| CheckpointError::Data("UTF-8"))
    }
    pub fn config(&mut self) -> Result<SimulationConfig, CheckpointError> {
        let c = SimulationConfig {
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
        };
        c.validate()
            .map_err(|_| CheckpointError::Data("simulation configuration"))?;
        Ok(c)
    }
    pub fn collider(&mut self) -> Result<Collider, CheckpointError> {
        let radius = self.fixed()?;
        let result = match self.u8()? {
            0 => Collider::circle(radius),
            1 => Collider::capsule(self.vector()?, self.vector()?, radius),
            2 => {
                let n = usize::from(self.u8()?);
                if !(2..=crate::simulation::MAX_CURVE_POINTS).contains(&n) {
                    return Err(CheckpointError::Data("curve length"));
                }
                let mut points = [Vec2::ZERO; crate::simulation::MAX_CURVE_POINTS];
                for p in &mut points[..n] {
                    *p = self.vector()?;
                }
                Collider::curve(&points[..n], radius)
            }
            _ => return Err(CheckpointError::Data("collider tag")),
        };
        result.map_err(|_| CheckpointError::Data("collider radius"))
    }
    pub fn bounds(&mut self) -> Result<BoundsBehavior, CheckpointError> {
        match self.u8()? {
            0 => Ok(BoundsBehavior::Despawn),
            1 => Ok(BoundsBehavior::Keep),
            _ => Err(CheckpointError::Data("bounds tag")),
        }
    }
    pub fn projectile(&mut self) -> Result<Projectile, CheckpointError> {
        let position = self.vector()?;
        let velocity = self.vector()?;
        let collider = self.collider()?;
        let faction = match self.u8()? {
            0 => Faction::Player,
            1 => Faction::Enemy,
            _ => return Err(CheckpointError::Data("faction")),
        };
        Ok(Projectile {
            position,
            velocity,
            collider,
            faction,
            damage: self.u32()?,
            lifetime: self.u32()?,
            bounds: self.bounds()?,
            rgba: self.u32()?,
        })
    }
    pub fn enemy(&mut self) -> Result<Enemy, CheckpointError> {
        let e = Enemy {
            position: self.vector()?,
            velocity: self.vector()?,
            radius: self.fixed()?,
            health: self.u32()?,
            contact_damage: self.u32()?,
            lifetime: self.u32()?,
            bounds: self.bounds()?,
            rgba: self.u32()?,
        };
        if e.radius.bits() <= 0 {
            return Err(CheckpointError::Data("enemy radius"));
        }
        Ok(e)
    }
}
