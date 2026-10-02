//! Reproducible M0 workload. Versioned together with the simulation protocol.
use crate::{Bullet, Config, Error, Fixed, Input, Runtime};

pub fn scene(count: u32, seed: u64) -> Result<Runtime, Error> {
    let mut runtime = Runtime::new(
        Config {
            capacity: count.max(1),
            ..Config::default()
        },
        seed,
    )?;
    for _ in 0..count {
        let x = Fixed::from_bits((runtime.random_u32() % (640 << 16)) as i32);
        let y = Fixed::from_bits((runtime.random_u32() % (480 << 16)) as i32);
        let vx = Fixed::from_bits((runtime.random_u32() % (4 << 16)) as i32 - (2 << 16));
        let vy = Fixed::from_bits((runtime.random_u32() % (4 << 16)) as i32 - (2 << 16));
        runtime.spawn(Bullet {
            x,
            y,
            vx,
            vy,
            radius: Fixed::from_bits(2 << 16),
            rgba: 0x72dce8ff,
        })?;
    }
    Ok(runtime)
}

/// Platform-independent conformance inputs; no random numbers supplied by host.
pub fn input(tick: u64) -> Input {
    Input {
        x: ((tick / 71) % 3) as i32 - 1,
        y: ((tick / 113) % 3) as i32 - 1,
    }
}

pub fn trace(ticks: u32) -> Vec<u64> {
    let mut runtime = scene(128, 42).expect("valid conformance fixture");
    (0..ticks)
        .map(|_| {
            runtime
                .set_input(input(runtime.tick()))
                .expect("valid input");
            runtime.step().expect("bounded trace");
            runtime.state_hash()
        })
        .collect()
}
