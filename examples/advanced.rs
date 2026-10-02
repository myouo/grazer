//! Headless audit of the complete M4 stage at every difficulty.
use grazer::{
    advanced::Difficulty,
    game::{GamePhase, showcase},
};
fn main() -> Result<(), grazer::GameError> {
    for difficulty in [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard] {
        let mut game = showcase::game(difficulty, 10000)?;
        let initial = game.state_hash();
        let mut peak = 0;
        let mut beam_segments = 0;
        let mut phases = 0;
        for frame in 0..36002 {
            game.step(showcase::input(frame))?;
            peak = peak.max(game.hud().projectiles);
            beam_segments = beam_segments.max(game.laser_segments().count());
            let phase = game.advanced_hud().expect("M4 mode").boss_phase;
            if phase > 0 {
                phases |= 1 << (phase - 1);
            }
        }
        let h = game.hud();
        let a = game.advanced_hud().expect("M4 mode");
        assert_eq!(game.phase(), GamePhase::Cleared);
        assert_eq!(h.tick, 36001);
        assert_eq!(phases, 7);
        println!(
            "{difficulty:?}: tick={} score={} grazes={} power={} collected={} cancelled={} phase_bonus={} peak_projectiles={} peak_laser_segments={} hash={:016x}",
            h.tick,
            h.score,
            h.grazes,
            a.power,
            a.collected,
            a.cancelled,
            a.phase_bonus,
            peak,
            beam_segments,
            game.state_hash()
        );
        game.restart()?;
        assert_eq!(game.state_hash(), initial);
    }
    Ok(())
}
