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
