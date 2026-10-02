//! Borrowed VM inspection at tick boundaries, with source maps and typed values.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Waiting,
    Joining,
}
pub struct TaskView<'a> {
    pub handle: TaskHandle,
    pub parent: Option<TaskHandle>,
    pub owner: Option<EntityHandle>,
    pub state: TaskState,
    pub wake_tick: u64,
    pub joining: Option<TaskHandle>,
    pub frames: Vec<FrameView<'a>>,
}
pub struct FrameView<'a> {
    pub function: &'a str,
    pub pc: u32,
    pub span: Span,
    pub line: u32,
    pub column: u32,
    pub registers: &'a [Value],
}
impl Vm {
    /// Allocate only on explicit inspection; ordinary VM ticks remain unchanged.
    pub fn inspect_tasks(&self) -> Vec<TaskView<'_>> {
        self.order
            .iter()
            .map(|&i| {
                let s = &self.slots[i as usize];
                let frames = s
                    .frames
                    .iter()
                    .map(|frame| {
                        let f = &self.program.functions[frame.function as usize];
                        let span = f
                            .code
                            .get(frame.pc as usize)
                            .or_else(|| f.code.last())
                            .map_or(Span::default(), |i| i.span);
                        let before = &self.program.source[..span.start as usize];
                        let line = before.bytes().filter(|&b| b == b'\n').count() as u32 + 1;
                        let column =
                            before.rsplit('\n').next().unwrap_or("").chars().count() as u32 + 1;
                        FrameView {
                            function: &f.name,
                            pc: frame.pc,
                            span,
                            line,
                            column,
                            registers: &frame.registers[..f.registers.len()],
                        }
                    })
                    .collect();
                TaskView {
                    handle: TaskHandle {
                        slot: i,
                        generation: s.generation,
                    },
                    parent: s.parent,
                    owner: s.owner,
                    state: if s.joining.is_some_and(|h| self.live(h)) {
                        TaskState::Joining
                    } else if s.wake > self.last_tick.map_or(0, |t| t.saturating_add(1)) {
                        TaskState::Waiting
                    } else {
                        TaskState::Ready
                    },
                    wake_tick: s.wake,
                    joining: s.joining,
                    frames,
                }
            })
            .collect()
    }
}
