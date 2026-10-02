fn main() {
    let ticks = std::env::args()
        .nth(1)
        .map(|v| v.parse::<u32>().expect("tick count"))
        .unwrap_or(100_000);
    assert!(ticks <= 100_000, "maximum 100,000 trace ticks");
    use std::io::Write;
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    let hashes = match std::env::args().nth(2).as_deref().unwrap_or("m0") {
        "m0" => grazer::demo::trace(ticks),
        "m1" => grazer::simulation::demo::trace(ticks),
        "m2" => grazer::game::trace(ticks),
        "m3" => grazer::language::trace(ticks),
        "m4" => grazer::game::showcase::trace(ticks),
        _ => panic!("trace mode must be m0, m1, m2, m3 or m4"),
    };
    for hash in hashes {
        writeln!(out, "{hash:016x}").unwrap();
    }
}
