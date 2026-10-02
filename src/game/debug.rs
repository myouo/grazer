//! Host-side runtime controls; pause/inspection/timing never enter game hashes.
use super::{
    checkpoint::CheckpointStage,
    replay::{GameRecorder, GameReplay, RecordingOptions, ReplayError, ReplayPlayer},
    *,
};
use crate::checkpoint::CheckpointError;
pub const PERFORMANCE_WINDOW: usize = 240;
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PerformanceSummary {
    pub samples: u32,
    pub update_ms: f64,
    pub draw_ms: f64,
    pub frame_ms: f64,
    pub update_p95_ms: f64,
    pub frame_p95_ms: f64,
}
#[derive(Clone, Copy, Default)]
struct Sample {
    update: f64,
    draw: f64,
    total: f64,
}
pub struct PerformancePanel {
    samples: [Sample; PERFORMANCE_WINDOW],
    next: usize,
    len: usize,
}
impl Default for PerformancePanel {
    fn default() -> Self {
        Self {
            samples: [Sample::default(); PERFORMANCE_WINDOW],
            next: 0,
            len: 0,
        }
    }
}
impl PerformancePanel {
    /// Hosts supply monotonic timing; invalid samples leave the panel unchanged.
    pub fn observe(&mut self, update_ms: f64, draw_ms: f64, total_ms: f64) -> bool {
        if ![update_ms, draw_ms, total_ms]
            .iter()
            .all(|v| v.is_finite() && (0.0..=1_000_000.0).contains(v))
        {
            return false;
        }
        self.samples[self.next] = Sample {
            update: update_ms,
            draw: draw_ms,
            total: total_ms,
        };
        self.next = (self.next + 1) % PERFORMANCE_WINDOW;
        self.len = (self.len + 1).min(PERFORMANCE_WINDOW);
        true
    }
    pub fn summary(&self) -> PerformanceSummary {
        if self.len == 0 {
            return PerformanceSummary::default();
        }
        let mut update = [0.0; PERFORMANCE_WINDOW];
        let mut total = [0.0; PERFORMANCE_WINDOW];
        for (i, s) in self.samples[..self.len].iter().enumerate() {
            update[i] = s.update;
            total[i] = s.total;
        }
        update[..self.len].sort_unstable_by(f64::total_cmp);
        total[..self.len].sort_unstable_by(f64::total_cmp);
        let index = (self.len * 95).div_ceil(100) - 1;
        let last = self.samples[(self.next + PERFORMANCE_WINDOW - 1) % PERFORMANCE_WINDOW];
        PerformanceSummary {
            samples: self.len as u32,
            update_ms: last.update,
            draw_ms: last.draw,
            frame_ms: last.total,
            update_p95_ms: update[index],
            frame_p95_ms: total[index],
        }
    }
}
#[derive(Debug)]
pub enum DebugError {
    Game(GameError),
    Replay(ReplayError),
    Checkpoint(CheckpointError),
    PausedRequired,
    Mode(&'static str),
}
impl std::fmt::Display for DebugError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Game(e) => write!(f, "{e}"),
            Self::Replay(e) => write!(f, "{e}"),
            Self::Checkpoint(e) => write!(f, "{e}"),
            Self::PausedRequired => f.write_str("pause before single-step/seek"),
            Self::Mode(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for DebugError {}
impl From<GameError> for DebugError {
    fn from(e: GameError) -> Self {
        Self::Game(e)
    }
}
impl From<ReplayError> for DebugError {
    fn from(e: ReplayError) -> Self {
        Self::Replay(e)
    }
}
impl From<CheckpointError> for DebugError {
    fn from(e: CheckpointError) -> Self {
        Self::Checkpoint(e)
    }
}
enum Runner<S: CheckpointStage> {
    Live(Box<Game<S>>),
    Recording(Box<GameRecorder<S>>),
    Playback(Box<ReplayPlayer<S>>),
}
pub struct DebugSession<S: CheckpointStage> {
    runner: Option<Runner<S>>,
    origin: Vec<u8>,
    paused: bool,
    hitboxes: bool,
    performance_visible: bool,
    performance: PerformancePanel,
}
impl<S: CheckpointStage> DebugSession<S> {
    pub fn new(game: Game<S>) -> Result<Self, DebugError> {
        let origin = game.checkpoint()?;
        Ok(Self {
            runner: Some(Runner::Live(Box::new(game))),
            origin,
            paused: false,
            hitboxes: false,
            performance_visible: false,
            performance: PerformancePanel::default(),
        })
    }
    pub fn game(&self) -> &Game<S> {
        match self.runner.as_ref().expect("session runner") {
            Runner::Live(g) => g,
            Runner::Recording(r) => r.game(),
            Runner::Playback(p) => p.game(),
        }
    }
    pub fn paused(&self) -> bool {
        self.paused
    }
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }
    pub fn hitboxes(&self) -> bool {
        self.hitboxes
    }
    pub fn set_hitboxes(&mut self, visible: bool) {
        self.hitboxes = visible;
    }
    pub fn performance_visible(&self) -> bool {
        self.performance_visible
    }
    pub fn set_performance_visible(&mut self, visible: bool) {
        self.performance_visible = visible;
    }
    pub fn performance(&self) -> PerformanceSummary {
        self.performance.summary()
    }
    pub fn observe_frame(&mut self, update_ms: f64, draw_ms: f64, total_ms: f64) -> bool {
        self.performance.observe(update_ms, draw_ms, total_ms)
    }
    pub fn recording(&self) -> bool {
        matches!(self.runner, Some(Runner::Recording(_)))
    }
    pub fn playback(&self) -> bool {
        matches!(self.runner, Some(Runner::Playback(_)))
    }
    pub fn replay_frame(&self) -> usize {
        match self.runner.as_ref().expect("runner") {
            Runner::Recording(r) => r.replay().frames().len(),
            Runner::Playback(p) => p.frame(),
            _ => 0,
        }
    }
    pub fn replay_length(&self) -> usize {
        match self.runner.as_ref().expect("runner") {
            Runner::Recording(r) => r.replay().frames().len(),
            Runner::Playback(p) => p.replay().frames().len(),
            _ => 0,
        }
    }
    fn run_step(&mut self, input: GameInput) -> Result<bool, DebugError> {
        let result = match self.runner.as_mut().expect("runner") {
            Runner::Live(g) => g.step(input).map(|()| true).map_err(DebugError::Game),
            Runner::Recording(r) => r.step(input).map(|_| true).map_err(DebugError::Replay),
            Runner::Playback(p) => p.step().map_err(DebugError::Replay),
        };
        if result.is_err() || matches!(result, Ok(false)) {
            self.paused = true;
        }
        result
    }
    /// Normal host tick: a paused session has no input, state or replay effects.
    pub fn advance(&mut self, input: GameInput) -> Result<bool, DebugError> {
        if self.paused {
            return Ok(false);
        }
        self.run_step(input)
    }
    /// One complete authoritative tick; no mid-instruction state is exposed.
    pub fn single_step(&mut self, input: GameInput) -> Result<bool, DebugError> {
        if !self.paused {
            return Err(DebugError::PausedRequired);
        }
        self.run_step(input)
    }
    /// Execute every intermediate tick; hosts suppress audio/rendering in this loop.
    pub fn fast_forward(&mut self, frames: usize, input: GameInput) -> Result<usize, DebugError> {
        if frames > super::replay::MAX_REPLAY_FRAMES {
            return Err(DebugError::Mode("fast-forward limit exceeded"));
        }
        let mut n = 0;
        while n < frames && self.run_step(input)? {
            n += 1;
        }
        Ok(n)
    }
    pub fn restart(&mut self) -> Result<(), DebugError> {
        match self.runner.as_mut().expect("runner") {
            Runner::Live(g) => **g = Game::restore_checkpoint(g.resource_pack(), &self.origin)?,
            Runner::Recording(r) => {
                r.reset()?;
            }
            Runner::Playback(p) => p.seek(0)?,
        }
        Ok(())
    }
    pub fn start_recording(
        &mut self,
        options: RecordingOptions,
        label: &str,
    ) -> Result<(), DebugError> {
        if self.recording() || self.playback() {
            return Err(DebugError::Mode("recording starts from a live game"));
        }
        let recorder = GameRecorder::new(self.game().clone(), options, label)?;
        self.runner = Some(Runner::Recording(Box::new(recorder)));
        Ok(())
    }
    pub fn stop_recording(&mut self) -> Result<GameReplay, DebugError> {
        if !self.recording() {
            return Err(DebugError::Mode("no active recording"));
        }
        let Some(Runner::Recording(r)) = self.runner.take() else {
            unreachable!("checked mode")
        };
        let (game, replay) = r.into_parts();
        self.runner = Some(Runner::Live(Box::new(game)));
        Ok(replay)
    }
    pub fn load_replay(
        &mut self,
        replay: Arc<GameReplay>,
    ) -> Result<Option<GameReplay>, DebugError> {
        if replay.metadata().identity.content_hash != self.game().identity().content_hash
            || replay.metadata().identity.stage_id != S::CONTENT_ID
        {
            return Err(CheckpointError::Stage.into());
        }
        let player = ReplayPlayer::new(replay, self.game().resource_pack())?;
        let old = self.archive_recording();
        self.runner = Some(Runner::Playback(Box::new(player)));
        self.paused = true;
        Ok(old)
    }
    pub fn seek(&mut self, frame: usize) -> Result<(), DebugError> {
        if !self.paused {
            return Err(DebugError::PausedRequired);
        }
        let Some(Runner::Playback(p)) = &mut self.runner else {
            return Err(DebugError::Mode("seek requires a loaded replay"));
        };
        p.seek(frame)?;
        Ok(())
    }
    fn archive_recording(&mut self) -> Option<GameReplay> {
        if self.recording() {
            Some(self.stop_recording().expect("checked recording"))
        } else {
            None
        }
    }
    /// Candidate construction/validation occurs before the old run is replaced.
    /// Return the old recording to the host; never mix content identities.
    pub fn replace_game(&mut self, game: Game<S>) -> Result<Option<GameReplay>, DebugError> {
        let origin = game.checkpoint()?;
        let old = self.archive_recording();
        self.runner = Some(Runner::Live(Box::new(game)));
        self.origin = origin;
        self.performance = PerformancePanel::default();
        self.paused = true;
        Ok(old)
    }
    pub fn restore(&mut self, bytes: &[u8]) -> Result<Option<GameReplay>, DebugError> {
        if super::checkpoint::CheckpointInfo::inspect(bytes)?
            .identity
            .content_hash
            != self.game().identity().content_hash
        {
            return Err(CheckpointError::Stage.into());
        }
        let candidate = Game::restore_checkpoint(self.game().resource_pack(), bytes)?;
        self.replace_game(candidate)
    }
}
impl DebugSession<crate::language::ScriptStage> {
    /// Source/resource reload restarts a validated candidate; failures keep the
    /// previous Game and recording. Active bytecode is never patched in place.
    pub fn reload(
        &mut self,
        file: &str,
        source: &str,
        resources: ResourcePack,
    ) -> Result<Option<GameReplay>, DebugError> {
        let old = self.game();
        let stage = crate::language::ScriptStage::compile(
            file,
            source,
            old.stage().vm().limits(),
            old.seed(),
        )
        .map_err(GameError::Script)?;
        self.reload_stage(stage, resources)
    }
    pub fn reload_stage(
        &mut self,
        stage: crate::language::ScriptStage,
        resources: ResourcePack,
    ) -> Result<Option<GameReplay>, DebugError> {
        let old = self.game();
        let candidate = if stage.vm().program().uses_advanced() {
            Game::with_advanced_stage(
                old.config(),
                old.seed(),
                resources,
                stage,
                old.simulation().advanced_config().unwrap_or_default(),
            )?
        } else {
            Game::with_stage(old.config(), old.seed(), resources, stage)?
        };
        self.replace_game(candidate)
    }
    pub fn practice(&mut self, phase: u32) -> Result<Option<GameReplay>, DebugError> {
        let old = self.game();
        let difficulty = old.simulation().difficulty().unwrap_or_default();
        let mut candidate =
            super::showcase::practice(phase, difficulty, old.config().simulation.player.health)?;
        candidate.resources = old.resource_pack();
        self.replace_game(candidate)
    }
}
