use super::*;
use crate::{
    Simulation, SimulationError,
    game::{Game, GameConfig, Stage, StageStatus},
    resources::ResourcePack,
};
use std::sync::Arc;
#[derive(Clone)]
pub struct ScriptStage {
    vm: Vm,
}
impl ScriptStage {
    pub(crate) fn from_vm(vm: Vm) -> Self {
        Self { vm }
    }
    pub fn new(program: Arc<Program>, limits: VmLimits, seed: u64) -> Result<Self, Diagnostic> {
        Ok(Self {
            vm: Vm::new(program, limits, seed)?,
        })
    }
    pub fn compile(
        file: &str,
        source: &str,
        limits: VmLimits,
        seed: u64,
    ) -> Result<Self, Diagnostic> {
        Self::new(Arc::new(Program::compile(file, source)?), limits, seed)
    }
    pub fn builtin(seed: u64) -> Result<Self, Diagnostic> {
        Self::compile(
            "first_sortie.graze",
            include_str!("../../assets/demo/first_sortie.graze"),
            VmLimits::default(),
            seed,
        )
    }
    pub fn showcase(seed: u64) -> Result<Self, Diagnostic> {
        Self::compile(
            "advanced_showcase.graze",
            include_str!("../../assets/demo/advanced_showcase.graze"),
            VmLimits::default(),
            seed,
        )
    }
    pub fn vm(&self) -> &Vm {
        &self.vm
    }
    pub fn save(&self) -> Vec<u8> {
        self.vm.save()
    }
    pub fn restore(program: Arc<Program>, bytes: &[u8]) -> Result<Self, Diagnostic> {
        Ok(Self {
            vm: Vm::restore(program, bytes)?,
        })
    }
}
impl Stage for ScriptStage {
    const CONTENT_ID: u64 = 0x47525a4d33000001;
    fn update(&mut self, world: &mut Simulation) -> Result<StageStatus, SimulationError> {
        self.vm
            .update(world)
            .map_err(|_| SimulationError::Exhausted)
    }
    fn state_hash(&self) -> u64 {
        self.vm.state_hash()
    }
    fn diagnostic(&self) -> Option<&Diagnostic> {
        self.vm.diagnostic()
    }
    fn after_step(&mut self, world: &Simulation) {
        self.vm.prune_dead_owners(world);
    }
    fn advanced_config(&self) -> Option<crate::advanced::AdvancedConfig> {
        self.vm
            .program()
            .uses_advanced()
            .then(crate::advanced::AdvancedConfig::default)
    }
}
pub fn conformance_game() -> Game<ScriptStage> {
    let mut config = GameConfig::default();
    config.simulation.projectile_capacity = 512;
    config.simulation.player.health = 10000;
    Game::with_stage(
        config,
        42,
        ResourcePack::builtin(),
        ScriptStage::builtin(42).expect("shipped stage compiles"),
    )
    .expect("valid script game")
}
pub fn trace(frames: u32) -> Vec<u64> {
    let mut game = conformance_game();
    (0..frames)
        .map(|frame| {
            game.step(crate::game::conformance_input(u64::from(frame)))
                .expect("bounded script content");
            game.state_hash()
        })
        .collect()
}

/// Serialized VM/RNG continuation fixture, also executed by the raw WASM host.
pub fn restore_fixture_hash(ticks: u32) -> u64 {
    let program=Arc::new(Program::compile("restore.graze","task main() { let n = 0; while n < 10000 { wave(random() % 1000); n = n + 1; wait(1); } }").expect("fixture"));
    let mut vm = Vm::new(program.clone(), VmLimits::default(), 42).expect("limits");
    let mut world = Simulation::new(
        crate::SimulationConfig {
            projectile_capacity: 1,
            enemy_capacity: 1,
            ..crate::SimulationConfig::default()
        },
        42,
    )
    .expect("config");
    let ticks = ticks.min(10000);
    for _ in 0..ticks / 2 {
        vm.update(&mut world).expect("update");
        world.step().expect("tick");
    }
    let mut restored = Vm::restore(program, &vm.save()).expect("restore");
    let mut restored_world = world.clone();
    for _ in ticks / 2..ticks {
        vm.update(&mut world).expect("update");
        world.step().expect("tick");
        restored
            .update(&mut restored_world)
            .expect("restored update");
        restored_world.step().expect("restored tick");
        assert_eq!(restored.state_hash(), vm.state_hash());
        assert_eq!(restored_world.state_hash(), world.state_hash());
    }
    vm.state_hash()
}
