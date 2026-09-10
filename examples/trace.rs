fn main() {
    let ticks = std::env::args()
        .nth(1)
        .map(|v| v.parse::<u32>().expect("tick count"))
        .unwrap_or(100_000);
    assert!(ticks <= 100_000, "maximum 100,000 trace ticks");
    use std::io::Write;
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for hash in grazer::demo::trace(ticks) {
        writeln!(out, "{hash:016x}").unwrap();
    }
}
