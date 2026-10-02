//! Self-contained distributable scripted game project. No filesystem, GPU,
//! clocks or audio device state enter this independently versioned archive.
use crate::{
    Game, GameConfig, GameError,
    advanced::AdvancedConfig,
    checkpoint::{CheckpointError, Reader, Writer},
    game::checkpoint::{GameIdentity, ResourceSummary},
    language::{Program, ScriptStage, VmLimits},
    resources::ResourcePack,
};
use std::sync::Arc;
pub const PROJECT_VERSION: u32 = 1;
pub const MAX_PROJECT_BYTES: usize = 96 * 1024 * 1024;
#[derive(Clone)]
pub struct Project {
    name: String,
    config: GameConfig,
    seed: u64,
    advanced: Option<AdvancedConfig>,
    limits: VmLimits,
    program: Arc<Program>,
    resources: Arc<ResourcePack>,
}
#[derive(Debug)]
pub enum ProjectError {
    Archive(CheckpointError),
    Game(GameError),
    Name,
}
impl From<CheckpointError> for ProjectError {
    fn from(e: CheckpointError) -> Self {
        Self::Archive(e)
    }
}
impl From<GameError> for ProjectError {
    fn from(e: GameError) -> Self {
        Self::Game(e)
    }
}
impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Archive(e) => write!(f, "project archive: {e}"),
            Self::Game(e) => write!(f, "project game: {e}"),
            Self::Name => {
                f.write_str("project name must be 1..256 bytes without control characters")
            }
        }
    }
}
impl std::error::Error for ProjectError {}
impl Project {
    pub fn new(
        name: &str,
        config: GameConfig,
        seed: u64,
        resources: ResourcePack,
        program: Arc<Program>,
        limits: VmLimits,
        advanced: Option<AdvancedConfig>,
    ) -> Result<Self, ProjectError> {
        if name.is_empty() || name.len() > 256 || name.chars().any(char::is_control) {
            return Err(ProjectError::Name);
        }
        let advanced = if program.uses_advanced() {
            Some(advanced.unwrap_or_default())
        } else {
            advanced
        };
        let project = Self {
            name: name.into(),
            config,
            seed,
            advanced,
            limits,
            program,
            resources: Arc::new(resources),
        };
        project.create_game()?;
        Ok(project)
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn program(&self) -> &Program {
        &self.program
    }
    pub fn resources(&self) -> &ResourcePack {
        &self.resources
    }
    pub fn resource_pack(&self) -> Arc<ResourcePack> {
        self.resources.clone()
    }
    pub fn config(&self) -> GameConfig {
        self.config
    }
    pub fn seed(&self) -> u64 {
        self.seed
    }
    pub fn limits(&self) -> VmLimits {
        self.limits
    }
    pub fn advanced_config(&self) -> Option<AdvancedConfig> {
        self.advanced
    }
    pub fn create_game(&self) -> Result<Game<ScriptStage>, GameError> {
        let stage = ScriptStage::new(self.program.clone(), self.limits, self.seed)
            .map_err(GameError::Script)?;
        if let Some(advanced) = self.advanced {
            Game::with_advanced_stage(
                self.config,
                self.seed,
                (*self.resources).clone(),
                stage,
                advanced,
            )
        } else {
            Game::with_stage(self.config, self.seed, (*self.resources).clone(), stage)
        }
    }
    fn identity(&self) -> GameIdentity {
        GameIdentity {
            game_protocol: if self.advanced.is_some() { 4 } else { 3 },
            simulation_protocol: if self.advanced.is_some() { 4 } else { 2 },
            stage_id: <ScriptStage as crate::Stage>::CONTENT_ID,
            content_hash: self.program.content_hash(),
            resources: ResourceSummary::of(&self.resources),
            config: self.config,
            advanced: self.advanced,
            seed: self.seed,
        }
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, ProjectError> {
        let mut w = Writer::new(b"GZPROJ01");
        w.u32(PROJECT_VERSION);
        w.u32(crate::TICK_RATE);
        w.text(&self.name);
        self.identity().write(&mut w);
        for n in [
            self.limits.tasks,
            self.limits.call_depth,
            self.limits.instructions_per_tick,
            self.limits.instructions_per_task,
            self.limits.commands_per_tick,
            self.limits.births_per_tick,
        ] {
            w.u32(n);
        }
        w.blob(&self.program.to_bytes());
        w.blob(&self.resources.to_bytes()?);
        Ok(w.finish(MAX_PROJECT_BYTES)?)
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProjectError> {
        let mut r = Reader::new(bytes, b"GZPROJ01", MAX_PROJECT_BYTES)?;
        if r.u32()? != PROJECT_VERSION || r.u32()? != crate::TICK_RATE {
            return Err(CheckpointError::Version.into());
        }
        let name = r.text(256)?;
        let identity = GameIdentity::read(&mut r)?;
        if identity.stage_id != <ScriptStage as crate::Stage>::CONTENT_ID {
            return Err(CheckpointError::Stage.into());
        }
        let limits = VmLimits {
            tasks: r.u32()?,
            call_depth: r.u32()?,
            instructions_per_tick: r.u32()?,
            instructions_per_task: r.u32()?,
            commands_per_tick: r.u32()?,
            births_per_tick: r.u32()?,
        };
        let program = Arc::new(
            Program::from_bytes(r.blob(16 * 1024 * 1024)?).map_err(CheckpointError::Script)?,
        );
        let resources = ResourcePack::from_bytes(r.blob(65 * 1024 * 1024)?)?;
        r.finish()?;
        if program.content_hash() != identity.content_hash
            || ResourceSummary::of(&resources) != identity.resources
        {
            return Err(CheckpointError::Fingerprint.into());
        }
        let project = Self::new(
            &name,
            identity.config,
            identity.seed,
            resources,
            program,
            limits,
            identity.advanced,
        )?;
        if project.identity() != identity {
            return Err(CheckpointError::Stage.into());
        }
        Ok(project)
    }
    pub fn showcase() -> Result<Self, ProjectError> {
        let stage = ScriptStage::showcase(42).map_err(GameError::Script)?;
        Self::new(
            "Prism Passage",
            GameConfig::default(),
            42,
            ResourcePack::builtin(),
            Arc::new(stage.vm().program().clone()),
            VmLimits::default(),
            Some(AdvancedConfig::default()),
        )
    }
}
