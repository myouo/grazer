//! Typed stage/bullet language and deterministic cooperative VM.
//!
//! ```
//! use grazer::language::{Program, Vm, VmLimits};
//! use grazer::{Simulation, SimulationConfig};
//! use std::sync::Arc;
//! let program = Arc::new(Program::compile("demo.graze", "task main() { wave(1); wait(1); complete(); }")?);
//! let mut vm = Vm::new(program.clone(), VmLimits::default(), 42)?;
//! let mut world = Simulation::new(SimulationConfig::default(), 42)?;
//! assert_eq!(vm.update(&mut world)?.wave, 1);
//! world.step()?;
//! let mut restored = Vm::restore(program, &vm.save())?;
//! assert!(restored.update(&mut world)?.complete);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
mod codec;
mod compiler;
mod debug;
mod program;
mod stage;
mod vm;
use crate::{EntityHandle, Fixed, Vec2};
pub use debug::{FrameView, TaskState, TaskView};
pub use program::Program;
pub use stage::{ScriptStage, conformance_game, restore_fixture_hash, trace};
pub use vm::{Vm, VmLimits};
pub const BYTECODE_VERSION: u32 = 2;
pub const VM_STATE_VERSION: u32 = 1;
pub const VM_PROTOCOL_VERSION: u32 = 2;
pub const MAX_REGISTERS: usize = 64;
pub const MAX_ARGUMENTS: usize = 8;
pub const MAX_FUNCTIONS: usize = 128;
pub const MAX_INSTRUCTIONS: usize = 65536;
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Type {
    Unit = 0,
    Int = 1,
    Bool = 2,
    Fixed = 3,
    Vec = 4,
    Entity = 5,
    Task = 6,
}
impl Type {
    pub(crate) fn decode(tag: u8) -> Option<Self> {
        Some(match tag {
            0 => Self::Unit,
            1 => Self::Int,
            2 => Self::Bool,
            3 => Self::Fixed,
            4 => Self::Vec,
            5 => Self::Entity,
            6 => Self::Task,
            _ => return None,
        })
    }
    pub(crate) fn zero(self) -> Value {
        match self {
            Self::Unit => Value::Unit,
            Self::Int => Value::Int(0),
            Self::Bool => Value::Bool(false),
            Self::Fixed => Value::Fixed(Fixed::ZERO),
            Self::Vec => Value::Vec(Vec2::ZERO),
            Self::Entity => Value::Entity(None),
            Self::Task => Value::Task(None),
        }
    }
}
impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unit => "unit",
            Self::Int => "int",
            Self::Bool => "bool",
            Self::Fixed => "fixed",
            Self::Vec => "vec",
            Self::Entity => "entity",
            Self::Task => "task",
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskHandle {
    pub slot: u32,
    pub generation: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    Unit,
    Int(i32),
    Bool(bool),
    Fixed(Fixed),
    Vec(Vec2),
    Entity(Option<EntityHandle>),
    Task(Option<TaskHandle>),
}
impl Value {
    pub fn ty(self) -> Type {
        match self {
            Self::Unit => Type::Unit,
            Self::Int(_) => Type::Int,
            Self::Bool(_) => Type::Bool,
            Self::Fixed(_) => Type::Fixed,
            Self::Vec(_) => Type::Vec,
            Self::Entity(_) => Type::Entity,
            Self::Task(_) => Type::Task,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum DiagnosticKind {
    Syntax = 1,
    Type = 2,
    Bytecode = 3,
    Overflow = 4,
    DivisionByZero = 5,
    Argument = 6,
    Entity = 7,
    Task = 8,
    InstructionBudget = 9,
    CommandBudget = 10,
    TaskBudget = 11,
    CallDepth = 12,
    Snapshot = 13,
    Host = 14,
}
impl DiagnosticKind {
    pub(crate) fn decode(v: u32) -> Option<Self> {
        Some(match v {
            1 => Self::Syntax,
            2 => Self::Type,
            3 => Self::Bytecode,
            4 => Self::Overflow,
            5 => Self::DivisionByZero,
            6 => Self::Argument,
            7 => Self::Entity,
            8 => Self::Task,
            9 => Self::InstructionBudget,
            10 => Self::CommandBudget,
            11 => Self::TaskBudget,
            12 => Self::CallDepth,
            13 => Self::Snapshot,
            14 => Self::Host,
            _ => return None,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub file: String,
    pub span: Span,
    pub line: u32,
    pub column: u32,
    pub function: String,
    pub task: Option<TaskHandle>,
    pub message: String,
}
impl Diagnostic {
    pub(crate) fn new(
        kind: DiagnosticKind,
        file: &str,
        source: &str,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        let mut start = (span.start as usize).min(source.len());
        while !source.is_char_boundary(start) {
            start -= 1;
        }
        let prefix = &source[..start];
        let line = prefix.bytes().filter(|&b| b == b'\n').count() as u32 + 1;
        let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() as u32 + 1;
        Self {
            kind,
            file: file.into(),
            span,
            line,
            column,
            function: String::new(),
            task: None,
            message: message.into(),
        }
    }
    pub(crate) fn data(kind: DiagnosticKind, message: impl Into<String>) -> Self {
        Self::new(kind, "<binary>", "", Span::default(), message)
    }
}
impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}",
            self.file, self.line, self.column, self.message
        )?;
        if !self.function.is_empty() {
            write!(f, " ({}", self.function)?;
            if let Some(task) = self.task {
                write!(f, ", task {}:{}", task.slot, task.generation)?;
            }
            write!(f, ")")?;
        }
        Ok(())
    }
}
impl std::error::Error for Diagnostic {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Binary {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}
impl Binary {
    pub fn decode(tag: u8) -> Option<Self> {
        Some(match tag {
            0 => Self::Add,
            1 => Self::Sub,
            2 => Self::Mul,
            3 => Self::Div,
            4 => Self::Rem,
            5 => Self::Eq,
            6 => Self::Ne,
            7 => Self::Lt,
            8 => Self::Le,
            9 => Self::Gt,
            10 => Self::Ge,
            _ => return None,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unary {
    Neg,
    Not,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Builtin {
    Vector = 0,
    Fixed = 1,
    Int = 2,
    X = 3,
    Y = 4,
    Width = 5,
    Height = 6,
    Player = 7,
    Tick = 8,
    Random = 9,
    Aim = 10,
    Enemy = 11,
    Emit = 12,
    Alive = 13,
    Position = 14,
    Move = 15,
    ClearEnemies = 16,
    ClearShots = 17,
    Wave = 18,
    Boss = 19,
    Complete = 20,
    Wait = 21,
    Join = 22,
    Cancel = 23,
    CancelChildren = 24,
    Attach = 25,
    CurrentTask = 26,
    Polar = 27,
    Ring = 28,
    Fan = 29,
    Aimed = 30,
    Spiral = 31,
    Compose = 32,
    Laser = 33,
    CurveLaser = 34,
    Drop = 35,
    EnemyDrop = 36,
    Difficulty = 37,
    Phase = 38,
    Despawn = 39,
    CancelShots = 40,
    Colour = 41,
}
impl Builtin {
    pub fn decode(tag: u8) -> Option<Self> {
        Self::ALL.get(tag as usize).copied()
    }
    pub const ALL: [Self; 42] = [
        Self::Vector,
        Self::Fixed,
        Self::Int,
        Self::X,
        Self::Y,
        Self::Width,
        Self::Height,
        Self::Player,
        Self::Tick,
        Self::Random,
        Self::Aim,
        Self::Enemy,
        Self::Emit,
        Self::Alive,
        Self::Position,
        Self::Move,
        Self::ClearEnemies,
        Self::ClearShots,
        Self::Wave,
        Self::Boss,
        Self::Complete,
        Self::Wait,
        Self::Join,
        Self::Cancel,
        Self::CancelChildren,
        Self::Attach,
        Self::CurrentTask,
        Self::Polar,
        Self::Ring,
        Self::Fan,
        Self::Aimed,
        Self::Spiral,
        Self::Compose,
        Self::Laser,
        Self::CurveLaser,
        Self::Drop,
        Self::EnemyDrop,
        Self::Difficulty,
        Self::Phase,
        Self::Despawn,
        Self::CancelShots,
        Self::Colour,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Vector => "vec",
            Self::Fixed => "fixed",
            Self::Int => "int",
            Self::X => "x",
            Self::Y => "y",
            Self::Width => "width",
            Self::Height => "height",
            Self::Player => "player",
            Self::Tick => "tick",
            Self::Random => "random",
            Self::Aim => "aim",
            Self::Enemy => "enemy",
            Self::Emit => "emit",
            Self::Alive => "alive",
            Self::Position => "position",
            Self::Move => "move",
            Self::ClearEnemies => "clear_enemies",
            Self::ClearShots => "clear_shots",
            Self::Wave => "wave",
            Self::Boss => "boss",
            Self::Complete => "complete",
            Self::Wait => "wait",
            Self::Join => "join",
            Self::Cancel => "cancel",
            Self::CancelChildren => "cancel_children",
            Self::Attach => "attach",
            Self::CurrentTask => "current_task",
            Self::Polar => "polar",
            Self::Ring => "ring",
            Self::Fan => "fan",
            Self::Aimed => "aimed",
            Self::Spiral => "spiral",
            Self::Compose => "compose",
            Self::Laser => "laser",
            Self::CurveLaser => "curve_laser",
            Self::Drop => "drop",
            Self::EnemyDrop => "enemy_drop",
            Self::Difficulty => "difficulty",
            Self::Phase => "phase",
            Self::Despawn => "despawn",
            Self::CancelShots => "cancel_shots",
            Self::Colour => "colour",
        }
    }
    pub fn signature(self) -> (&'static [Type], Type) {
        use Type::*;
        match self {
            Self::Vector => (&[Fixed, Fixed], Vec),
            Self::Fixed => (&[Int], Fixed),
            Self::Int => (&[Fixed], Int),
            Self::X | Self::Y => (&[Vec], Fixed),
            Self::Width | Self::Height => (&[], Fixed),
            Self::Player => (&[], Vec),
            Self::Tick | Self::Random => (&[], Int),
            Self::Aim => (&[Vec, Vec, Fixed], Vec),
            Self::Enemy => (&[Vec, Vec, Fixed, Int, Int, Int], Entity),
            Self::Emit => (&[Vec, Vec, Fixed, Int, Int], Entity),
            Self::Alive => (&[Entity], Bool),
            Self::Position => (&[Entity], Vec),
            Self::Move => (&[Entity, Vec], Unit),
            Self::ClearEnemies | Self::ClearShots | Self::Complete | Self::CancelChildren => {
                (&[], Unit)
            }
            Self::Wave | Self::Wait => (&[Int], Unit),
            Self::Boss => (&[Entity, Int], Unit),
            Self::Join | Self::Cancel => (&[Task], Unit),
            Self::Attach => (&[Entity], Unit),
            Self::CurrentTask => (&[], Task),
            Self::Polar => (&[Fixed, Fixed], Vec),
            Self::Ring => (&[Vec, Int, Fixed, Fixed, Fixed, Int], Unit),
            Self::Fan | Self::Spiral => (&[Vec, Int, Fixed, Fixed, Fixed, Fixed, Int], Unit),
            Self::Aimed => (&[Vec, Vec, Int, Fixed, Fixed, Fixed, Int], Unit),
            Self::Compose => (&[Entity, Vec, Fixed], Unit),
            Self::Laser => (&[Vec, Vec, Fixed, Int, Int, Int], Entity),
            Self::CurveLaser => (&[Vec, Vec, Vec, Fixed, Int, Int, Int], Entity),
            Self::Drop => (&[Vec, Int, Int], Entity),
            Self::EnemyDrop | Self::Phase => (&[Entity, Int, Int], Unit),
            Self::Difficulty => (&[], Int),
            Self::Despawn => (&[Entity], Unit),
            Self::CancelShots => (&[Bool], Int),
            Self::Colour => (&[Entity, Int], Unit),
        }
    }
    pub fn task_only(self) -> bool {
        matches!(
            self,
            Self::Wait
                | Self::Join
                | Self::Cancel
                | Self::CancelChildren
                | Self::Attach
                | Self::CurrentTask
        )
    }
    pub fn command(self) -> bool {
        matches!(
            self,
            Self::Enemy
                | Self::Emit
                | Self::Move
                | Self::ClearEnemies
                | Self::ClearShots
                | Self::Wave
                | Self::Boss
                | Self::Complete
                | Self::Ring
                | Self::Fan
                | Self::Aimed
                | Self::Spiral
                | Self::Compose
                | Self::Laser
                | Self::CurveLaser
                | Self::Drop
                | Self::EnemyDrop
                | Self::Phase
                | Self::Despawn
                | Self::CancelShots
                | Self::Colour
        )
    }
    pub fn command_cost(self, args: &[Value; 8]) -> u32 {
        if !self.command() {
            return 0;
        }
        let count = match self {
            Self::Ring | Self::Fan | Self::Spiral => Some(1),
            Self::Aimed => Some(2),
            _ => None,
        };
        if let Some(index) = count
            && let Value::Int(n) = args[index]
        {
            return n.max(1) as u32;
        }
        1
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    Const {
        dst: u8,
        value: Value,
    },
    Copy {
        dst: u8,
        src: u8,
    },
    Unary {
        dst: u8,
        src: u8,
        op: Unary,
    },
    Binary {
        dst: u8,
        left: u8,
        right: u8,
        op: Binary,
    },
    Jump {
        target: u32,
    },
    Branch {
        condition: u8,
        when: bool,
        target: u32,
    },
    Call {
        dst: u8,
        function: u16,
        args: [u8; 8],
        len: u8,
    },
    Fork {
        dst: u8,
        function: u16,
        args: [u8; 8],
        len: u8,
    },
    Builtin {
        dst: u8,
        builtin: Builtin,
        args: [u8; 8],
        len: u8,
    },
    Return {
        value: Option<u8>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Instruction {
    pub op: Op,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub(crate) struct Function {
    pub name: String,
    pub params: Vec<Type>,
    pub result: Type,
    pub task: bool,
    pub registers: Vec<Type>,
    pub code: Vec<Instruction>,
}
