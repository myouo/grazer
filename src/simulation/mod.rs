//! M1 headless simulation, protocol 2. The protocol-1 `Runtime` remains the M0
//! motion/presentation fixture. This core owns no clock, OS RNG or renderer.
//!
//! ```
//! use grazer::{SimulationConfig, Input};
//! use grazer::simulation::{ReplayRecorder, InputReplay};
//! let config = SimulationConfig { projectile_capacity: 64, enemy_capacity: 8,
//!     ..SimulationConfig::default() };
//! let mut recording = ReplayRecorder::new(config, 42)?;
//! recording.step(Input { x: 1, y: 0 })?;
//! let expected = recording.simulation().state_hash();
//! let bytes = recording.finish()?.to_bytes();
//! let restored = InputReplay::from_bytes(&bytes)?.play()?;
//! assert_eq!(restored.state_hash(), expected);
//! # Ok::<(), grazer::simulation::ReplayError>(())
//! ```

pub mod demo;
mod geometry;
mod pool;
pub mod replay;
pub use geometry::{Collider, InvalidCollider, MAX_CURVE_POINTS, Vec2};
pub use replay::{
    Command, CommandResult, InputReplay, REPLAY_VERSION, ReplayError, ReplayFrame, ReplayPlayer,
    ReplayRecorder,
};

use crate::{Fixed, Input};
use pool::Pool;

pub const SIMULATION_PROTOCOL_VERSION: u32 = 2;
pub const MAX_ENTITY_CAPACITY: u32 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimulationError {
    InvalidConfig,
    InvalidInput,
    InvalidEntity,
    InvalidHandle,
    Capacity,
    Exhausted,
    ArithmeticOverflow,
}
impl std::fmt::Display for SimulationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidConfig => "invalid simulation configuration",
            Self::InvalidInput => "input axes must be -1, 0 or 1",
            Self::InvalidEntity => "entity needs an in-bounds position, positive radius and health",
            Self::InvalidHandle => "entity handle is stale or belongs to a different entity kind",
            Self::Capacity => "entity capacity reached",
            Self::Exhausted => "tick or entity generation exhausted",
            Self::ArithmeticOverflow => "entity motion exceeds the Q16.16 coordinate range",
        })
    }
}
impl std::error::Error for SimulationError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum EntityKind {
    Player = 0,
    Enemy = 1,
    Projectile = 2,
}

/// Value handle, scoped to its simulation (and clones of that simulation).
/// Reused slots have a new generation. Never use handles across unrelated worlds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityHandle {
    kind: EntityKind,
    slot: u32,
    generation: u32,
}
impl EntityHandle {
    pub(crate) fn from_parts(kind: u32, slot: u32, generation: u32) -> Option<Self> {
        let kind = match kind {
            0 if slot == 0 && generation == 1 => EntityKind::Player,
            1 => EntityKind::Enemy,
            2 => EntityKind::Projectile,
            _ => return None,
        };
        if generation == 0 || slot >= MAX_ENTITY_CAPACITY {
            return None;
        }
        Some(Self {
            kind,
            slot,
            generation,
        })
    }
    pub const PLAYER: Self = Self {
        kind: EntityKind::Player,
        slot: 0,
        generation: 1,
    };
    pub fn kind(self) -> EntityKind {
        self.kind
    }
    pub fn slot(self) -> u32 {
        self.slot
    }
    pub fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerConfig {
    pub position: Vec2,
    pub radius: Fixed,
    /// Absolute outer radius, at least the hit radius. A projectile grazes once.
    pub graze_radius: Fixed,
    /// Units per tick per axis. Diagonal input is intentionally unnormalized.
    pub speed: Fixed,
    pub health: u32,
    /// A hit on tick T blocks damage on ticks T+1 through T+N, inclusive.
    pub invulnerability_ticks: u32,
}
impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            position: Vec2::new(Fixed::from_bits(320 << 16), Fixed::from_bits(400 << 16)),
            radius: Fixed::from_bits(2 << 16),
            graze_radius: Fixed::from_bits(12 << 16),
            speed: Fixed::from_bits(2 << 16),
            health: 3,
            invulnerability_ticks: 120,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimulationConfig {
    pub width: Fixed,
    pub height: Fixed,
    pub projectile_capacity: u32,
    pub enemy_capacity: u32,
    pub player: PlayerConfig,
}
impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            width: Fixed::from_bits(640 << 16),
            height: Fixed::from_bits(480 << 16),
            projectile_capacity: 100_000,
            enemy_capacity: 1024,
            player: PlayerConfig::default(),
        }
    }
}
impl SimulationConfig {
    pub(crate) fn contains(self, position: Vec2) -> bool {
        position.x.bits() >= 0
            && position.x < self.width
            && position.y.bits() >= 0
            && position.y < self.height
    }
    pub(crate) fn validate(self) -> Result<(), SimulationError> {
        if self.width.bits() <= 0
            || self.height.bits() <= 0
            || self.projectile_capacity == 0
            || self.projectile_capacity > MAX_ENTITY_CAPACITY
            || self.enemy_capacity == 0
            || self.enemy_capacity > MAX_ENTITY_CAPACITY
            || !self.contains(self.player.position)
            || self.player.radius.bits() <= 0
            || self.player.graze_radius < self.player.radius
            || self.player.speed.bits() < 0
            || self.player.health == 0
        {
            return Err(SimulationError::InvalidConfig);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Faction {
    Player = 0,
    Enemy = 1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BoundsBehavior {
    Despawn = 0,
    Keep = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Projectile {
    pub position: Vec2,
    pub velocity: Vec2,
    pub collider: Collider,
    pub faction: Faction,
    pub damage: u32,
    /// Number of steps, including the final collision pass. Zero means unlimited.
    pub lifetime: u32,
    pub bounds: BoundsBehavior,
    /// Presentation only; numeric 0xRRGGBBAA, also included in state hashes.
    pub rgba: u32,
}
impl Projectile {
    pub(crate) fn validate(&self, config: SimulationConfig) -> Result<(), SimulationError> {
        if !config.contains(self.position) {
            return Err(SimulationError::InvalidEntity);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Enemy {
    pub position: Vec2,
    pub velocity: Vec2,
    pub radius: Fixed,
    pub health: u32,
    pub contact_damage: u32,
    pub lifetime: u32,
    pub bounds: BoundsBehavior,
    pub rgba: u32,
}
impl Enemy {
    pub(crate) fn validate(&self, config: SimulationConfig) -> Result<(), SimulationError> {
        if !config.contains(self.position) || self.radius.bits() <= 0 || self.health == 0 {
            return Err(SimulationError::InvalidEntity);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerState {
    pub position: Vec2,
    pub previous_position: Vec2,
    pub health: u32,
    pub invulnerable_ticks: u32,
    /// Saturates at u64::MAX rather than wrapping.
    pub grazes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DespawnReason {
    Hit,
    HealthDepleted,
    Lifetime,
    OutOfBounds,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Actual damage, possibly zero due to invulnerability or a simultaneous kill.
    Hit {
        source: EntityHandle,
        target: EntityHandle,
        damage: u32,
    },
    Grazed {
        projectile: EntityHandle,
    },
    PlayerDied,
    Destroyed {
        entity: EntityHandle,
        reason: DespawnReason,
    },
}
/// Renderer-independent owned snapshot. Player/enemies have a circle collider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntitySnapshot {
    pub handle: EntityHandle,
    pub position: Vec2,
    pub previous_position: Vec2,
    pub collider: Collider,
    pub health: Option<u32>,
    pub rgba: u32,
}

#[derive(Clone)]
struct LiveProjectile {
    spec: Projectile,
    previous: Vec2,
    remaining: u32,
    grazed: bool,
    remove: Option<DespawnReason>,
}
#[derive(Clone)]
struct LiveEnemy {
    spec: Enemy,
    collider: Collider,
    previous: Vec2,
    remaining: u32,
    remove: Option<DespawnReason>,
}
#[derive(Clone, Copy)]
enum Contact {
    Hit {
        projectile: EntityHandle,
        target: EntityHandle,
    },
    Graze(EntityHandle),
    Body(EntityHandle),
}

/// Deterministic single-threaded simulation. Host commands execute between ticks.
/// Steps preflight all checked motion, then move, gather contacts in spawn order,
/// resolve damage/grazing, and stably compact destroyed entities. New and cloned
/// worlds reserve all working buffers; successful steps allocate no memory.
pub struct Simulation {
    config: SimulationConfig,
    tick: u64,
    rng: u64,
    input: Input,
    player: PlayerState,
    player_collider: Collider,
    projectiles: Pool<LiveProjectile>,
    enemies: Pool<LiveEnemy>,
    contacts: Vec<Contact>,
    events: Vec<Event>,
}
impl Clone for Simulation {
    fn clone(&self) -> Self {
        let mut contacts = Vec::with_capacity(self.contacts.capacity());
        contacts.extend_from_slice(&self.contacts);
        let mut events = Vec::with_capacity(self.events.capacity());
        events.extend_from_slice(&self.events);
        Self {
            config: self.config,
            tick: self.tick,
            rng: self.rng,
            input: self.input,
            player: self.player,
            player_collider: self.player_collider,
            projectiles: self.projectiles.clone(),
            enemies: self.enemies.clone(),
            contacts,
            events,
        }
    }
}
impl Simulation {
    pub fn new(config: SimulationConfig, seed: u64) -> Result<Self, SimulationError> {
        config.validate()?;
        let working_capacity = config.projectile_capacity as usize + config.enemy_capacity as usize;
        Ok(Self {
            config,
            tick: 0,
            rng: seed,
            input: Input::default(),
            player: PlayerState {
                position: config.player.position,
                previous_position: config.player.position,
                health: config.player.health,
                invulnerable_ticks: 0,
                grazes: 0,
            },
            player_collider: Collider::circle(config.player.radius)
                .expect("validated player radius"),
            projectiles: Pool::new(EntityKind::Projectile, config.projectile_capacity),
            enemies: Pool::new(EntityKind::Enemy, config.enemy_capacity),
            contacts: Vec::with_capacity(working_capacity),
            events: Vec::with_capacity(working_capacity * 2 + 1),
        })
    }
    pub fn config(&self) -> SimulationConfig {
        self.config
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn input(&self) -> Input {
        self.input
    }
    pub fn player(&self) -> PlayerState {
        self.player
    }
    pub fn projectile_count(&self) -> usize {
        self.projectiles.len()
    }
    pub fn enemy_count(&self) -> usize {
        self.enemies.len()
    }
    pub fn events(&self) -> &[Event] {
        &self.events
    }
    pub fn set_input(&mut self, input: Input) -> Result<(), SimulationError> {
        validate_input(input)?;
        self.input = input;
        Ok(())
    }
    pub fn random_u32(&mut self) -> u32 {
        self.rng = self.rng.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        ((z ^ (z >> 31)) >> 32) as u32
    }
    pub fn spawn_projectile(&mut self, spec: Projectile) -> Result<EntityHandle, SimulationError> {
        spec.validate(self.config)?;
        self.projectiles.insert(LiveProjectile {
            previous: spec.position,
            remaining: spec.lifetime,
            spec,
            grazed: false,
            remove: None,
        })
    }
    pub fn spawn_enemy(&mut self, spec: Enemy) -> Result<EntityHandle, SimulationError> {
        spec.validate(self.config)?;
        let collider = Collider::circle(spec.radius).expect("validated enemy radius");
        self.enemies.insert(LiveEnemy {
            previous: spec.position,
            remaining: spec.lifetime,
            spec,
            collider,
            remove: None,
        })
    }
    pub fn despawn(&mut self, handle: EntityHandle) -> Result<(), SimulationError> {
        match handle.kind {
            EntityKind::Projectile => self.projectiles.remove(handle),
            EntityKind::Enemy => self.enemies.remove(handle),
            EntityKind::Player => Err(SimulationError::InvalidHandle),
        }
    }
    pub fn projectile(&self, handle: EntityHandle) -> Option<&Projectile> {
        Some(&self.projectiles.get(handle)?.spec)
    }
    pub fn enemy(&self, handle: EntityHandle) -> Option<&Enemy> {
        Some(&self.enemies.get(handle)?.spec)
    }
    pub(crate) fn set_velocity(
        &mut self,
        handle: EntityHandle,
        velocity: Vec2,
    ) -> Result<(), SimulationError> {
        match handle.kind {
            EntityKind::Enemy => {
                self.enemies
                    .get_mut(handle)
                    .ok_or(SimulationError::InvalidHandle)?
                    .spec
                    .velocity = velocity
            }
            EntityKind::Projectile => {
                self.projectiles
                    .get_mut(handle)
                    .ok_or(SimulationError::InvalidHandle)?
                    .spec
                    .velocity = velocity
            }
            EntityKind::Player => return Err(SimulationError::InvalidHandle),
        }
        Ok(())
    }
    // M2 native game commands run inside a Game tick and are reproduced by its
    // versioned GameInput trace. They do not extend the M1 replay command ABI.
    pub(crate) fn clear_hostile_projectiles(&mut self) {
        self.projectiles
            .retain(|entry| entry.value.spec.faction != Faction::Enemy);
    }
    pub(crate) fn clear_enemies(&mut self) {
        self.enemies.retain(|_| false);
    }
    pub(crate) fn protect_player(&mut self, ticks: u32) {
        self.player.invulnerable_ticks = self.player.invulnerable_ticks.max(ticks);
    }
    pub(crate) fn damage_enemies(&mut self, amount: u32) {
        for entry in self.enemies.entries_mut() {
            let enemy = &mut entry.value;
            enemy.spec.health = enemy.spec.health.saturating_sub(amount);
            if enemy.spec.health == 0 {
                enemy.remove = Some(DespawnReason::HealthDepleted);
            }
        }
    }
    /// Player first, enemies in spawn order, then projectiles in spawn order.
    /// Hosts can reuse their own snapshot buffer with `clear` and `extend`.
    pub fn snapshots(&self) -> impl Iterator<Item = EntitySnapshot> + '_ {
        std::iter::once(EntitySnapshot {
            handle: EntityHandle::PLAYER,
            position: self.player.position,
            previous_position: self.player.previous_position,
            collider: self.player_collider,
            health: Some(self.player.health),
            rgba: 0xffffffff,
        })
        .chain(self.enemies.entries().iter().map(|e| EntitySnapshot {
            handle: e.handle,
            position: e.value.spec.position,
            previous_position: e.value.previous,
            collider: e.value.collider,
            health: Some(e.value.spec.health),
            rgba: e.value.spec.rgba,
        }))
        .chain(self.projectiles.entries().iter().map(|e| EntitySnapshot {
            handle: e.handle,
            position: e.value.spec.position,
            previous_position: e.value.previous,
            collider: e.value.spec.collider,
            health: None,
            rgba: e.value.spec.rgba,
        }))
    }
    fn preflight(&self) -> Result<u64, SimulationError> {
        let next = self.tick.checked_add(1).ok_or(SimulationError::Exhausted)?;
        for entry in self.projectiles.entries() {
            entry
                .value
                .spec
                .position
                .checked_add(entry.value.spec.velocity)
                .ok_or(SimulationError::ArithmeticOverflow)?;
        }
        for entry in self.enemies.entries() {
            entry
                .value
                .spec
                .position
                .checked_add(entry.value.spec.velocity)
                .ok_or(SimulationError::ArithmeticOverflow)?;
        }
        Ok(next)
    }
    pub fn step(&mut self) -> Result<(), SimulationError> {
        self.step_with_input(self.input)
    }

    /// A rejected step leaves input, events, entities, RNG and tick unchanged.
    pub fn step_with_input(&mut self, input: Input) -> Result<(), SimulationError> {
        self.step_with_speed(input, self.config.player.speed)
    }
    pub(crate) fn step_with_speed(
        &mut self,
        input: Input,
        speed: Fixed,
    ) -> Result<(), SimulationError> {
        validate_input(input)?;
        let next_tick = self.preflight()?;
        self.input = input;
        self.events.clear();
        self.contacts.clear();
        self.move_entities(speed);
        self.gather_contacts();
        self.resolve_contacts();
        self.commit_removals();
        self.tick = next_tick;
        Ok(())
    }
    fn move_entities(&mut self, player_speed: Fixed) {
        self.player.previous_position = self.player.position;
        if self.player.health > 0 {
            let speed = i64::from(player_speed.bits());
            let x = i64::from(self.player.position.x.bits()) + i64::from(self.input.x) * speed;
            let y = i64::from(self.player.position.y.bits()) + i64::from(self.input.y) * speed;
            self.player.position = Vec2::new(
                Fixed::from_bits(x.clamp(0, i64::from(self.config.width.bits()) - 1) as i32),
                Fixed::from_bits(y.clamp(0, i64::from(self.config.height.bits()) - 1) as i32),
            );
        }
        for entry in self.projectiles.entries_mut() {
            let e = &mut entry.value;
            e.previous = e.spec.position;
            e.spec.position = e
                .spec
                .position
                .checked_add(e.spec.velocity)
                .expect("preflight motion");
        }
        for entry in self.enemies.entries_mut() {
            let e = &mut entry.value;
            e.previous = e.spec.position;
            e.spec.position = e
                .spec
                .position
                .checked_add(e.spec.velocity)
                .expect("preflight motion");
        }
    }
    fn gather_contacts(&mut self) {
        for entry in self.projectiles.entries() {
            let p = &entry.value;
            match p.spec.faction {
                Faction::Enemy if self.player.health > 0 => {
                    if p.spec.collider.swept_contact(
                        p.previous,
                        p.spec.position,
                        self.player.previous_position,
                        self.player.position,
                        self.config.player.radius,
                    ) {
                        self.contacts.push(Contact::Hit {
                            projectile: entry.handle,
                            target: EntityHandle::PLAYER,
                        });
                    } else if !p.grazed
                        && p.spec.collider.swept_contact(
                            p.previous,
                            p.spec.position,
                            self.player.previous_position,
                            self.player.position,
                            self.config.player.graze_radius,
                        )
                    {
                        self.contacts.push(Contact::Graze(entry.handle));
                    }
                }
                Faction::Player => {
                    // One projectile hits the first contacted enemy in spawn
                    // order, not the earliest time of impact. Contacts use the
                    // pre-damage actor set, so simultaneous shots are consumed.
                    for enemy in self.enemies.entries() {
                        let e = &enemy.value;
                        if e.spec.health == 0 {
                            continue;
                        }
                        if p.spec.collider.swept_contact(
                            p.previous,
                            p.spec.position,
                            e.previous,
                            e.spec.position,
                            e.spec.radius,
                        ) {
                            self.contacts.push(Contact::Hit {
                                projectile: entry.handle,
                                target: enemy.handle,
                            });
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
        if self.player.health > 0 {
            for entry in self.enemies.entries() {
                let e = &entry.value;
                if e.spec.health == 0 {
                    continue;
                }
                if e.spec.contact_damage > 0
                    && e.collider.swept_contact(
                        e.previous,
                        e.spec.position,
                        self.player.previous_position,
                        self.player.position,
                        self.config.player.radius,
                    )
                {
                    self.contacts.push(Contact::Body(entry.handle));
                }
            }
        }
    }
    fn resolve_contacts(&mut self) {
        // Capture the timer before decrementing: N protects N subsequent ticks.
        let player_protected = self.player.invulnerable_ticks > 0;
        self.player.invulnerable_ticks = self.player.invulnerable_ticks.saturating_sub(1);
        let mut protected = player_protected;
        for index in 0..self.contacts.len() {
            match self.contacts[index] {
                Contact::Hit { projectile, target } => {
                    let p = self
                        .projectiles
                        .get_mut(projectile)
                        .expect("live contact source");
                    p.remove = Some(DespawnReason::Hit);
                    let damage = p.spec.damage;
                    if target == EntityHandle::PLAYER {
                        self.hit_player(projectile, damage, &mut protected);
                    } else {
                        let e = self.enemies.get_mut(target).expect("live contact target");
                        let applied = damage.min(e.spec.health);
                        e.spec.health -= applied;
                        if e.spec.health == 0 {
                            e.remove = Some(DespawnReason::HealthDepleted);
                        }
                        self.events.push(Event::Hit {
                            source: projectile,
                            target,
                            damage: applied,
                        });
                    }
                }
                Contact::Graze(projectile) if self.player.health > 0 => {
                    self.projectiles
                        .get_mut(projectile)
                        .expect("live graze source")
                        .grazed = true;
                    self.player.grazes = self.player.grazes.saturating_add(1);
                    self.events.push(Event::Grazed { projectile });
                }
                Contact::Body(enemy) => {
                    let e = self.enemies.get(enemy).expect("live contact source");
                    self.hit_player(enemy, e.spec.contact_damage, &mut protected);
                }
                _ => {}
            }
        }
    }
    fn hit_player(&mut self, source: EntityHandle, damage: u32, protected: &mut bool) {
        let applied = if *protected {
            0
        } else {
            damage.min(self.player.health)
        };
        self.player.health -= applied;
        self.events.push(Event::Hit {
            source,
            target: EntityHandle::PLAYER,
            damage: applied,
        });
        if applied > 0 {
            self.player.invulnerable_ticks = self.config.player.invulnerability_ticks;
            *protected = self.config.player.invulnerability_ticks > 0;
            if self.player.health == 0 {
                self.events.push(Event::PlayerDied);
            }
        }
    }
    fn commit_removals(&mut self) {
        for entry in self.projectiles.entries_mut() {
            let p = &mut entry.value;
            expire(&mut p.remaining, p.spec.lifetime, &mut p.remove);
            if p.remove.is_none()
                && p.spec.bounds == BoundsBehavior::Despawn
                && p.spec
                    .collider
                    .outside(p.spec.position, self.config.width, self.config.height)
            {
                p.remove = Some(DespawnReason::OutOfBounds);
            }
            if let Some(reason) = p.remove {
                self.events.push(Event::Destroyed {
                    entity: entry.handle,
                    reason,
                });
            }
        }
        for entry in self.enemies.entries_mut() {
            let e = &mut entry.value;
            expire(&mut e.remaining, e.spec.lifetime, &mut e.remove);
            if e.remove.is_none()
                && e.spec.bounds == BoundsBehavior::Despawn
                && e.collider
                    .outside(e.spec.position, self.config.width, self.config.height)
            {
                e.remove = Some(DespawnReason::OutOfBounds);
            }
            if let Some(reason) = e.remove {
                self.events.push(Event::Destroyed {
                    entity: entry.handle,
                    reason,
                });
            }
        }
        self.projectiles.retain(|e| e.value.remove.is_none());
        self.enemies.retain(|e| e.value.remove.is_none());
    }

    /// FNV-1a over fixed-width little-endian fields, including free-list order,
    /// generations, pending input, previous positions, lifetime and graze flags.
    /// Events/contacts and cached geometry are derived and excluded.
    pub fn state_hash(&self) -> u64 {
        let mut hash = StateHasher::new();
        hash.u32(SIMULATION_PROTOCOL_VERSION);
        hash.config(self.config);
        hash.u64(self.tick);
        hash.u64(self.rng);
        hash.input(self.input);
        hash.vector(self.player.position);
        hash.vector(self.player.previous_position);
        hash.u32(self.player.health);
        hash.u32(self.player.invulnerable_ticks);
        hash.u64(self.player.grazes);
        self.projectiles.hash_layout(&mut hash);
        for e in self.projectiles.entries() {
            hash.handle(e.handle);
            hash.projectile(&e.value.spec);
            hash.vector(e.value.previous);
            hash.u32(e.value.remaining);
            hash.u8(u8::from(e.value.grazed));
        }
        self.enemies.hash_layout(&mut hash);
        for e in self.enemies.entries() {
            hash.handle(e.handle);
            hash.enemy(&e.value.spec);
            hash.vector(e.value.previous);
            hash.u32(e.value.remaining);
        }
        hash.finish()
    }
}
fn expire(remaining: &mut u32, lifetime: u32, reason: &mut Option<DespawnReason>) {
    if lifetime > 0 {
        *remaining -= 1;
        if *remaining == 0 && reason.is_none() {
            *reason = Some(DespawnReason::Lifetime);
        }
    }
}
pub(crate) fn validate_input(input: Input) -> Result<(), SimulationError> {
    if !(-1..=1).contains(&input.x) || !(-1..=1).contains(&input.y) {
        return Err(SimulationError::InvalidInput);
    }
    Ok(())
}

pub(crate) struct StateHasher(u64);
impl StateHasher {
    fn new() -> Self {
        Self(0xcbf29ce484222325)
    }
    fn bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
    pub fn u8(&mut self, value: u8) {
        self.bytes(&[value]);
    }
    pub fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }
    fn fixed(&mut self, value: Fixed) {
        self.bytes(&value.bits().to_le_bytes());
    }
    fn vector(&mut self, value: Vec2) {
        self.fixed(value.x);
        self.fixed(value.y);
    }
    fn input(&mut self, value: Input) {
        self.bytes(&value.x.to_le_bytes());
        self.bytes(&value.y.to_le_bytes());
    }
    fn handle(&mut self, value: EntityHandle) {
        self.u8(value.kind as u8);
        self.u32(value.slot);
        self.u32(value.generation);
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
            geometry::Shape::Circle => self.u8(0),
            geometry::Shape::Capsule { start, end } => {
                self.u8(1);
                self.vector(*start);
                self.vector(*end);
            }
            geometry::Shape::Curve { points, len } => {
                self.u8(2);
                self.u8(*len);
                for &p in &points[..usize::from(*len)] {
                    self.vector(p);
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
    fn finish(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhausted_tick_is_atomic() {
        let mut world = Simulation::new(
            SimulationConfig {
                projectile_capacity: 1,
                enemy_capacity: 1,
                ..SimulationConfig::default()
            },
            0,
        )
        .unwrap();
        world.tick = u64::MAX;
        let before = world.state_hash();
        assert_eq!(
            world.step_with_input(Input { x: 1, y: 1 }),
            Err(SimulationError::Exhausted)
        );
        assert_eq!(before, world.state_hash());
    }
}
