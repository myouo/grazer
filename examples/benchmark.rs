use std::{hint::black_box, time::Instant};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let count = args
        .get(1)
        .map(|v| v.parse::<u32>().unwrap())
        .unwrap_or(100_000);
    let ticks = args
        .get(2)
        .map(|v| v.parse::<usize>().unwrap())
        .unwrap_or(1200);
    assert!(ticks > 0);
    match args.get(3).map(String::as_str).unwrap_or("m0") {
        "m0" => {}
        "m1" => {
            benchmark_headless(
                count,
                ticks,
                args.get(4).map(String::as_str).unwrap_or("circle"),
            );
            return;
        }
        _ => panic!("benchmark mode must be m0 or m1"),
    }
    let mut runtime = grazer::demo::scene(count, 42).unwrap();
    for _ in 0..120 {
        runtime.step().unwrap();
    }
    let mut times = Vec::with_capacity(ticks);
    for _ in 0..ticks {
        let start = Instant::now();
        runtime.step().unwrap();
        black_box(&runtime);
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "motion-only count={count} ticks={ticks} warmup=120 median_ms={:.4} p95_ms={:.4} max_ms={:.4} hash={:016x}",
        times[ticks / 2],
        times[(ticks * 95).div_ceil(100) - 1],
        times[ticks - 1],
        runtime.state_hash()
    );
}

fn benchmark_headless(count: u32, ticks: usize, shape: &str) {
    use grazer::{Event, simulation::demo};
    let kind = match shape {
        "circle" => 0,
        "capsule" => 1,
        "curve" => 2,
        _ => panic!("shape must be circle, capsule or curve"),
    };
    let collider = demo::benchmark_collider(kind).unwrap();
    let mut world = demo::benchmark_scene_with_collider(count, 42, collider).unwrap();
    for _ in 0..120 {
        world
            .step_with_input(grazer::demo::input(world.tick()))
            .unwrap();
        demo::replenish_with_collider(&mut world, count, collider).unwrap();
    }
    let mut times = Vec::with_capacity(ticks);
    let mut replenishment = Vec::with_capacity(ticks);
    let mut hits = 0;
    let mut minimum_count = count as usize;
    for _ in 0..ticks {
        assert_eq!(world.projectile_count(), count as usize);
        let start = Instant::now();
        world
            .step_with_input(grazer::demo::input(world.tick()))
            .unwrap();
        black_box(&world);
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        minimum_count = minimum_count.min(world.projectile_count());
        hits += world
            .events()
            .iter()
            .filter(|event| matches!(event, Event::Hit { .. }))
            .count();
        let start = Instant::now();
        demo::replenish_with_collider(&mut world, count, collider).unwrap();
        replenishment.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    replenishment.sort_by(f64::total_cmp);
    let p95 = (ticks * 95).div_ceil(100) - 1;
    println!(
        "{{\"workload\":\"m1-motion-collision-graze\",\"shape\":\"{shape}\",\"count\":{count},\"ticks\":{ticks},\"warmup\":120,\"simulation_median_ms\":{:.6},\"simulation_p95_ms\":{:.6},\"simulation_max_ms\":{:.6},\"replenish_p95_ms\":{:.6},\"minimum_count_after_step\":{minimum_count},\"hits_in_samples\":{hits},\"grazes_including_warmup\":{},\"hash\":\"{:016x}\"}}",
        times[ticks / 2],
        times[p95],
        times[ticks - 1],
        replenishment[p95],
        world.player().grazes,
        world.state_hash()
    );
}
