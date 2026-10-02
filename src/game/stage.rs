use super::units;
use crate::{
    BoundsBehavior, Collider, Enemy, EntityHandle, EntityKind, Faction, Fixed, Projectile,
    Simulation, SimulationError, Vec2, resources::Fingerprint,
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StageStatus {
    pub wave: u32,
    pub boss: Option<EntityHandle>,
    pub boss_max_health: u32,
    pub complete: bool,
}
/// Native Rust stage SDK. Content must avoid clocks/OS RNG/unordered iteration.
/// Hash every persistent field that can affect subsequent update commands.
/// Reuse preallocated buffers if the stage must retain the built-in zero-tick-
/// allocation guarantee; arbitrary user stage code can otherwise allocate.
/// Errors stop Game rather than dropping spawns or silently skipping content.
pub trait Stage: Clone {
    const CONTENT_ID: u64;
    fn update(&mut self, world: &mut Simulation) -> Result<StageStatus, SimulationError>;
    fn state_hash(&self) -> u64;
    fn diagnostic(&self) -> Option<&crate::language::Diagnostic> {
        None
    }
    fn after_step(&mut self, _: &Simulation) {}
}
#[derive(Clone, Default)]
pub struct DemoStage {
    boss: Option<EntityHandle>,
}
impl Stage for DemoStage {
    const CONTENT_ID: u64 = 0x47525a4d32000001;
    fn update(&mut self, world: &mut Simulation) -> Result<StageStatus, SimulationError> {
        let tick = world.tick();
        let cfg = world.config();
        if tick == 7200 {
            world.clear_enemies();
            world.clear_hostile_projectiles();
            self.boss = Some(world.spawn_enemy(Enemy {
                position: Vec2::new(
                    Fixed::from_bits(cfg.width.bits() / 2),
                    Fixed::from_bits(cfg.height.bits() / 8),
                ),
                velocity: Vec2::ZERO,
                radius: units(24),
                health: 1000,
                contact_damage: 1,
                lifetime: 0,
                bounds: BoundsBehavior::Keep,
                rgba: 0xe99fffff,
            })?);
        }
        if let Some(handle) = self.boss {
            if let Some(boss) = world.enemy(handle).copied() {
                if tick.is_multiple_of(45) {
                    for x in [-2, -1, 0, 1, 2] {
                        spawn(
                            world,
                            boss.position,
                            Vec2::new(units(x), Fixed::from_bits(3 << 15)),
                        )?;
                    }
                }
                if tick.is_multiple_of(70) {
                    let target = world.player().position;
                    let dx = i64::from(target.x.bits()) - i64::from(boss.position.x.bits());
                    let dy = i64::from(target.y.bits()) - i64::from(boss.position.y.bits());
                    let scale = dx.abs().max(dy.abs()).max(1);
                    let vx = (dx * (3 << 16) / scale) as i32;
                    let vy = (dy * (3 << 16) / scale) as i32;
                    for offset in [-16384, 0, 16384] {
                        spawn(
                            world,
                            boss.position,
                            Vec2::new(Fixed::from_bits(vx + offset), Fixed::from_bits(vy)),
                        )?;
                    }
                }
            }
            return Ok(StageStatus {
                wave: 20,
                boss: Some(handle),
                boss_max_health: 1000,
                complete: world.enemy(handle).is_none(),
            });
        }
        if tick.is_multiple_of(360) {
            for column in [1, 3, 5] {
                let x = (i64::from(cfg.width.bits()) * column / 6) as i32;
                world.spawn_enemy(Enemy {
                    position: Vec2::new(
                        Fixed::from_bits(x),
                        Fixed::from_bits(cfg.height.bits() / 32),
                    ),
                    velocity: Vec2::new(Fixed::ZERO, Fixed::from_bits(5 << 14)),
                    radius: units(12),
                    health: 12,
                    contact_damage: 1,
                    lifetime: 440,
                    bounds: BoundsBehavior::Despawn,
                    rgba: 0xff9292ff,
                })?;
            }
        }
        if tick.is_multiple_of(60) {
            let mut origins = [Vec2::ZERO; 64];
            let mut count = 0;
            for entity in world
                .snapshots()
                .filter(|entity| entity.handle.kind() == EntityKind::Enemy)
            {
                if count == origins.len() {
                    return Err(SimulationError::Capacity);
                }
                origins[count] = entity.position;
                count += 1;
            }
            for &origin in &origins[..count] {
                for x in [-1, 0, 1] {
                    spawn(
                        world,
                        origin,
                        Vec2::new(units(x), Fixed::from_bits(5 << 15)),
                    )?;
                }
            }
        }
        Ok(StageStatus {
            wave: (tick / 360 + 1) as u32,
            ..StageStatus::default()
        })
    }
    fn state_hash(&self) -> u64 {
        let mut hash = Fingerprint::new();
        if let Some(h) = self.boss {
            hash.u32(h.slot());
            hash.u32(h.generation());
        } else {
            hash.u32(u32::MAX);
        }
        hash.finish()
    }
}
fn spawn(world: &mut Simulation, position: Vec2, velocity: Vec2) -> Result<(), SimulationError> {
    world.spawn_projectile(Projectile {
        position,
        velocity,
        collider: Collider::circle(units(3)).expect("positive radius"),
        faction: Faction::Enemy,
        damage: 1,
        lifetime: 420,
        bounds: BoundsBehavior::Despawn,
        rgba: 0xf58ac7ff,
    })?;
    Ok(())
}
