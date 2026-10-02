//! Complete input/control replay, resource binding, checkpoints and verified seek.
use super::{
    checkpoint::{CheckpointInfo, CheckpointStage, GameIdentity, StateHashes},
    *,
};
use crate::checkpoint::{CheckpointError, Reader, Writer};
pub const REPLAY_VERSION: u32 = 1;
pub const MAX_REPLAY_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_REPLAY_FRAMES: usize = 1_000_000;
pub const MAX_CHECKPOINTS: usize = 4096;
const MAGIC: &[u8; 8] = b"GZREP001";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordingOptions {
    pub max_frames: u32,
    pub checkpoint_interval: u32,
}
impl Default for RecordingOptions {
    fn default() -> Self {
        Self {
            max_frames: 100000,
            checkpoint_interval: 600,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayAction {
    Step(GameInput),
    Reset,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameOutcome {
    pub kind: u8,
    pub detail: u32,
}
impl FrameOutcome {
    fn of(result: &Result<(), GameError>) -> Self {
        match result {
            Ok(()) => Self::default(),
            Err(GameError::Script(d)) => Self {
                kind: 1,
                detail: d.kind as u32,
            },
            Err(GameError::Simulation(e)) => Self {
                kind: 2,
                detail: simulation_code(*e),
            },
            Err(GameError::Faulted) => Self { kind: 3, detail: 0 },
            Err(_) => Self { kind: 4, detail: 0 },
        }
    }
}
fn simulation_code(e: SimulationError) -> u32 {
    match e {
        SimulationError::InvalidConfig => 1,
        SimulationError::InvalidInput => 2,
        SimulationError::InvalidEntity => 3,
        SimulationError::InvalidHandle => 4,
        SimulationError::Capacity => 5,
        SimulationError::Exhausted => 6,
        SimulationError::ArithmeticOverflow => 7,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayFrame {
    pub action: ReplayAction,
    pub outcome: FrameOutcome,
    pub tick: u64,
    pub hashes: StateHashes,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayCheckpoint {
    pub frame: u64,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayMetadata {
    pub version: u32,
    pub tick_rate: u32,
    pub identity: GameIdentity,
    pub initial_tick: u64,
    pub initial_hashes: StateHashes,
    pub label: String,
}
#[derive(Clone)]
pub struct GameReplay {
    metadata: ReplayMetadata,
    initial: Vec<u8>,
    frames: Vec<ReplayFrame>,
    checkpoints: Vec<ReplayCheckpoint>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Component {
    Game = 0,
    World = 1,
    Stage = 2,
    Player = 3,
    Enemies = 4,
    Projectiles = 5,
    Drops = 6,
    Outcome = 7,
    Tick = 8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Divergence {
    pub frame: u64,
    pub tick: u64,
    pub component: Component,
    pub expected: u64,
    pub actual: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayError {
    Checkpoint(CheckpointError),
    Game(GameError),
    Limit,
    Data(&'static str),
    Diverged(Divergence),
    Stopped,
    End,
}
impl From<CheckpointError> for ReplayError {
    fn from(e: CheckpointError) -> Self {
        Self::Checkpoint(e)
    }
}
impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Checkpoint(e) => write!(f, "replay: {e}"),
            Self::Game(e) => write!(f, "recorded game fault: {e}"),
            Self::Limit => f.write_str("replay recording/frame/checkpoint limit exceeded"),
            Self::Data(s) => write!(f, "invalid replay: {s}"),
            Self::Diverged(d) => write!(
                f,
                "replay diverged at frame {} (tick {}), {:?}: expected {:016x}, got {:016x}",
                d.frame, d.tick, d.component, d.expected, d.actual
            ),
            Self::Stopped => f.write_str("replay stopped after divergence; seek to recover"),
            Self::End => f.write_str("replay is at the end"),
        }
    }
}
impl std::error::Error for ReplayError {}
fn difference(expected: StateHashes, actual: StateHashes) -> Option<(Component, u64, u64)> {
    for (c, a, b) in [
        (Component::Player, expected.player, actual.player),
        (Component::Enemies, expected.enemies, actual.enemies),
        (
            Component::Projectiles,
            expected.projectiles,
            actual.projectiles,
        ),
        (Component::Drops, expected.drops, actual.drops),
        (Component::Stage, expected.stage, actual.stage),
        (Component::World, expected.world, actual.world),
        (Component::Game, expected.game, actual.game),
    ] {
        if a != b {
            return Some((c, a, b));
        }
    }
    None
}
impl GameReplay {
    pub fn metadata(&self) -> &ReplayMetadata {
        &self.metadata
    }
    pub fn frames(&self) -> &[ReplayFrame] {
        &self.frames
    }
    pub fn checkpoints(&self) -> &[ReplayCheckpoint] {
        &self.checkpoints
    }
    pub fn initial_checkpoint(&self) -> &[u8] {
        &self.initial
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, ReplayError> {
        let mut w = Writer::new(MAGIC);
        w.u32(REPLAY_VERSION);
        w.u32(crate::TICK_RATE);
        self.metadata.identity.write(&mut w);
        w.u64(self.metadata.initial_tick);
        self.metadata.initial_hashes.write(&mut w);
        w.text(&self.metadata.label);
        w.blob(&self.initial);
        w.u32(self.frames.len() as u32);
        for frame in &self.frames {
            match frame.action {
                ReplayAction::Step(input) => {
                    w.u8(0);
                    w.u8((input.x + 1) as u8);
                    w.u8((input.y + 1) as u8);
                    w.u8(input.flags() as u8);
                }
                ReplayAction::Reset => w.u8(1),
            }
            w.u8(frame.outcome.kind);
            w.u32(frame.outcome.detail);
            w.u64(frame.tick);
            frame.hashes.write(&mut w);
        }
        w.u32(self.checkpoints.len() as u32);
        for cp in &self.checkpoints {
            w.u64(cp.frame);
            w.blob(&cp.bytes);
        }
        Ok(w.finish(MAX_REPLAY_BYTES)?)
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ReplayError> {
        let mut r = Reader::new(bytes, MAGIC, MAX_REPLAY_BYTES)?;
        let version = r.u32()?;
        let tick_rate = r.u32()?;
        if version != REPLAY_VERSION || tick_rate != crate::TICK_RATE {
            return Err(CheckpointError::Version.into());
        }
        let identity = GameIdentity::read(&mut r)?;
        let initial_tick = r.u64()?;
        let initial_hashes = StateHashes::read(&mut r)?;
        let label = r.text(4096)?;
        let initial = r.blob(crate::checkpoint::MAX_CHECKPOINT_BYTES)?.to_vec();
        let info = CheckpointInfo::inspect(&initial)?;
        if info.identity != identity || info.tick != initial_tick || info.hashes != initial_hashes {
            return Err(ReplayError::Data("initial checkpoint binding"));
        }
        let n = r.count(MAX_REPLAY_FRAMES, 70)?;
        let mut frames = Vec::with_capacity(n);
        for _ in 0..n {
            let action = match r.u8()? {
                0 => ReplayAction::Step(
                    GameInput::from_flags(
                        i32::from(r.u8()?) - 1,
                        i32::from(r.u8()?) - 1,
                        u32::from(r.u8()?),
                    )
                    .map_err(|_| ReplayError::Data("input axes/flags"))?,
                ),
                1 => ReplayAction::Reset,
                _ => return Err(ReplayError::Data("action tag")),
            };
            let outcome = FrameOutcome {
                kind: r.u8()?,
                detail: r.u32()?,
            };
            if !match outcome.kind {
                0 | 3 => outcome.detail == 0,
                1 => crate::language::DiagnosticKind::decode(outcome.detail).is_some(),
                2 => (1..=7).contains(&outcome.detail),
                _ => false,
            } {
                return Err(ReplayError::Data("frame outcome"));
            }
            let tick = r.u64()?;
            let hashes = StateHashes::read(&mut r)?;
            if action == ReplayAction::Reset
                && (tick != initial_tick
                    || hashes != initial_hashes
                    || outcome != FrameOutcome::default())
            {
                return Err(ReplayError::Data("reset frame"));
            }
            frames.push(ReplayFrame {
                action,
                outcome,
                tick,
                hashes,
            });
        }
        let n = r.count(MAX_CHECKPOINTS, 12)?;
        let mut checkpoints = Vec::with_capacity(n);
        let mut previous = 0;
        for _ in 0..n {
            let frame = r.u64()?;
            if frame <= previous || frame > frames.len() as u64 {
                return Err(ReplayError::Data("checkpoint order/frame"));
            }
            let bytes = r.blob(crate::checkpoint::MAX_CHECKPOINT_BYTES)?.to_vec();
            let info = CheckpointInfo::inspect(&bytes)?;
            let expected = &frames[frame as usize - 1];
            if info.identity != identity
                || info.tick != expected.tick
                || info.hashes != expected.hashes
            {
                return Err(ReplayError::Data("checkpoint frame binding"));
            }
            checkpoints.push(ReplayCheckpoint { frame, bytes });
            previous = frame;
        }
        r.finish()?;
        Ok(Self {
            metadata: ReplayMetadata {
                version,
                tick_rate,
                identity,
                initial_tick,
                initial_hashes,
                label,
            },
            initial,
            frames,
            checkpoints,
        })
    }
}
pub struct GameRecorder<S: CheckpointStage> {
    game: Game<S>,
    replay: GameReplay,
    options: RecordingOptions,
    checkpoint_bytes: usize,
}
impl<S: CheckpointStage> GameRecorder<S> {
    pub fn new(game: Game<S>, options: RecordingOptions, label: &str) -> Result<Self, ReplayError> {
        if options.max_frames == 0
            || options.max_frames as usize > MAX_REPLAY_FRAMES
            || label.len() > 4096
        {
            return Err(ReplayError::Limit);
        }
        let initial = game.checkpoint()?;
        let capacity = options
            .max_frames
            .checked_div(options.checkpoint_interval)
            .unwrap_or(0) as usize;
        let capacity = capacity.min(MAX_CHECKPOINTS);
        let metadata = ReplayMetadata {
            version: REPLAY_VERSION,
            tick_rate: crate::TICK_RATE,
            identity: game.identity(),
            initial_tick: game.hud().tick,
            initial_hashes: game.state_hashes(),
            label: label.into(),
        };
        let checkpoint_bytes = initial.len();
        Ok(Self {
            game,
            replay: GameReplay {
                metadata,
                initial,
                frames: Vec::with_capacity(options.max_frames as usize),
                checkpoints: Vec::with_capacity(capacity),
            },
            options,
            checkpoint_bytes,
        })
    }
    pub fn game(&self) -> &Game<S> {
        &self.game
    }
    pub fn replay(&self) -> &GameReplay {
        &self.replay
    }
    fn can_record(&self) -> Result<(), ReplayError> {
        let next = self.replay.frames.len() + 1;
        if next > self.options.max_frames as usize
            || self.options.checkpoint_interval > 0
                && next.is_multiple_of(self.options.checkpoint_interval as usize)
                && self.replay.checkpoints.len() == MAX_CHECKPOINTS
        {
            Err(ReplayError::Limit)
        } else {
            Ok(())
        }
    }
    fn capture(
        &mut self,
        action: ReplayAction,
        result: Result<(), GameError>,
    ) -> Result<StateHashes, ReplayError> {
        let hashes = self.game.state_hashes();
        let outcome = FrameOutcome::of(&result);
        self.replay.frames.push(ReplayFrame {
            action,
            outcome,
            tick: self.game.hud().tick,
            hashes,
        });
        let frame = self.replay.frames.len();
        if self.options.checkpoint_interval > 0
            && frame.is_multiple_of(self.options.checkpoint_interval as usize)
        {
            let bytes = self.game.checkpoint()?;
            self.checkpoint_bytes = self
                .checkpoint_bytes
                .checked_add(bytes.len())
                .ok_or(ReplayError::Limit)?;
            if self.checkpoint_bytes + frame * 80 > MAX_REPLAY_BYTES {
                return Err(ReplayError::Limit);
            }
            self.replay.checkpoints.push(ReplayCheckpoint {
                frame: frame as u64,
                bytes,
            });
        }
        match result {
            Ok(()) => Ok(hashes),
            Err(e) => Err(ReplayError::Game(e)),
        }
    }
    pub fn step(&mut self, input: GameInput) -> Result<StateHashes, ReplayError> {
        GameInput::from_flags(input.x, input.y, input.flags()).map_err(ReplayError::Game)?;
        self.can_record()?;
        let result = self.game.step(input);
        self.capture(ReplayAction::Step(input), result)
    }
    /// Reset to this recording's start, including a mid-stage practice start.
    pub fn reset(&mut self) -> Result<StateHashes, ReplayError> {
        self.can_record()?;
        self.game = Game::restore_checkpoint(self.game.resource_pack(), &self.replay.initial)?;
        self.capture(ReplayAction::Reset, Ok(()))
    }
    pub fn finish(self) -> GameReplay {
        self.replay
    }
    pub fn into_parts(self) -> (Game<S>, GameReplay) {
        (self.game, self.replay)
    }
}
pub struct ReplayPlayer<S: CheckpointStage> {
    replay: Arc<GameReplay>,
    game: Game<S>,
    frame: usize,
    last_error: Option<ReplayError>,
}
impl<S: CheckpointStage> ReplayPlayer<S> {
    pub fn new(replay: Arc<GameReplay>, resources: Arc<ResourcePack>) -> Result<Self, ReplayError> {
        let game = Game::restore_checkpoint(resources, &replay.initial)?;
        if game.identity() != replay.metadata.identity
            || game.state_hashes() != replay.metadata.initial_hashes
        {
            return Err(ReplayError::Data("replay identity"));
        }
        Ok(Self {
            replay,
            game,
            frame: 0,
            last_error: None,
        })
    }
    pub fn game(&self) -> &Game<S> {
        &self.game
    }
    pub fn frame(&self) -> usize {
        self.frame
    }
    pub fn replay(&self) -> &GameReplay {
        &self.replay
    }
    pub fn last_error(&self) -> Option<&ReplayError> {
        self.last_error.as_ref()
    }
    pub fn step(&mut self) -> Result<bool, ReplayError> {
        if self.last_error.is_some() {
            return Err(ReplayError::Stopped);
        }
        let Some(expected) = self.replay.frames.get(self.frame).copied() else {
            return Ok(false);
        };
        let result = match expected.action {
            ReplayAction::Step(input) => self.game.step(input),
            ReplayAction::Reset => {
                self.game =
                    Game::restore_checkpoint(self.game.resource_pack(), &self.replay.initial)?;
                Ok(())
            }
        };
        let outcome = FrameOutcome::of(&result);
        let hashes = self.game.state_hashes();
        let diff = if outcome != expected.outcome {
            Some((
                Component::Outcome,
                (u64::from(expected.outcome.kind) << 32) | u64::from(expected.outcome.detail),
                (u64::from(outcome.kind) << 32) | u64::from(outcome.detail),
            ))
        } else if self.game.hud().tick != expected.tick {
            Some((Component::Tick, expected.tick, self.game.hud().tick))
        } else {
            difference(expected.hashes, hashes)
        };
        if let Some((component, expected_hash, actual)) = diff {
            let error = ReplayError::Diverged(Divergence {
                frame: self.frame as u64 + 1,
                tick: self.game.hud().tick,
                component,
                expected: expected_hash,
                actual,
            });
            self.last_error = Some(error.clone());
            return Err(error);
        }
        self.frame += 1;
        Ok(true)
    }
    /// Restore the nearest preceding checkpoint and verify each remaining frame.
    /// A failed seek leaves this player intact; frame indices include resets.
    pub fn seek(&mut self, frame: usize) -> Result<(), ReplayError> {
        if frame > self.replay.frames.len() {
            return Err(ReplayError::End);
        }
        let cp = self
            .replay
            .checkpoints
            .iter()
            .rev()
            .find(|cp| cp.frame as usize <= frame);
        let (start, bytes) = cp.map_or((0, self.replay.initial.as_slice()), |cp| {
            (cp.frame as usize, cp.bytes.as_slice())
        });
        let game = Game::restore_checkpoint(self.game.resource_pack(), bytes)?;
        let expected = if start == 0 {
            self.replay.metadata.initial_hashes
        } else {
            self.replay.frames[start - 1].hashes
        };
        if game.identity() != self.replay.metadata.identity || game.state_hashes() != expected {
            return Err(ReplayError::Data("seek checkpoint"));
        }
        let mut candidate = Self {
            replay: self.replay.clone(),
            game,
            frame: start,
            last_error: None,
        };
        while candidate.frame < frame {
            candidate.step()?;
        }
        *self = candidate;
        Ok(())
    }
    pub fn fast_forward(&mut self, frames: usize) -> Result<usize, ReplayError> {
        let mut n = 0;
        while n < frames && self.step()? {
            n += 1;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_component_divergence_stops_and_a_valid_seek_recovers() {
        let mut r = GameRecorder::new(
            crate::language::conformance_game(),
            RecordingOptions {
                max_frames: 20,
                checkpoint_interval: 0,
            },
            "divergence",
        )
        .unwrap();
        for _ in 0..20 {
            r.step(GameInput {
                x: 1,
                ..GameInput::default()
            })
            .unwrap();
        }
        let mut replay = r.finish();
        replay.frames[9].hashes.player ^= 1;
        let replay = Arc::new(GameReplay::from_bytes(&replay.to_bytes().unwrap()).unwrap());
        let mut p = ReplayPlayer::<crate::language::ScriptStage>::new(
            replay,
            Arc::new(ResourcePack::builtin()),
        )
        .unwrap();
        for _ in 0..9 {
            p.step().unwrap();
        }
        let error = p.step().unwrap_err();
        assert!(matches!(
            error,
            ReplayError::Diverged(Divergence {
                frame: 10,
                component: Component::Player,
                ..
            })
        ));
        assert!(matches!(p.step(), Err(ReplayError::Stopped)));
        p.seek(9).unwrap();
        assert_eq!(p.frame(), 9);
        assert!(p.last_error().is_none());
    }
}
