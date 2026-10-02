use std::time::Duration;
/// Host clock with exact rational 60 Hz accumulation. At most eight ticks per
/// frame, retaining overload backlog. Inactive frames discard paused wall time.
#[derive(Default)]
pub struct FrameClock {
    units: u128,
}
impl FrameClock {
    pub fn reset(&mut self) {
        self.units = 0;
    }
    pub fn advance(&mut self, elapsed: Duration, active: bool) -> usize {
        if !active {
            self.reset();
            return 0;
        }
        self.units = self
            .units
            .saturating_add(elapsed.as_nanos().saturating_mul(60));
        let ticks = (self.units / 1_000_000_000).min(8) as usize;
        self.units -= ticks as u128 * 1_000_000_000;
        ticks
    }
}
