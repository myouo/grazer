use grazer::{
    Game, GameConfig, GameInput,
    game::GamePhase,
    language::{Program, ScriptStage, VmLimits},
    resources::ResourcePack,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("restore-check") {
        println!("{:016x}", grazer::language::restore_fixture_hash(10000));
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("compile") {
        let source =
            std::fs::read_to_string(args.get(2).ok_or("compile needs source and output paths")?)?;
        let program = Program::compile(args.get(2).expect("path"), &source)?;
        std::fs::write(
            args.get(3).ok_or("compile needs output path")?,
            program.to_bytes(),
        )?;
        println!(
            "functions={} instructions={} fingerprint={:016x}",
            program.function_names().len(),
            program.instruction_count(),
            program.content_hash()
        );
        return Ok(());
    }
    let stage = if let Some(path) = args.get(1) {
        ScriptStage::compile(
            path,
            &std::fs::read_to_string(path)?,
            VmLimits::default(),
            42,
        )?
    } else {
        ScriptStage::builtin(42)?
    };
    let mut config = GameConfig::default();
    config.simulation.player.health = 10000;
    config.simulation.projectile_capacity = 512;
    let mut game = Game::with_stage(config, 42, ResourcePack::builtin(), stage)?;
    for _ in 0..12000 {
        game.step(GameInput {
            fire: true,
            ..GameInput::default()
        })?;
        if game.phase() != GamePhase::Playing {
            break;
        }
    }
    println!(
        "phase={:?} tick={} score={} tasks={} instructions_last={} hash={:016x}",
        game.phase(),
        game.hud().tick,
        game.hud().score,
        game.stage().vm().task_count(),
        game.stage().vm().last_instruction_count(),
        game.state_hash()
    );
    if game.phase() != GamePhase::Cleared {
        return Err("stage did not clear".into());
    }
    Ok(())
}
