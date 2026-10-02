use grazer::{
    game::{
        checkpoint::CheckpointInfo,
        replay::{GameRecorder, GameReplay, RecordingOptions, ReplayPlayer},
        showcase,
    },
    language::ScriptStage,
    resources::ResourcePack,
};
use std::sync::Arc;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str).unwrap_or("audit") {
        "record" => {
            let path = args.get(2).ok_or("record needs output path")?;
            let frames = args
                .get(3)
                .map(|s| s.parse())
                .transpose()?
                .unwrap_or(100000);
            let replay = record(frames)?;
            let bytes = replay.to_bytes()?;
            std::fs::write(path, &bytes)?;
            println!(
                "recorded frames={} checkpoints={} bytes={} content={:016x} resources={:016x}",
                replay.frames().len(),
                replay.checkpoints().len(),
                bytes.len(),
                replay.metadata().identity.content_hash,
                replay.metadata().identity.resources.content_hash
            );
        }
        "verify" | "seek" => {
            let replay = Arc::new(GameReplay::from_bytes(&std::fs::read(
                args.get(2).ok_or("needs replay path")?,
            )?)?);
            let mut player = ReplayPlayer::<ScriptStage>::new(
                replay.clone(),
                Arc::new(ResourcePack::builtin()),
            )?;
            if args[1] == "seek" {
                let frame = args.get(3).ok_or("seek needs frame")?.parse()?;
                player.seek(frame)?;
            } else {
                while player.step()? {}
            }
            println!(
                "frame={} tick={} hash={:016x}",
                player.frame(),
                player.game().hud().tick,
                player.game().state_hash()
            );
        }
        "practice" => {
            let phase = args
                .get(2)
                .ok_or("practice needs Boss phase and output path")?
                .parse()?;
            let g = showcase::practice(phase, grazer::advanced::Difficulty::Normal, 0)?;
            let bytes = g.checkpoint()?;
            std::fs::write(args.get(3).ok_or("practice needs output path")?, bytes)?;
            println!(
                "practice phase={phase} tick={} health={} hash={:016x}",
                g.hud().tick,
                g.hud().health,
                g.state_hash()
            );
        }
        "checkpoint" => {
            let bytes = std::fs::read(args.get(2).ok_or("checkpoint needs input path")?)?;
            println!("{:?}", CheckpointInfo::inspect(&bytes)?);
        }
        "audit" => {
            let replay = record(100000)?;
            let bytes = replay.to_bytes()?;
            let replay = Arc::new(GameReplay::from_bytes(&bytes)?);
            let mut player = ReplayPlayer::<ScriptStage>::new(
                replay.clone(),
                Arc::new(ResourcePack::builtin()),
            )?;
            while player.step()? {}
            let final_hash = player.game().state_hash();
            for frame in [0, 1, 600, 25201, 28891, 32491, 36001, 42000, 84000, 100000] {
                player.seek(frame)?;
                let expected = if frame == 0 {
                    replay.metadata().initial_hashes.game
                } else {
                    replay.frames()[frame - 1].hashes.game
                };
                assert_eq!(player.game().state_hash(), expected);
            }
            println!(
                "PASS M5 frames=100000 checkpoints={} bytes={} final={final_hash:016x}",
                replay.checkpoints().len(),
                bytes.len()
            );
        }
        _ => return Err("mode must be record, verify, seek, practice, checkpoint or audit".into()),
    }
    Ok(())
}
fn record(frames: u32) -> Result<GameReplay, Box<dyn std::error::Error>> {
    let mut r = GameRecorder::new(
        showcase::conformance_game(),
        RecordingOptions {
            max_frames: frames.max(1),
            ..RecordingOptions::default()
        },
        "Prism Passage M5",
    )?;
    for frame in 0..frames {
        r.step(showcase::input(u64::from(frame)))?;
    }
    Ok(r.finish())
}
