use super::*;
use crate::{
    advanced::{AdvancedConfig, Difficulty, LaserTiming},
    checkpoint::{CHECKPOINT_VERSION, CheckpointError as E, MAX_CHECKPOINT_BYTES, Reader, Writer},
};
const MAGIC: &[u8; 8] = b"GZSIMST1";
fn write_reason(reason: Option<DespawnReason>, out: &mut Writer) {
    out.u8(match reason {
        None => 0,
        Some(DespawnReason::Hit) => 1,
        Some(DespawnReason::HealthDepleted) => 2,
        Some(DespawnReason::Lifetime) => 3,
        Some(DespawnReason::OutOfBounds) => 4,
    });
}
fn read_reason(input: &mut Reader<'_>) -> Result<Option<DespawnReason>, E> {
    Ok(match input.u8()? {
        0 => None,
        1 => Some(DespawnReason::Hit),
        2 => Some(DespawnReason::HealthDepleted),
        3 => Some(DespawnReason::Lifetime),
        4 => Some(DespawnReason::OutOfBounds),
        _ => return Err(E::Data("despawn reason")),
    })
}
fn write_motion(m: Option<Motion>, out: &mut Writer) {
    out.bool(m.is_some());
    if let Some(m) = m {
        out.vector(m.acceleration);
        out.fixed(m.turn_per_tick);
    }
}
fn read_motion(input: &mut Reader<'_>, advanced: bool) -> Result<Option<Motion>, E> {
    if input.bool()? {
        if !advanced {
            return Err(E::Version);
        }
        Ok(Some(Motion {
            acceleration: input.vector()?,
            turn_per_tick: input.fixed()?,
        }))
    } else {
        Ok(None)
    }
}
fn write_reward(r: DropReward, out: &mut Writer) {
    out.u32(r.kind as u32);
    out.u32(r.value);
}
fn read_reward(input: &mut Reader<'_>) -> Result<DropReward, E> {
    let kind = DropKind::from_u32(input.u32()?).ok_or(E::Data("drop kind"))?;
    let value = input.u32()?;
    if value == 0 {
        return Err(E::Data("drop value"));
    }
    Ok(DropReward { kind, value })
}
fn remaining(lifetime: u32, left: u32) -> Result<(), E> {
    if lifetime == 0 && left != 0 || lifetime > 0 && (left == 0 || left > lifetime) {
        Err(E::Data("remaining lifetime"))
    } else {
        Ok(())
    }
}
fn handle_bound(h: EntityHandle, c: SimulationConfig, advanced: Option<AdvancedConfig>) -> bool {
    match h.kind() {
        EntityKind::Player => h == EntityHandle::PLAYER,
        EntityKind::Enemy => h.slot() < c.enemy_capacity,
        EntityKind::Projectile => h.slot() < c.projectile_capacity,
        EntityKind::Drop => advanced.is_some_and(|a| h.slot() < a.drop_capacity),
    }
}
impl Simulation {
    pub fn checkpoint(&self) -> Result<Vec<u8>, E> {
        let mut out = Writer::new(MAGIC);
        out.u32(CHECKPOINT_VERSION);
        out.u32(self.protocol_version());
        out.config(self.config);
        out.u64(self.tick);
        out.u64(self.rng);
        out.u32(self.input.x as u32);
        out.u32(self.input.y as u32);
        out.vector(self.player.position);
        out.vector(self.player.previous_position);
        out.u32(self.player.health);
        out.u32(self.player.invulnerable_ticks);
        out.u64(self.player.grazes);
        out.bool(self.advanced.is_some());
        if let Some(s) = &self.advanced {
            out.u32(s.config.difficulty as u32);
            out.u32(s.config.drop_capacity);
        }
        self.projectiles.write_checkpoint(&mut out, |p, out| {
            out.projectile(p.spec);
            out.vector(p.previous);
            out.u32(p.remaining);
            out.bool(p.grazed);
            write_reason(p.remove, out);
            write_motion(p.motion, out);
            out.bool(p.laser.is_some());
            if let Some(l) = p.laser {
                for n in [l.timing.warmup, l.timing.active, l.timing.fade, l.age] {
                    out.u32(n);
                }
            }
        });
        self.enemies.write_checkpoint(&mut out, |e, out| {
            out.enemy(e.spec);
            out.vector(e.previous);
            out.u32(e.remaining);
            write_reason(e.remove, out);
            write_motion(e.motion, out);
            out.bool(e.drop.is_some());
            if let Some(r) = e.drop {
                write_reward(r, out);
            }
        });
        if let Some(s) = &self.advanced {
            for n in [s.cancelled, s.cancel_points, s.collected, s.phase_deadline] {
                out.u64(n);
            }
            out.u32(s.boss_phase);
            out.u32(s.phases_started);
            s.drops.write_checkpoint(&mut out, |d, out| {
                out.vector(d.position);
                out.vector(d.previous);
                write_reward(d.reward, out);
                out.u32(d.age);
                out.bool(d.remove);
            });
        }
        out.u32(self.events.len() as u32);
        for event in &self.events {
            match *event {
                Event::Hit {
                    source,
                    target,
                    damage,
                } => {
                    out.u8(0);
                    out.handle(source);
                    out.handle(target);
                    out.u32(damage);
                }
                Event::Grazed { projectile } => {
                    out.u8(1);
                    out.handle(projectile);
                }
                Event::PlayerDied => out.u8(2),
                Event::Destroyed { entity, reason } => {
                    out.u8(3);
                    out.handle(entity);
                    write_reason(Some(reason), &mut out);
                }
                Event::Collected {
                    entity,
                    kind,
                    value,
                } => {
                    out.u8(4);
                    out.handle(entity);
                    write_reward(DropReward { kind, value }, &mut out);
                }
            }
        }
        out.u64(self.state_hash());
        out.finish(MAX_CHECKPOINT_BYTES)
    }
    pub fn restore_checkpoint(bytes: &[u8]) -> Result<Self, E> {
        let mut input = Reader::new(bytes, MAGIC, MAX_CHECKPOINT_BYTES)?;
        if input.u32()? != CHECKPOINT_VERSION {
            return Err(E::Version);
        }
        let protocol = input.u32()?;
        let config = input.config()?;
        let tick = input.u64()?;
        let rng = input.u64()?;
        let pending_input = Input {
            x: input.u32()? as i32,
            y: input.u32()? as i32,
        };
        validate_input(pending_input).map_err(|_| E::Data("input"))?;
        let player = PlayerState {
            position: input.vector()?,
            previous_position: input.vector()?,
            health: input.u32()?,
            invulnerable_ticks: input.u32()?,
            grazes: input.u64()?,
        };
        if !config.contains(player.position)
            || !config.contains(player.previous_position)
            || player.health > config.player.health
        {
            return Err(E::Data("player state"));
        }
        let advanced = if input.bool()? {
            let difficulty = Difficulty::from_u32(input.u32()?).ok_or(E::Data("difficulty"))?;
            let drop_capacity = input.u32()?;
            if drop_capacity == 0 || drop_capacity > MAX_ENTITY_CAPACITY {
                return Err(E::TooLarge);
            }
            Some(AdvancedConfig {
                difficulty,
                drop_capacity,
            })
        } else {
            None
        };
        if protocol
            != if advanced.is_some() {
                advanced::ADVANCED_PROTOCOL_VERSION
            } else {
                SIMULATION_PROTOCOL_VERSION
            }
        {
            return Err(E::Version);
        }
        let projectiles = Pool::read_checkpoint(
            EntityKind::Projectile,
            config.projectile_capacity,
            &mut input,
            |input| {
                let spec = input.projectile()?;
                let previous = input.vector()?;
                let left = input.u32()?;
                remaining(spec.lifetime, left)?;
                let grazed = input.bool()?;
                let remove = read_reason(input)?;
                let motion = read_motion(input, advanced.is_some())?;
                let laser = if input.bool()? {
                    if advanced.is_none()
                        || spec.faction != Faction::Enemy
                        || spec.bounds != BoundsBehavior::Keep
                        || matches!(spec.collider.shape, geometry::Shape::Circle)
                    {
                        return Err(E::Data("laser shape/mode"));
                    }
                    let timing = LaserTiming {
                        warmup: input.u32()?,
                        active: input.u32()?,
                        fade: input.u32()?,
                    };
                    let age = input.u32()?;
                    let total = timing
                        .warmup
                        .checked_add(timing.active)
                        .and_then(|n| n.checked_add(timing.fade))
                        .ok_or(E::Data("laser timing"))?;
                    if timing.active == 0
                        || total != spec.lifetime
                        || age >= total
                        || left != total - age
                    {
                        return Err(E::Data("laser age/lifetime"));
                    }
                    Some(LiveLaser { timing, age })
                } else {
                    None
                };
                Ok(LiveProjectile {
                    spec,
                    previous,
                    remaining: left,
                    grazed,
                    remove,
                    motion,
                    laser,
                })
            },
        )?;
        if projectiles
            .entries()
            .iter()
            .filter(|p| p.value.laser.is_some())
            .count()
            > advanced::MAX_LASERS
        {
            return Err(E::TooLarge);
        }
        let enemies = Pool::read_checkpoint(
            EntityKind::Enemy,
            config.enemy_capacity,
            &mut input,
            |input| {
                let spec = input.enemy()?;
                let previous = input.vector()?;
                let left = input.u32()?;
                remaining(spec.lifetime, left)?;
                let remove = read_reason(input)?;
                if spec.health == 0 && remove != Some(DespawnReason::HealthDepleted) {
                    return Err(E::Data("dead enemy state"));
                }
                let motion = read_motion(input, advanced.is_some())?;
                let drop = if input.bool()? {
                    if advanced.is_none() {
                        return Err(E::Version);
                    }
                    Some(read_reward(input)?)
                } else {
                    None
                };
                Ok(LiveEnemy {
                    spec,
                    collider: Collider::circle(spec.radius).map_err(|_| E::Data("enemy radius"))?,
                    previous,
                    remaining: left,
                    remove,
                    motion,
                    drop,
                })
            },
        )?;
        let advanced = if let Some(advanced_config) = advanced {
            let cancelled = input.u64()?;
            let cancel_points = input.u64()?;
            let collected = input.u64()?;
            let phase_deadline = input.u64()?;
            let boss_phase = input.u32()?;
            let phases_started = input.u32()?;
            if boss_phase == 0 && phase_deadline != 0 || phases_started == 0 && boss_phase != 0 {
                return Err(E::Data("phase state"));
            }
            let drops = Pool::read_checkpoint(
                EntityKind::Drop,
                advanced_config.drop_capacity,
                &mut input,
                |input| {
                    let d = LiveDrop {
                        position: input.vector()?,
                        previous: input.vector()?,
                        reward: read_reward(input)?,
                        age: input.u32()?,
                        remove: input.bool()?,
                    };
                    if !config.contains(d.position)
                        || !config.contains(d.previous)
                        || d.age >= 600
                        || d.remove
                    {
                        return Err(E::Data("drop state"));
                    }
                    Ok(d)
                },
            )?;
            if drops.available()
                < enemies
                    .entries()
                    .iter()
                    .filter(|e| e.value.drop.is_some())
                    .count()
            {
                return Err(E::Data("reserved death drops"));
            }
            Some(Box::new(AdvancedState {
                config: advanced_config,
                drops,
                cancelled,
                cancel_points,
                collected,
                boss_phase,
                phase_deadline,
                phases_started,
            }))
        } else {
            None
        };
        let advanced_config = advanced.as_ref().map(|s| s.config);
        let working = config.projectile_capacity as usize + config.enemy_capacity as usize;
        let event_capacity =
            working * 2 + 1 + advanced_config.map_or(0, |s| s.drop_capacity as usize);
        let n = input.count(event_capacity, 1)?;
        let mut events = Vec::with_capacity(event_capacity);
        for _ in 0..n {
            let event = match input.u8()? {
                0 => Event::Hit {
                    source: input.handle()?,
                    target: input.handle()?,
                    damage: input.u32()?,
                },
                1 => Event::Grazed {
                    projectile: input.handle()?,
                },
                2 => Event::PlayerDied,
                3 => Event::Destroyed {
                    entity: input.handle()?,
                    reason: read_reason(&mut input)?.ok_or(E::Data("destroy event"))?,
                },
                4 => {
                    let entity = input.handle()?;
                    let r = read_reward(&mut input)?;
                    if entity.kind() != EntityKind::Drop || advanced.is_none() {
                        return Err(E::Data("pickup event"));
                    }
                    Event::Collected {
                        entity,
                        kind: r.kind,
                        value: r.value,
                    }
                }
                _ => return Err(E::Data("event tag")),
            };
            let valid = match event {
                Event::Hit { source, target, .. } => {
                    handle_bound(source, config, advanced_config)
                        && handle_bound(target, config, advanced_config)
                }
                Event::Grazed { projectile } => {
                    projectile.kind() == EntityKind::Projectile
                        && handle_bound(projectile, config, advanced_config)
                }
                Event::Destroyed { entity, .. } | Event::Collected { entity, .. } => {
                    handle_bound(entity, config, advanced_config)
                }
                Event::PlayerDied => true,
            };
            if !valid {
                return Err(E::Data("event handle bounds"));
            }
            events.push(event);
        }
        let expected = input.u64()?;
        input.finish()?;
        let world = Self {
            config,
            tick,
            rng,
            input: pending_input,
            player,
            player_collider: Collider::circle(config.player.radius).expect("validated radius"),
            projectiles,
            enemies,
            contacts: Vec::with_capacity(working),
            events,
            advanced,
        };
        if world.state_hash() != expected {
            return Err(E::Fingerprint);
        }
        Ok(world)
    }
    /// Component fingerprints for first-divergence diagnosis; no allocations.
    pub fn component_hashes(&self) -> [u64; 4] {
        let mut player = StateHasher::new();
        player.vector(self.player.position);
        player.vector(self.player.previous_position);
        player.u32(self.player.health);
        player.u32(self.player.invulnerable_ticks);
        player.u64(self.player.grazes);
        let mut enemies = StateHasher::new();
        self.enemies.hash_layout(&mut enemies);
        for e in self.enemies.entries() {
            enemies.handle(e.handle);
            enemies.enemy(&e.value.spec);
            enemies.vector(e.value.previous);
            enemies.u32(e.value.remaining);
            enemies.motion(e.value.motion);
            if let Some(r) = e.value.drop {
                enemies.u8(1);
                enemies.u32(r.kind as u32);
                enemies.u32(r.value);
            } else {
                enemies.u8(0);
            }
        }
        let mut shots = StateHasher::new();
        self.projectiles.hash_layout(&mut shots);
        for p in self.projectiles.entries() {
            shots.handle(p.handle);
            shots.projectile(&p.value.spec);
            shots.vector(p.value.previous);
            shots.u32(p.value.remaining);
            shots.u8(u8::from(p.value.grazed));
            shots.motion(p.value.motion);
            shots.u8(u8::from(p.value.laser.is_some()));
            if let Some(l) = p.value.laser {
                for n in [l.timing.warmup, l.timing.active, l.timing.fade, l.age] {
                    shots.u32(n);
                }
            }
        }
        let mut drops = StateHasher::new();
        if let Some(s) = &self.advanced {
            s.hash(&mut drops);
        } else {
            drops.u8(0);
        }
        [
            player.finish(),
            enemies.finish(),
            shots.finish(),
            drops.finish(),
        ]
    }
    pub(crate) fn prepare_practice(&mut self) {
        self.player.health = self.config.player.health;
        self.player.position = self.config.player.position;
        self.player.previous_position = self.player.position;
        self.player.invulnerable_ticks = 120;
        self.player.grazes = 0;
        self.projectiles
            .retain(|e| e.value.spec.faction == Faction::Enemy);
        if let Some(s) = &mut self.advanced {
            s.drops.retain(|_| false);
            s.cancelled = 0;
            s.cancel_points = 0;
            s.collected = 0;
        }
        self.events.clear();
        self.contacts.clear();
    }
}
