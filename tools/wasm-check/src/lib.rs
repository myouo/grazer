//! Raw WASM conformance harness; the Vec is retained in thread-local storage.
use std::cell::RefCell;
thread_local! {
    static TRACE: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    static BENCHMARK: RefCell<Option<Benchmark>> = const { RefCell::new(None) };
    static GAME_REPLAY_BYTES: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static GAME_REPLAY_PLAYER: RefCell<Option<grazer::game::replay::ReplayPlayer<grazer::language::ScriptStage>>> = const { RefCell::new(None) };
}
#[unsafe(no_mangle)]
pub extern "C" fn game_replay_buffer(length: u32) -> *mut u8 {
    if length as usize > grazer::game::replay::MAX_REPLAY_BYTES {
        return std::ptr::null_mut();
    }
    GAME_REPLAY_BYTES.with(|bytes| {
        let mut bytes = bytes.borrow_mut();
        bytes.resize(length as usize, 0);
        bytes.as_mut_ptr()
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn game_replay_load() -> u32 {
    GAME_REPLAY_BYTES.with(|bytes| {
        let bytes = bytes.borrow();
        let result = grazer::game::replay::GameReplay::from_bytes(&bytes).and_then(|r| {
            grazer::game::replay::ReplayPlayer::new(
                std::sync::Arc::new(r),
                std::sync::Arc::new(grazer::resources::ResourcePack::builtin()),
            )
        });
        match result {
            Ok(player) => {
                GAME_REPLAY_PLAYER.with(|p| *p.borrow_mut() = Some(player));
                1
            }
            Err(_) => 0,
        }
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn game_replay_step() -> u32 {
    GAME_REPLAY_PLAYER.with(|p| {
        p.borrow_mut().as_mut().map_or(2, |p| match p.step() {
            Ok(true) => 1,
            Ok(false) => 0,
            Err(_) => 2,
        })
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn game_replay_hash() -> u64 {
    GAME_REPLAY_PLAYER.with(|p| p.borrow().as_ref().map_or(0, |p| p.game().state_hash()))
}
#[unsafe(no_mangle)]
pub extern "C" fn game_replay_seek(frame: u32) -> u32 {
    GAME_REPLAY_PLAYER.with(|p| {
        p.borrow_mut()
            .as_mut()
            .map_or(0, |p| u32::from(p.seek(frame as usize).is_ok()))
    })
}
struct Benchmark {
    world: grazer::Simulation,
    count: u32,
    collider: grazer::Collider,
}

#[unsafe(no_mangle)]
pub extern "C" fn run_trace(ticks: u32) -> *const u64 {
    TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        *trace = grazer::demo::trace(ticks.min(100_000));
        trace.as_ptr()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn run_simulation_trace(ticks: u32) -> *const u64 {
    TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        *trace = grazer::simulation::demo::trace(ticks.min(100_000));
        trace.as_ptr()
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn run_game_trace(frames: u32) -> *const u64 {
    TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        *trace = grazer::game::trace(frames.min(100000));
        trace.as_ptr()
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn run_script_trace(frames: u32) -> *const u64 {
    TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        *trace = grazer::language::trace(frames.min(100000));
        trace.as_ptr()
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn run_advanced_trace(frames: u32) -> *const u64 {
    TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        *trace = grazer::game::showcase::trace(frames.min(100000));
        trace.as_ptr()
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn run_vm_restore_check(ticks: u32) -> u64 {
    grazer::language::restore_fixture_hash(ticks)
}

#[unsafe(no_mangle)]
pub extern "C" fn run_replay_check(ticks: u32) -> u64 {
    let replay =
        grazer::simulation::demo::replay(ticks.min(10_000)).expect("valid fixture recording");
    grazer::simulation::InputReplay::from_bytes(&replay.to_bytes())
        .expect("valid replay bytes")
        .play()
        .expect("matching replay")
        .state_hash()
}

#[unsafe(no_mangle)]
pub extern "C" fn benchmark_begin(count: u32, shape: u32) -> u32 {
    use grazer::simulation::demo;
    let Ok(collider) = demo::benchmark_collider(shape) else {
        return 0;
    };
    let Ok(world) = demo::benchmark_scene_with_collider(count, 42, collider) else {
        return 0;
    };
    BENCHMARK.with(|benchmark| {
        *benchmark.borrow_mut() = Some(Benchmark {
            world,
            count,
            collider,
        })
    });
    1
}
#[unsafe(no_mangle)]
pub extern "C" fn benchmark_step() -> u32 {
    BENCHMARK.with(|benchmark| {
        let mut benchmark = benchmark.borrow_mut();
        let Some(benchmark) = benchmark.as_mut() else {
            return 0;
        };
        let input = grazer::demo::input(benchmark.world.tick());
        u32::from(benchmark.world.step_with_input(input).is_ok())
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn benchmark_replenish() -> u32 {
    BENCHMARK.with(|benchmark| {
        let mut benchmark = benchmark.borrow_mut();
        let Some(benchmark) = benchmark.as_mut() else {
            return 0;
        };
        u32::from(
            grazer::simulation::demo::replenish_with_collider(
                &mut benchmark.world,
                benchmark.count,
                benchmark.collider,
            )
            .is_ok(),
        )
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn benchmark_hash() -> u64 {
    BENCHMARK.with(|benchmark| {
        benchmark
            .borrow()
            .as_ref()
            .map_or(0, |b| b.world.state_hash())
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn benchmark_count() -> u32 {
    BENCHMARK.with(|benchmark| {
        benchmark
            .borrow()
            .as_ref()
            .map_or(0, |b| b.world.projectile_count() as u32)
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn benchmark_grazes() -> u64 {
    BENCHMARK.with(|benchmark| {
        benchmark
            .borrow()
            .as_ref()
            .map_or(0, |b| b.world.player().grazes)
    })
}
