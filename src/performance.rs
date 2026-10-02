//! Reproducible full-frame stress fixture; timing is supplied only by the host.
use crate::{
    BoundsBehavior, Collider, Faction, Fixed, Input, PlayerConfig, Projectile, Simulation,
    SimulationConfig, SimulationError, Vec2,
};
pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;
pub fn scene(count: u32, seed: u64) -> Result<Simulation, SimulationError> {
    let mut world = Simulation::new(
        SimulationConfig {
            width: units(WIDTH as i32),
            height: units(HEIGHT as i32),
            projectile_capacity: count.max(1),
            enemy_capacity: 1,
            player: PlayerConfig {
                position: Vec2::new(units(960), units(960)),
                health: u32::MAX,
                invulnerability_ticks: 2,
                ..PlayerConfig::default()
            },
        },
        seed,
    )?;
    replenish(&mut world, count)?;
    Ok(world)
}
pub fn replenish(world: &mut Simulation, count: u32) -> Result<(), SimulationError> {
    if count > world.config().projectile_capacity {
        return Err(SimulationError::Capacity);
    }
    let collider = Collider::circle(units(2)).expect("positive radius");
    while world.projectile_count() < count as usize {
        let x = Fixed::from_bits((world.random_u32() % world.config().width.bits() as u32) as i32);
        let y = Fixed::from_bits((world.random_u32() % world.config().height.bits() as u32) as i32);
        let vx = Fixed::from_bits((world.random_u32() % 196609) as i32 - 98304);
        let vy = Fixed::from_bits((world.random_u32() % 196609) as i32 - 98304);
        world.spawn_projectile(Projectile {
            position: Vec2::new(x, y),
            velocity: Vec2::new(vx, vy),
            collider,
            faction: Faction::Enemy,
            damage: 1,
            lifetime: 0,
            bounds: BoundsBehavior::Despawn,
            rgba: 0x72dce8c0,
        })?;
    }
    Ok(())
}
pub fn input(tick: u64) -> Input {
    Input {
        x: match tick % 480 {
            0..=119 => 1,
            240..=359 => -1,
            _ => 0,
        },
        y: match tick % 360 {
            0..=89 => -1,
            180..=269 => 1,
            _ => 0,
        },
    }
}
fn units(v: i32) -> Fixed {
    Fixed::from_int(v).expect("fixture coordinate")
}
