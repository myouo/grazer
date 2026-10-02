//! Raw WASM conformance harness; the Vec is retained in thread-local storage.
use std::cell::RefCell;
thread_local! {
    static TRACE: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    static BENCHMARK: RefCell<Option<Benchmark>> = const { RefCell::new(None) };
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
