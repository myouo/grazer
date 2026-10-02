//! M4 creation primitives. Angles use Q16.16 turns: 0 points right and 0.25
//! points down. All authoritative trigonometry and geometry use integers.
use super::{pool::Pool, *};

pub const ADVANCED_PROTOCOL_VERSION: u32 = 4;
pub const MAX_PATTERN_BULLETS: u32 = 512;
pub const MAX_LASERS: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum Difficulty {
    Easy = 0,
    #[default]
    Normal = 1,
    Hard = 2,
}
impl Difficulty {
    pub fn from_u32(value: u32) -> Option<Self> {
        Some(match value {
            0 => Self::Easy,
            1 => Self::Normal,
            2 => Self::Hard,
            _ => return None,
        })
    }
    pub fn score_multiplier(self) -> u64 {
        self as u64 + 1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdvancedConfig {
    pub difficulty: Difficulty,
    pub drop_capacity: u32,
}
impl Default for AdvancedConfig {
    fn default() -> Self {
        Self {
            difficulty: Difficulty::Normal,
            drop_capacity: 512,
        }
    }
}

/// Integrate position with the current velocity, then add acceleration and
/// rotate the resulting velocity. Composed motion never rotates the collider;
/// lasers translate their fixed capsule/polyline along this motion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Motion {
    pub acceleration: Vec2,
    pub turn_per_tick: Fixed,
}
impl Motion {
    pub(crate) fn next_velocity(self, velocity: Vec2) -> Result<Vec2, SimulationError> {
        rotate(
            velocity
                .checked_add(self.acceleration)
                .ok_or(SimulationError::ArithmeticOverflow)?,
            self.turn_per_tick,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaserTiming {
    pub warmup: u32,
    pub active: u32,
    pub fade: u32,
}
impl LaserTiming {
    fn total(self) -> Option<u32> {
        self.warmup.checked_add(self.active)?.checked_add(self.fade)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum LaserPhase {
    Warning = 0,
    Active = 1,
    Fading = 2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LiveLaser {
    pub timing: LaserTiming,
    pub age: u32,
}
impl LiveLaser {
    pub fn phase(self) -> LaserPhase {
        if self.age < self.timing.warmup {
            LaserPhase::Warning
        } else if self.age < self.timing.warmup + self.timing.active {
            LaserPhase::Active
        } else {
            LaserPhase::Fading
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum DropKind {
    Point = 0,
    Power = 1,
    Bomb = 2,
}
impl DropKind {
    pub fn from_u32(value: u32) -> Option<Self> {
        Some(match value {
            0 => Self::Point,
            1 => Self::Power,
            2 => Self::Bomb,
            _ => return None,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropReward {
    pub kind: DropKind,
    pub value: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropSnapshot {
    pub handle: EntityHandle,
    pub position: Vec2,
    pub reward: DropReward,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LaserSegment {
    pub slot: u32,
    pub generation: u32,
    pub segment: u32,
    pub phase: u32,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub width: f32,
    pub rgba: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AdvancedMetrics {
    pub drops: u32,
    pub collected: u64,
    pub cancelled: u64,
    pub boss_phase: u32,
    pub phase_ticks: u32,
    pub phases_started: u32,
}
#[derive(Clone)]
pub(crate) struct LiveDrop {
    pub position: Vec2,
    pub previous: Vec2,
    pub reward: DropReward,
    pub age: u32,
    pub remove: bool,
}
#[derive(Clone)]
pub(crate) struct AdvancedState {
    pub config: AdvancedConfig,
    pub drops: Pool<LiveDrop>,
    pub cancelled: u64,
    pub cancel_points: u64,
    pub collected: u64,
    pub boss_phase: u32,
    pub phase_deadline: u64,
    pub phases_started: u32,
}
impl AdvancedState {
    pub fn new(config: AdvancedConfig) -> Self {
        Self {
            config,
            drops: Pool::new(EntityKind::Drop, config.drop_capacity),
            cancelled: 0,
            cancel_points: 0,
            collected: 0,
            boss_phase: 0,
            phase_deadline: 0,
            phases_started: 0,
        }
    }
    pub fn hash(&self, hash: &mut StateHasher) {
        hash.u32(self.config.difficulty as u32);
        hash.u32(self.config.drop_capacity);
        for value in [
            self.cancelled,
            self.cancel_points,
            self.collected,
            self.phase_deadline,
        ] {
            hash.u64(value);
        }
        hash.u32(self.boss_phase);
        hash.u32(self.phases_started);
        self.drops.hash_layout(hash);
        for entry in self.drops.entries() {
            hash.handle(entry.handle);
            hash.vector(entry.value.position);
            hash.vector(entry.value.previous);
            hash.u32(entry.value.reward.kind as u32);
            hash.u32(entry.value.reward.value);
            hash.u32(entry.value.age);
        }
    }
}

/// A regular ring or a centered fan (including endpoints). A spiral emitter
/// uses `Spiral` for successive spokes and advances its start angle each volley.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    Ring { angle: Fixed },
    Fan { angle: Fixed, spread: Fixed },
    Aimed { target: Vec2, spread: Fixed },
    Spiral { angle: Fixed, step: Fixed },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PatternShot {
    pub origin: Vec2,
    pub count: u32,
    pub speed: Fixed,
    pub radius: Fixed,
    pub lifetime: u32,
    pub rgba: u32,
}

/// Fixed lookup with linear interpolation; cardinal directions are exact.
pub fn polar(speed: Fixed, angle: Fixed) -> Result<Vec2, SimulationError> {
    if speed.bits() < 0 {
        return Err(SimulationError::InvalidEntity);
    }
    let a = angle.bits().rem_euclid(65536);
    let sine = sin(a);
    let cosine = sin((a + 16384) % 65536);
    Ok(Vec2::new(
        Fixed::from_bits((i64::from(speed.bits()) * i64::from(cosine) / 65536) as i32),
        Fixed::from_bits((i64::from(speed.bits()) * i64::from(sine) / 65536) as i32),
    ))
}
fn sin(angle: i32) -> i32 {
    let (offset, sign) = match angle / 16384 {
        0 => (angle, 1),
        1 => (32768 - angle, 1),
        2 => (angle - 32768, -1),
        _ => (65536 - angle, -1),
    };
    let index = (offset / 128) as usize;
    let value = if index == 128 {
        QUARTER_SINE[128]
    } else {
        QUARTER_SINE[index] + (QUARTER_SINE[index + 1] - QUARTER_SINE[index]) * (offset % 128) / 128
    };
    value * sign
}
pub fn rotate(vector: Vec2, angle: Fixed) -> Result<Vec2, SimulationError> {
    let direction = polar(Fixed::ONE, angle)?;
    let x = (i64::from(vector.x.bits()) * i64::from(direction.x.bits())
        - i64::from(vector.y.bits()) * i64::from(direction.y.bits()))
        / 65536;
    let y = (i64::from(vector.x.bits()) * i64::from(direction.y.bits())
        + i64::from(vector.y.bits()) * i64::from(direction.x.bits()))
        / 65536;
    Ok(Vec2::new(
        Fixed::from_bits(i32::try_from(x).map_err(|_| SimulationError::ArithmeticOverflow)?),
        Fixed::from_bits(i32::try_from(y).map_err(|_| SimulationError::ArithmeticOverflow)?),
    ))
}
/// Deterministic quadrant search, valid even across the entire Q16.16 range.
pub fn angle_to(from: Vec2, to: Vec2) -> Fixed {
    let dx = i64::from(to.x.bits()) - i64::from(from.x.bits());
    let dy = i64::from(to.y.bits()) - i64::from(from.y.bits());
    if dx == 0 && dy == 0 {
        return Fixed::ZERO;
    }
    let mut low = 0;
    let mut high = 16384;
    while low < high {
        let middle = (low + high) / 2;
        let v = polar(Fixed::ONE, Fixed::from_bits(middle)).expect("unit direction");
        if dx.abs() * i64::from(v.y.bits()) < dy.abs() * i64::from(v.x.bits()) {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    Fixed::from_bits(match (dx < 0, dy < 0) {
        (false, false) => low,
        (true, false) => 32768 - low,
        (true, true) => 32768 + low,
        (false, true) => (65536 - low) % 65536,
    })
}

impl Simulation {
    /// Opt in before the first spawn/tick. Allocates all M4 drop storage once.
    /// Legacy worlds keep their original protocol-2 hash and collision rules.
    pub fn enable_advanced(&mut self, config: AdvancedConfig) -> Result<(), SimulationError> {
        if self.tick != 0
            || self.enemy_count() != 0
            || self.projectile_count() != 0
            || self.advanced.is_some()
            || config.drop_capacity == 0
            || config.drop_capacity > MAX_ENTITY_CAPACITY
        {
            return Err(SimulationError::InvalidConfig);
        }
        self.events
            .reserve(self.events.capacity() + config.drop_capacity as usize);
        self.advanced = Some(Box::new(AdvancedState::new(config)));
        Ok(())
    }
    pub fn advanced_config(&self) -> Option<AdvancedConfig> {
        self.advanced.as_ref().map(|s| s.config)
    }
    pub fn difficulty(&self) -> Option<Difficulty> {
        self.advanced_config().map(|c| c.difficulty)
    }
    pub fn advanced_metrics(&self) -> Option<AdvancedMetrics> {
        self.advanced.as_ref().map(|s| AdvancedMetrics {
            drops: s.drops.len() as u32,
            collected: s.collected,
            cancelled: s.cancelled,
            boss_phase: s.boss_phase,
            phase_ticks: s
                .phase_deadline
                .saturating_sub(self.tick)
                .min(u64::from(u32::MAX)) as u32,
            phases_started: s.phases_started,
        })
    }
    pub(crate) fn require_advanced(&self) -> Result<(), SimulationError> {
        if self.advanced.is_some() {
            Ok(())
        } else {
            Err(SimulationError::InvalidConfig)
        }
    }
    pub fn emit_pattern(
        &mut self,
        pattern: Pattern,
        shot: PatternShot,
    ) -> Result<(), SimulationError> {
        self.require_advanced()?;
        if shot.count == 0
            || shot.count > MAX_PATTERN_BULLETS
            || shot.speed.bits() < 0
            || !self.config.contains(shot.origin)
        {
            return Err(SimulationError::InvalidEntity);
        }
        if self.projectiles.available() < shot.count as usize {
            return Err(SimulationError::Capacity);
        }
        let collider = Collider::circle(shot.radius).map_err(|_| SimulationError::InvalidEntity)?;
        let (base, step) = match pattern {
            Pattern::Ring { angle } => (i64::from(angle.bits()), 65536i64),
            Pattern::Fan { angle, spread } => (
                i64::from(angle.bits())
                    - if shot.count > 1 {
                        i64::from(spread.bits()) / 2
                    } else {
                        0
                    },
                i64::from(spread.bits()),
            ),
            Pattern::Aimed { target, spread } => (
                i64::from(angle_to(shot.origin, target).bits())
                    - if shot.count > 1 {
                        i64::from(spread.bits()) / 2
                    } else {
                        0
                    },
                i64::from(spread.bits()),
            ),
            Pattern::Spiral { angle, step } => (i64::from(angle.bits()), i64::from(step.bits())),
        };
        for i in 0..shot.count {
            let offset = match pattern {
                Pattern::Ring { .. } => step * i64::from(i) / i64::from(shot.count),
                Pattern::Spiral { .. } => step * i64::from(i),
                _ => {
                    if shot.count > 1 {
                        step * i64::from(i) / i64::from(shot.count - 1)
                    } else {
                        0
                    }
                }
            };
            let velocity = polar(
                shot.speed,
                Fixed::from_bits((base + offset).rem_euclid(65536) as i32),
            )?;
            self.spawn_projectile(Projectile {
                position: shot.origin,
                velocity,
                collider,
                faction: Faction::Enemy,
                damage: 1,
                lifetime: shot.lifetime,
                bounds: BoundsBehavior::Despawn,
                rgba: shot.rgba,
            })?;
        }
        Ok(())
    }
    pub fn compose_motion(
        &mut self,
        handle: EntityHandle,
        motion: Motion,
    ) -> Result<(), SimulationError> {
        self.require_advanced()?;
        match handle.kind {
            EntityKind::Enemy => {
                self.enemies
                    .get_mut(handle)
                    .ok_or(SimulationError::InvalidHandle)?
                    .motion = Some(motion)
            }
            EntityKind::Projectile => {
                self.projectiles
                    .get_mut(handle)
                    .ok_or(SimulationError::InvalidHandle)?
                    .motion = Some(motion)
            }
            _ => return Err(SimulationError::InvalidHandle),
        }
        Ok(())
    }
    pub fn colour(&mut self, handle: EntityHandle, rgba: u32) -> Result<(), SimulationError> {
        self.require_advanced()?;
        match handle.kind {
            EntityKind::Enemy => {
                self.enemies
                    .get_mut(handle)
                    .ok_or(SimulationError::InvalidHandle)?
                    .spec
                    .rgba = rgba
            }
            EntityKind::Projectile => {
                self.projectiles
                    .get_mut(handle)
                    .ok_or(SimulationError::InvalidHandle)?
                    .spec
                    .rgba = rgba
            }
            _ => return Err(SimulationError::InvalidHandle),
        }
        Ok(())
    }
    /// Persistent hostile beam: warnings and fade are harmless; active ticks
    /// damage without consuming the beam, including during player immunity.
    pub fn spawn_laser(
        &mut self,
        origin: Vec2,
        velocity: Vec2,
        collider: Collider,
        timing: LaserTiming,
        rgba: u32,
    ) -> Result<EntityHandle, SimulationError> {
        self.require_advanced()?;
        if timing.active == 0 || matches!(collider.shape, geometry::Shape::Circle) {
            return Err(SimulationError::InvalidEntity);
        }
        if self
            .projectiles
            .entries()
            .iter()
            .filter(|e| e.value.laser.is_some())
            .count()
            >= MAX_LASERS
        {
            return Err(SimulationError::Capacity);
        }
        let lifetime = timing.total().ok_or(SimulationError::InvalidEntity)?;
        let handle = self.spawn_projectile(Projectile {
            position: origin,
            velocity,
            collider,
            faction: Faction::Enemy,
            damage: 1,
            lifetime,
            bounds: BoundsBehavior::Keep,
            rgba,
        })?;
        self.projectiles.get_mut(handle).expect("new laser").laser =
            Some(LiveLaser { timing, age: 0 });
        Ok(handle)
    }
    pub fn drop_item(
        &mut self,
        position: Vec2,
        reward: DropReward,
    ) -> Result<EntityHandle, SimulationError> {
        if !self.config.contains(position) || reward.value == 0 {
            return Err(SimulationError::InvalidEntity);
        }
        let reserved = self
            .enemies
            .entries()
            .iter()
            .filter(|e| e.value.drop.is_some())
            .count();
        if self
            .advanced
            .as_ref()
            .ok_or(SimulationError::InvalidConfig)?
            .drops
            .available()
            <= reserved
        {
            return Err(SimulationError::Capacity);
        }
        self.advanced
            .as_mut()
            .ok_or(SimulationError::InvalidConfig)?
            .drops
            .insert(LiveDrop {
                position,
                previous: position,
                reward,
                age: 0,
                remove: false,
            })
    }
    pub fn enemy_drop(
        &mut self,
        enemy: EntityHandle,
        reward: DropReward,
    ) -> Result<(), SimulationError> {
        self.require_advanced()?;
        if reward.value == 0 {
            return Err(SimulationError::InvalidEntity);
        }
        let current = self
            .enemies
            .get(enemy)
            .ok_or(SimulationError::InvalidHandle)?;
        let reserved = self
            .enemies
            .entries()
            .iter()
            .filter(|e| e.value.drop.is_some())
            .count();
        if current.drop.is_none()
            && self
                .advanced
                .as_ref()
                .expect("advanced mode")
                .drops
                .available()
                <= reserved
        {
            return Err(SimulationError::Capacity);
        }
        self.enemies
            .get_mut(enemy)
            .ok_or(SimulationError::InvalidHandle)?
            .drop = Some(reward);
        Ok(())
    }
    pub fn drops(&self) -> impl Iterator<Item = DropSnapshot> + '_ {
        self.advanced
            .iter()
            .flat_map(|s| s.drops.entries())
            .map(|e| DropSnapshot {
                handle: e.handle,
                position: e.value.position,
                reward: e.value.reward,
            })
    }
    pub fn cancel_shots(&mut self, reward: bool) -> Result<u32, SimulationError> {
        let state = self
            .advanced
            .as_mut()
            .ok_or(SimulationError::InvalidConfig)?;
        let count = self
            .projectiles
            .entries()
            .iter()
            .filter(|e| e.value.spec.faction == Faction::Enemy)
            .count() as u32;
        state.cancelled = state.cancelled.saturating_add(u64::from(count));
        if reward {
            state.cancel_points = state.cancel_points.saturating_add(u64::from(count) * 5);
        }
        self.clear_hostile_projectiles();
        Ok(count)
    }
    pub fn begin_boss_phase(
        &mut self,
        enemy: EntityHandle,
        phase: u32,
        duration: u32,
    ) -> Result<(), SimulationError> {
        if phase == 0 || duration == 0 || self.enemy(enemy).is_none() {
            return Err(SimulationError::InvalidEntity);
        }
        let deadline = self
            .tick
            .checked_add(u64::from(duration))
            .ok_or(SimulationError::Exhausted)?;
        let state = self
            .advanced
            .as_mut()
            .ok_or(SimulationError::InvalidConfig)?;
        state.boss_phase = phase;
        state.phase_deadline = deadline;
        state.phases_started = state.phases_started.saturating_add(1);
        Ok(())
    }
    pub fn laser_phase(&self, entity: EntityHandle) -> Option<LaserPhase> {
        self.projectiles.get(entity)?.laser.map(LiveLaser::phase)
    }
    pub fn laser_segments(&self) -> impl Iterator<Item = LaserSegment> + '_ {
        self.projectiles
            .entries()
            .iter()
            .filter(|e| e.value.laser.is_some())
            .flat_map(|e| {
                e.value
                    .spec
                    .collider
                    .segments()
                    .enumerate()
                    .map(move |(i, (start, end))| LaserSegment {
                        slot: e.handle.slot(),
                        generation: e.handle.generation(),
                        segment: i as u32,
                        phase: e.value.laser.expect("laser filter").phase() as u32,
                        x1: e.value.spec.position.x.to_f32() + start.x.to_f32(),
                        y1: e.value.spec.position.y.to_f32() + start.y.to_f32(),
                        x2: e.value.spec.position.x.to_f32() + end.x.to_f32(),
                        y2: e.value.spec.position.y.to_f32() + end.y.to_f32(),
                        width: e.value.spec.collider.radius().to_f32() * 2.0,
                        rgba: e.value.spec.rgba,
                    })
            })
    }
    pub fn drop_snapshot(&self, handle: EntityHandle) -> Option<DropSnapshot> {
        let e = self.advanced.as_ref()?.drops.get(handle)?;
        Some(DropSnapshot {
            handle,
            position: e.position,
            reward: e.reward,
        })
    }
    pub(crate) fn take_cancel_points(&mut self) -> u64 {
        self.advanced
            .as_mut()
            .map_or(0, |s| std::mem::take(&mut s.cancel_points))
    }
}

/// 16-point quadratic Bezier, evaluated exactly as integer Bernstein weights.
pub fn bezier_collider(
    control: Vec2,
    end: Vec2,
    radius: Fixed,
) -> Result<Collider, SimulationError> {
    let mut points = [Vec2::ZERO; MAX_CURVE_POINTS];
    let n = (MAX_CURVE_POINTS - 1) as i64;
    for (i, point) in points.iter_mut().enumerate() {
        let t = i as i64;
        let component = |c: Fixed, e: Fixed| {
            Fixed::from_bits(
                ((2 * (n - t) * t * i64::from(c.bits()) + t * t * i64::from(e.bits())) / (n * n))
                    as i32,
            )
        };
        *point = Vec2::new(component(control.x, end.x), component(control.y, end.y));
    }
    Collider::curve(&points, radius).map_err(|_| SimulationError::InvalidEntity)
}

// round(sin(i * pi / 256) * 65536), i=0..128. This table is part of protocol 4.
const QUARTER_SINE: [i32; 129] = [
    0, 804, 1608, 2412, 3216, 4019, 4821, 5623, 6424, 7224, 8022, 8820, 9616, 10411, 11204, 11996,
    12785, 13573, 14359, 15143, 15924, 16703, 17479, 18253, 19024, 19792, 20557, 21320, 22078,
    22834, 23586, 24335, 25080, 25821, 26558, 27291, 28020, 28745, 29466, 30182, 30893, 31600,
    32303, 33000, 33692, 34380, 35062, 35738, 36410, 37076, 37736, 38391, 39040, 39683, 40320,
    40951, 41576, 42194, 42806, 43412, 44011, 44604, 45190, 45769, 46341, 46906, 47464, 48015,
    48559, 49095, 49624, 50146, 50660, 51166, 51665, 52156, 52639, 53114, 53581, 54040, 54491,
    54934, 55368, 55794, 56212, 56621, 57022, 57414, 57798, 58172, 58538, 58896, 59244, 59583,
    59914, 60235, 60547, 60851, 61145, 61429, 61705, 61971, 62228, 62476, 62714, 62943, 63162,
    63372, 63572, 63763, 63944, 64115, 64277, 64429, 64571, 64704, 64827, 64940, 65043, 65137,
    65220, 65294, 65358, 65413, 65457, 65492, 65516, 65531, 65536,
];
