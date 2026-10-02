//! Versioned M1 conformance and performance workloads, independent of rendering.
use super::{
    BoundsBehavior, Collider, Command, Enemy, EntityKind, Faction, InputReplay, PlayerConfig,
    Projectile, ReplayError, ReplayRecorder, Simulation, SimulationConfig, SimulationError, Vec2,
};
use crate::Fixed;

fn units(value: i32) -> Fixed {
    Fixed::from_int(value).expect("small fixture coordinate")
}
fn point(x: i32, y: i32) -> Vec2 {
    Vec2::new(units(x), units(y))
}

pub fn fixture_config() -> SimulationConfig {
    SimulationConfig {
        projectile_capacity: 64,
        enemy_capacity: 8,
        player: PlayerConfig {
            health: 1_000_000,
            invulnerability_ticks: 2,
            ..PlayerConfig::default()
        },
        ..SimulationConfig::default()
    }
}
pub fn fixture() -> Simulation {
    Simulation::new(fixture_config(), 42).expect("valid M1 fixture")
}

/// Fixed-size command buffer: repeated recycling, all three collider types,
/// hostile/friendly hits, body contacts, grazing, expiry, culling and RNG draws.
pub fn fixture_commands(world: &Simulation) -> [Option<Command>; 6] {
    let tick = world.tick();
    let config = world.config();
    let player = world.player().position;
    let mut commands = [None; 6];
    commands[0] = Some(Command::RandomU32);
    if tick.is_multiple_of(37) {
        commands[1] = Some(Command::SpawnEnemy(Enemy {
            position: player,
            velocity: Vec2::ZERO,
            radius: units(3),
            health: 3,
            contact_damage: 1,
            lifetime: 40,
            bounds: BoundsBehavior::Despawn,
            rgba: 0xff8866ff,
        }));
    }
    if tick.is_multiple_of(11) {
        let collider = match (tick / 11) % 3 {
            0 => Collider::circle(Fixed::ONE),
            1 => Collider::capsule(point(-4, -2), point(4, 2), Fixed::ONE),
            _ => Collider::curve(&[point(-6, 4), point(0, -4), point(6, 4)], Fixed::ONE),
        }
        .expect("valid fixture collider");
        // Vary offset independently of shape so every shape hits, grazes and
        // misses across the native/WASM fixture.
        let offset = [0, 8, 24][((tick / 33) % 3) as usize];
        let y = (i64::from(player.y.bits()) + i64::from(offset) * 65536)
            .min(i64::from(config.height.bits()) - 1) as i32;
        commands[2] = Some(Command::SpawnProjectile(Projectile {
            position: Vec2::new(Fixed::ZERO, Fixed::from_bits(y)),
            velocity: Vec2::new(Fixed::from_bits(config.width.bits() - 1), Fixed::ZERO),
            collider,
            faction: Faction::Enemy,
            damage: 1,
            lifetime: 3,
            bounds: BoundsBehavior::Despawn,
            rgba: 0x72dce8ff,
        }));
    }
    if tick.is_multiple_of(19) {
        let x =
            (i64::from(player.x.bits()) + 8 * 65536).min(i64::from(config.width.bits()) - 1) as i32;
        commands[3] = Some(Command::SpawnProjectile(Projectile {
            position: Vec2::new(Fixed::from_bits(x), player.y),
            velocity: Vec2::ZERO,
            collider: Collider::circle(Fixed::ONE).expect("positive radius"),
            faction: Faction::Enemy,
            damage: 1,
            lifetime: 25,
            bounds: BoundsBehavior::Keep,
            rgba: 0xcc99ffff,
        }));
    }
    if tick.is_multiple_of(7) {
        let target = world
            .snapshots()
            .find(|snapshot| snapshot.handle.kind() == EntityKind::Enemy)
            .map_or(Vec2::new(player.x, units(120)), |snapshot| {
                snapshot.position
            });
        commands[4] = Some(Command::SpawnProjectile(Projectile {
            position: target,
            velocity: Vec2::ZERO,
            collider: Collider::circle(Fixed::ONE).expect("positive radius"),
            faction: Faction::Player,
            damage: 2,
            lifetime: 1,
            bounds: BoundsBehavior::Despawn,
            rgba: 0xffff88ff,
        }));
    }
    if tick.is_multiple_of(31) {
        commands[5] = world
            .snapshots()
            .find(|snapshot| snapshot.handle.kind() == EntityKind::Projectile)
            .map(|snapshot| Command::Despawn(snapshot.handle));
    }
    commands
}
pub fn fixture_step(world: &mut Simulation) -> Result<(), SimulationError> {
    for command in fixture_commands(world).into_iter().flatten() {
        super::replay::apply(world, command)?;
    }
    world.step_with_input(crate::demo::input(world.tick()))
}
pub fn trace(ticks: u32) -> Vec<u64> {
    let mut world = fixture();
    (0..ticks)
        .map(|_| {
            fixture_step(&mut world).expect("bounded M1 fixture");
            world.state_hash()
        })
        .collect()
}
pub fn replay(ticks: u32) -> Result<InputReplay, ReplayError> {
    let mut recorder = ReplayRecorder::new(fixture_config(), 42)?;
    for _ in 0..ticks {
        for command in fixture_commands(recorder.simulation())
            .into_iter()
            .flatten()
        {
            recorder.command(command)?;
        }
        recorder.step(crate::demo::input(recorder.simulation().tick()))?;
    }
    recorder.finish()
}

pub fn benchmark_scene(count: u32, seed: u64) -> Result<Simulation, SimulationError> {
    benchmark_scene_with_collider(count, seed, benchmark_collider(0)?)
}

/// 0 = circle, 1 = capsule, 2 = three-point curve, with two-unit thickness.
pub fn benchmark_collider(shape: u32) -> Result<Collider, SimulationError> {
    match shape {
        0 => Collider::circle(units(2)),
        1 => Collider::capsule(point(-6, 0), point(6, 0), units(2)),
        2 => Collider::curve(&[point(-6, 3), point(0, -3), point(6, 3)], units(2)),
        _ => return Err(SimulationError::InvalidEntity),
    }
    .map_err(|_| SimulationError::InvalidEntity)
}
pub fn benchmark_scene_with_collider(
    count: u32,
    seed: u64,
    collider: Collider,
) -> Result<Simulation, SimulationError> {
    let config = SimulationConfig {
        projectile_capacity: count.max(1),
        enemy_capacity: 1,
        player: PlayerConfig {
            health: u32::MAX,
            invulnerability_ticks: 0,
            ..PlayerConfig::default()
        },
        ..SimulationConfig::default()
    };
    let mut world = Simulation::new(config, seed)?;
    replenish_with_collider(&mut world, count, collider)?;
    Ok(world)
}
/// Keep exactly the requested count before every measured step. Replenishment is
/// a host boundary workload measured separately from simulation stepping.
pub fn replenish(world: &mut Simulation, count: u32) -> Result<(), SimulationError> {
    replenish_with_collider(world, count, benchmark_collider(0)?)
}
pub fn replenish_with_collider(
    world: &mut Simulation,
    count: u32,
    collider: Collider,
) -> Result<(), SimulationError> {
    if count > world.config().projectile_capacity {
        return Err(SimulationError::Capacity);
    }
    while world.projectile_count() < count as usize {
        let x = Fixed::from_bits((world.random_u32() % world.config().width.bits() as u32) as i32);
        let y = Fixed::from_bits((world.random_u32() % world.config().height.bits() as u32) as i32);
        let vx = Fixed::from_bits((world.random_u32() % 513) as i32 - 256);
        let vy = Fixed::from_bits((world.random_u32() % 513) as i32 - 256);
        world.spawn_projectile(Projectile {
            position: Vec2::new(x, y),
            velocity: Vec2::new(vx, vy),
            collider,
            faction: Faction::Enemy,
            damage: 1,
            lifetime: 0,
            bounds: BoundsBehavior::Keep,
            rgba: 0x72dce8ff,
        })?;
    }
    Ok(())
}
