use grazer::{Bullet, Config, Error, Fixed, Input, Runtime};
fn bullet() -> Bullet {
    Bullet {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        vx: Fixed::from_int(-1).unwrap(),
        vy: Fixed::from_bits(i32::MIN),
        radius: Fixed::ONE,
        rgba: 0xffffffff,
    }
}
#[test]
fn wraps_negative_and_large_velocity_without_overflow() {
    let mut r = Runtime::new(Config::default(), 0).unwrap();
    r.spawn(bullet()).unwrap();
    r.step().unwrap();
    let b = r.sprites().nth(1).unwrap();
    assert_eq!(b.x, 639.0);
    assert_eq!(b.y, 352.0);
}
#[test]
fn rejected_commands_do_not_change_state() {
    let mut r = Runtime::new(
        Config {
            capacity: 1,
            ..Config::default()
        },
        0,
    )
    .unwrap();
    r.spawn(bullet()).unwrap();
    let hash = r.state_hash();
    assert_eq!(r.spawn(bullet()), Err(Error::Capacity));
    assert_eq!(r.set_input(Input { x: 2, y: 0 }), Err(Error::InvalidInput));
    assert_eq!(r.state_hash(), hash);
}
#[test]
fn checkpoint_and_input_are_authoritative() {
    let mut r = grazer::demo::scene(200, 42).unwrap();
    for tick in 0..50 {
        r.set_input(grazer::demo::input(tick)).unwrap();
        r.step().unwrap();
    }
    let mut restored = r.clone();
    restored.set_input(Input { x: 1, y: 1 }).unwrap();
    assert_ne!(restored.state_hash(), r.state_hash());
    for tick in 50..2000 {
        for runtime in [&mut r, &mut restored] {
            runtime.set_input(grazer::demo::input(tick)).unwrap();
            runtime.step().unwrap();
        }
        assert_eq!(r.state_hash(), restored.state_hash());
    }
}
#[test]
fn rng_golden_and_config_validation() {
    let mut r = Runtime::new(Config::default(), 0).unwrap();
    assert_eq!(r.random_u32(), 0xe220a839);
    assert_eq!(r.random_u32(), 0x6e789e6a);
    assert!(
        Runtime::new(
            Config {
                width: Fixed::ZERO,
                ..Config::default()
            },
            0
        )
        .is_err()
    );
    assert!(
        Runtime::new(
            Config {
                capacity: 1_000_001,
                ..Config::default()
            },
            0
        )
        .is_err()
    );
}
