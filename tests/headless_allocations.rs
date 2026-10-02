use grazer::{
    BoundsBehavior, Collider, Enemy, Faction, Fixed, Input, Projectile, Simulation, Vec2,
    simulation::{demo, replay::ReplayPlayer},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
struct CountingAllocator;
// SAFETY: all allocation operations delegate unchanged to the system allocator.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        TRACK.with(|track| {
            if track.get() {
                ALLOCATIONS.with(|count| count.set(count.get() + 1));
            }
        });
        // SAFETY: layout is the caller's valid allocation layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: pointer and layout came from the delegated system allocation.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        TRACK.with(|track| {
            if track.get() {
                ALLOCATIONS.with(|count| count.set(count.get() + 1));
            }
        });
        // SAFETY: caller supplies an allocated pointer/layout and valid new size.
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn advanced_stage_motion_lasers_death_drops_and_snapshots_reuse_buffers() {
    let mut game = grazer::game::showcase::conformance_game().clone();
    ALLOCATIONS.set(0);
    TRACK.set(true);
    for frame in 0..36002 {
        game.step(grazer::game::showcase::input(frame)).unwrap();
        for sprite in game.sprites() {
            std::hint::black_box(sprite);
        }
        for segment in game.laser_segments() {
            std::hint::black_box(segment);
        }
        std::hint::black_box(game.advanced_hud());
    }
    TRACK.set(false);
    assert_eq!(ALLOCATIONS.get(), 0);
}

#[test]
fn vm_forks_calls_waits_and_owner_cancellation_reuse_buffers() {
    let mut game = grazer::language::conformance_game().clone();
    ALLOCATIONS.set(0);
    TRACK.set(true);
    for frame in 0..11000 {
        game.step(grazer::game::conformance_input(frame)).unwrap();
        std::hint::black_box(game.state_hash());
    }
    TRACK.set(false);
    assert_eq!(ALLOCATIONS.get(), 0);
}

#[test]
fn native_game_ticks_audio_and_sprite_iteration_reuse_all_buffers() {
    let mut game = grazer::game::conformance_game().clone();
    ALLOCATIONS.set(0);
    TRACK.set(true);
    for frame in 0..1000 {
        game.step(grazer::game::conformance_input(frame)).unwrap();
        std::hint::black_box(game.state_hash());
        for sprite in game.sprites() {
            std::hint::black_box(sprite);
        }
        std::hint::black_box(game.audio_events());
    }
    TRACK.set(false);
    assert_eq!(ALLOCATIONS.get(), 0);
}

#[test]
fn cloned_world_reuses_pools_contact_and_event_buffers_at_full_capacity() {
    let mut config = demo::fixture_config();
    config.player.health = 1;
    config.player.invulnerability_ticks = 0;
    let world = Simulation::new(config, 42).unwrap();
    let mut world = world.clone();
    let projectile = Projectile {
        position: config.player.position,
        velocity: Vec2::ZERO,
        collider: Collider::circle(Fixed::ONE).unwrap(),
        faction: Faction::Enemy,
        damage: 1,
        lifetime: 1,
        bounds: BoundsBehavior::Keep,
        rgba: 0xffffffff,
    };
    let enemy = Enemy {
        position: config.player.position,
        velocity: Vec2::ZERO,
        radius: Fixed::ONE,
        health: 3,
        contact_damage: 1,
        lifetime: 1,
        bounds: BoundsBehavior::Keep,
        rgba: 0xffffffff,
    };
    ALLOCATIONS.set(0);
    TRACK.set(true);
    for index in 0..10 {
        for _ in 0..config.projectile_capacity {
            world.spawn_projectile(projectile).unwrap();
        }
        for _ in 0..config.enemy_capacity {
            world.spawn_enemy(enemy).unwrap();
        }
        world.step_with_input(Input::default()).unwrap();
        if index == 0 {
            assert_eq!(
                world.events().len(),
                (config.projectile_capacity as usize + config.enemy_capacity as usize) * 2 + 1
            );
        }
        std::hint::black_box(world.state_hash());
        for snapshot in world.snapshots() {
            std::hint::black_box(snapshot);
        }
    }
    TRACK.set(false);
    assert_eq!(ALLOCATIONS.get(), 0);
}

#[test]
fn playback_allocates_only_when_constructing_the_world() {
    let replay = demo::replay(500).unwrap();
    let mut player = ReplayPlayer::new(&replay).unwrap();
    ALLOCATIONS.set(0);
    TRACK.set(true);
    while player.step().unwrap() {}
    TRACK.set(false);
    assert_eq!(ALLOCATIONS.get(), 0);
}
