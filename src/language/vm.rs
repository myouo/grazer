use super::*;
use crate::{
    BoundsBehavior, Collider, Enemy, EntityKind, Faction, Projectile, Simulation, SimulationError,
    game::StageStatus, resources::Fingerprint,
};
use std::sync::Arc;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VmLimits {
    pub tasks: u32,
    pub call_depth: u32,
    pub instructions_per_tick: u32,
    pub instructions_per_task: u32,
    pub commands_per_tick: u32,
    pub births_per_tick: u32,
}
impl Default for VmLimits {
    fn default() -> Self {
        Self {
            tasks: 32,
            call_depth: 8,
            instructions_per_tick: 16384,
            instructions_per_task: 4096,
            commands_per_tick: 512,
            births_per_tick: 16,
        }
    }
}
impl VmLimits {
    pub(crate) fn validate(self) -> Result<(), Diagnostic> {
        if !(1..=256).contains(&self.tasks)
            || !(1..=16).contains(&self.call_depth)
            || !(1..=1_000_000).contains(&self.instructions_per_tick)
            || !(1..=self.instructions_per_tick).contains(&self.instructions_per_task)
            || !(1..=8192).contains(&self.commands_per_tick)
            || !(1..=self.tasks).contains(&self.births_per_tick)
        {
            return Err(Diagnostic::data(
                DiagnosticKind::Argument,
                "invalid VM limits",
            ));
        }
        Ok(())
    }
}
#[derive(Clone)]
pub(crate) struct Frame {
    pub function: u16,
    pub pc: u32,
    pub return_dst: u8,
    pub registers: [Value; 64],
}
impl Frame {
    fn new(program: &Program, function: u16, args: &[Value], return_dst: u8) -> Self {
        let function_data = &program.functions[function as usize];
        let mut registers = [Value::Unit; 64];
        for (index, &ty) in function_data.registers.iter().enumerate() {
            registers[index] = ty.zero();
        }
        registers[..args.len()].copy_from_slice(args);
        Self {
            function,
            pc: 0,
            return_dst,
            registers,
        }
    }
}
pub(crate) struct Slot {
    pub generation: u32,
    pub active: bool,
    pub done: bool,
    pub parent: Option<TaskHandle>,
    pub owner: Option<EntityHandle>,
    pub wake: u64,
    pub joining: Option<TaskHandle>,
    pub frames: Vec<Frame>,
}
pub struct Vm {
    pub(crate) program: Arc<Program>,
    pub(crate) limits: VmLimits,
    pub(crate) slots: Vec<Slot>,
    pub(crate) order: Vec<u32>,
    pub(crate) free: Vec<u32>,
    pub(crate) rng: u64,
    pub(crate) last_tick: Option<u64>,
    pub(crate) status: StageStatus,
    pub(crate) fault: Option<Diagnostic>,
    pub(crate) last_instructions: u32,
}
impl Clone for Vm {
    fn clone(&self) -> Self {
        let slots = self
            .slots
            .iter()
            .map(|slot| {
                let mut frames = Vec::with_capacity(self.limits.call_depth as usize);
                frames.extend_from_slice(&slot.frames);
                Slot {
                    generation: slot.generation,
                    active: slot.active,
                    done: slot.done,
                    parent: slot.parent,
                    owner: slot.owner,
                    wake: slot.wake,
                    joining: slot.joining,
                    frames,
                }
            })
            .collect();
        let mut order = Vec::with_capacity(self.limits.tasks as usize);
        order.extend_from_slice(&self.order);
        let mut free = Vec::with_capacity(self.limits.tasks as usize);
        free.extend_from_slice(&self.free);
        Self {
            program: self.program.clone(),
            limits: self.limits,
            slots,
            order,
            free,
            rng: self.rng,
            last_tick: self.last_tick,
            status: self.status,
            fault: self.fault.clone(),
            last_instructions: self.last_instructions,
        }
    }
}
#[derive(Default)]
struct Budget {
    instructions: u32,
    commands: u32,
    births: u32,
}
enum Control {
    Continue,
    Wait(u64),
    Join(TaskHandle),
}
impl Vm {
    pub fn new(program: Arc<Program>, limits: VmLimits, seed: u64) -> Result<Self, Diagnostic> {
        limits.validate()?;
        let slots = (0..limits.tasks)
            .map(|_| Slot {
                generation: 1,
                active: false,
                done: false,
                parent: None,
                owner: None,
                wake: 0,
                joining: None,
                frames: Vec::with_capacity(limits.call_depth as usize),
            })
            .collect();
        let mut vm = Self {
            program,
            limits,
            slots,
            order: Vec::with_capacity(limits.tasks as usize),
            free: (0..limits.tasks).rev().collect(),
            rng: seed,
            last_tick: None,
            status: StageStatus::default(),
            fault: None,
            last_instructions: 0,
        };
        vm.start(vm.program.entry, &[], None)
            .expect("one root task fits validated limits");
        Ok(vm)
    }
    pub fn program(&self) -> &Program {
        &self.program
    }
    pub fn limits(&self) -> VmLimits {
        self.limits
    }
    pub fn task_count(&self) -> usize {
        self.order.len()
    }
    pub fn status(&self) -> StageStatus {
        self.status
    }
    pub fn diagnostic(&self) -> Option<&Diagnostic> {
        self.fault.as_ref()
    }
    pub fn last_instruction_count(&self) -> u32 {
        self.last_instructions
    }
    pub fn save(&self) -> Vec<u8> {
        super::codec::write_vm(self)
    }
    pub fn restore(program: Arc<Program>, bytes: &[u8]) -> Result<Self, Diagnostic> {
        super::codec::read_vm(program, bytes)
    }
    pub fn task_handles(&self) -> impl Iterator<Item = TaskHandle> + '_ {
        self.order.iter().map(|&slot| TaskHandle {
            slot,
            generation: self.slots[slot as usize].generation,
        })
    }
    fn live(&self, handle: TaskHandle) -> bool {
        self.slots
            .get(handle.slot as usize)
            .is_some_and(|s| s.active && !s.done && s.generation == handle.generation)
    }
    fn start(
        &mut self,
        function: u16,
        args: &[Value],
        parent: Option<TaskHandle>,
    ) -> Result<TaskHandle, DiagnosticKind> {
        let slot = self.free.pop().ok_or(DiagnosticKind::TaskBudget)?;
        let handle = TaskHandle {
            slot,
            generation: self.slots[slot as usize].generation,
        };
        let state = &mut self.slots[slot as usize];
        state.active = true;
        state.done = false;
        state.parent = parent;
        state.owner = None;
        state.wake = 0;
        state.joining = None;
        state.frames.clear();
        state
            .frames
            .push(Frame::new(&self.program, function, args, u8::MAX));
        self.order.push(slot);
        Ok(handle)
    }
    fn descendant(&self, mut child: TaskHandle, ancestor: TaskHandle) -> bool {
        for _ in 0..self.slots.len() {
            let Some(slot) = self.slots.get(child.slot as usize) else {
                return false;
            };
            if slot.generation != child.generation {
                return false;
            }
            let Some(parent) = slot.parent else {
                return false;
            };
            if parent == ancestor {
                return true;
            }
            child = parent;
        }
        false
    }
    fn cancel_inner(&mut self, handle: TaskHandle, include_root: bool) {
        if !self.live(handle) {
            return;
        }
        for index in 0..self.slots.len() {
            let h = TaskHandle {
                slot: index as u32,
                generation: self.slots[index].generation,
            };
            if self.slots[index].active
                && (include_root && h == handle || self.descendant(h, handle))
            {
                self.slots[index].done = true;
            }
        }
    }
    /// Boundary cancellation is idempotent for completed/stale handles.
    pub fn cancel(&mut self, handle: TaskHandle) {
        self.cancel_inner(handle, true);
        self.compact();
    }
    fn compact(&mut self) {
        let mut write = 0;
        for read in 0..self.order.len() {
            let index = self.order[read];
            let slot = &mut self.slots[index as usize];
            if slot.done {
                slot.active = false;
                slot.done = false;
                slot.frames.clear();
                slot.parent = None;
                slot.owner = None;
                slot.joining = None;
                slot.wake = 0;
                if let Some(next) = slot.generation.checked_add(1) {
                    slot.generation = next;
                    self.free.push(index);
                }
            } else {
                self.order[write] = index;
                write += 1;
            }
        }
        self.order.truncate(write);
    }
    /// Owner death cancels its task subtree before the next update, including
    /// immediately after the Game's collision/destruction phase.
    pub fn prune_dead_owners(&mut self, world: &Simulation) {
        for index in 0..self.slots.len() {
            if self.slots[index].active
                && !self.slots[index].done
                && self.slots[index]
                    .owner
                    .is_some_and(|h| !entity_alive(world, h))
            {
                let handle = TaskHandle {
                    slot: index as u32,
                    generation: self.slots[index].generation,
                };
                self.cancel_inner(handle, true);
            }
        }
        self.compact();
    }
    pub fn update(&mut self, world: &mut Simulation) -> Result<StageStatus, Diagnostic> {
        if let Some(fault) = &self.fault {
            return Err(fault.clone());
        }
        let tick = world.tick();
        if self
            .last_tick
            .is_some_and(|last| last.checked_add(1) != Some(tick))
        {
            return self.stop(self.program.diagnostic(
                DiagnosticKind::Snapshot,
                self.program.entry as usize,
                0,
                "VM requires the matching next simulation tick",
            ));
        }
        self.last_tick = Some(tick);
        self.prune_dead_owners(world);
        let mut budget = Budget::default();
        let mut cursor = 0;
        while cursor < self.order.len() {
            let index = self.order[cursor] as usize;
            cursor += 1;
            let slot = &self.slots[index];
            if slot.done || slot.wake > tick || slot.joining.is_some_and(|h| self.live(h)) {
                continue;
            }
            self.slots[index].joining = None;
            if let Err(diagnostic) = self.execute(index, world, &mut budget) {
                self.last_instructions = budget.instructions;
                self.compact();
                return self.stop(diagnostic);
            }
        }
        self.last_instructions = budget.instructions;
        self.compact();
        Ok(self.status)
    }
    fn stop<T>(&mut self, diagnostic: Diagnostic) -> Result<T, Diagnostic> {
        self.fault = Some(diagnostic.clone());
        Err(diagnostic)
    }
    fn execute(
        &mut self,
        index: usize,
        world: &mut Simulation,
        budget: &mut Budget,
    ) -> Result<(), Diagnostic> {
        let mut steps = 0;
        loop {
            if self.slots[index].done {
                return Ok(());
            }
            let frame = self.slots[index].frames.last().expect("live task frame");
            let function = frame.function as usize;
            let pc = frame.pc as usize;
            let make_error = |kind, message: &str| {
                let mut d = self.program.diagnostic(kind, function, pc, message);
                d.task = Some(TaskHandle {
                    slot: index as u32,
                    generation: self.slots[index].generation,
                });
                d
            };
            if steps == self.limits.instructions_per_task
                || budget.instructions == self.limits.instructions_per_tick
            {
                return Err(make_error(
                    DiagnosticKind::InstructionBudget,
                    "instruction budget exhausted; VM paused",
                ));
            }
            let Some(instruction) = self.program.functions[function].code.get(pc).copied() else {
                return Err(make_error(DiagnosticKind::Bytecode, "invalid execution PC"));
            };
            steps += 1;
            budget.instructions += 1;
            self.slots[index].frames.last_mut().expect("frame").pc += 1;
            let result = (|| -> Result<(), (DiagnosticKind, &'static str)> {
                match instruction.op {
                    Op::Const { dst, value } => self.store(index, dst, value),
                    Op::Copy { dst, src } => self.store(index, dst, self.load(index, src)),
                    Op::Unary { dst, src, op } => {
                        let value = unary(op, self.load(index, src))?;
                        self.store(index, dst, value);
                    }
                    Op::Binary {
                        dst,
                        left,
                        right,
                        op,
                    } => {
                        let value = binary(op, self.load(index, left), self.load(index, right))?;
                        self.store(index, dst, value);
                    }
                    Op::Jump { target } => {
                        self.slots[index].frames.last_mut().expect("frame").pc = target
                    }
                    Op::Branch {
                        condition,
                        when,
                        target,
                    } => {
                        if self.load(index, condition) == Value::Bool(when) {
                            self.slots[index].frames.last_mut().expect("frame").pc = target;
                        }
                    }
                    Op::Call {
                        dst,
                        function,
                        args,
                        len,
                    } => {
                        if self.slots[index].frames.len() == self.limits.call_depth as usize {
                            return Err((DiagnosticKind::CallDepth, "call depth limit exceeded"));
                        }
                        let values = self.arguments(index, args);
                        self.slots[index].frames.push(Frame::new(
                            &self.program,
                            function,
                            &values[..len as usize],
                            dst,
                        ));
                    }
                    Op::Fork {
                        dst,
                        function,
                        args,
                        len,
                    } => {
                        if budget.births == self.limits.births_per_tick {
                            return Err((DiagnosticKind::TaskBudget, "task birth budget exceeded"));
                        }
                        budget.births += 1;
                        let args = self.arguments(index, args);
                        let parent = Some(TaskHandle {
                            slot: index as u32,
                            generation: self.slots[index].generation,
                        });
                        let handle = self
                            .start(function, &args[..len as usize], parent)
                            .map_err(|kind| (kind, "task capacity exhausted"))?;
                        self.store(index, dst, Value::Task(Some(handle)));
                    }
                    Op::Builtin {
                        dst, builtin, args, ..
                    } => {
                        let args = self.arguments(index, args);
                        let cost = builtin.command_cost(&args);
                        if cost > 0 {
                            if cost
                                > self
                                    .limits
                                    .commands_per_tick
                                    .saturating_sub(budget.commands)
                            {
                                return Err((
                                    DiagnosticKind::CommandBudget,
                                    "host command budget exhausted",
                                ));
                            }
                            budget.commands += cost;
                        }
                        let (value, control) = self.builtin(index, builtin, args, world)?;
                        self.store(index, dst, value);
                        match control {
                            Control::Continue => {}
                            Control::Wait(wake) => self.slots[index].wake = wake,
                            Control::Join(task) => self.slots[index].joining = Some(task),
                        }
                    }
                    Op::Return { value } => {
                        let value = value.map_or(Value::Unit, |r| self.load(index, r));
                        let frame = self.slots[index].frames.pop().expect("frame");
                        if self.slots[index].frames.is_empty() {
                            let handle = TaskHandle {
                                slot: index as u32,
                                generation: self.slots[index].generation,
                            };
                            self.cancel_inner(handle, true);
                        } else {
                            self.store(index, frame.return_dst, value);
                        }
                    }
                }
                Ok(())
            })();
            if let Err((kind, message)) = result {
                let mut d = self.program.diagnostic(kind, function, pc, message);
                d.task = Some(TaskHandle {
                    slot: index as u32,
                    generation: self.slots[index].generation,
                });
                return Err(d);
            }
            if self.slots[index].done
                || self.slots[index].wake > world.tick()
                || self.slots[index].joining.is_some_and(|h| self.live(h))
            {
                return Ok(());
            }
        }
    }
    fn arguments(&self, index: usize, args: [u8; 8]) -> [Value; 8] {
        args.map(|register| self.load(index, register))
    }
    fn load(&self, index: usize, register: u8) -> Value {
        self.slots[index].frames.last().expect("frame").registers[register as usize]
    }
    fn store(&mut self, index: usize, register: u8, value: Value) {
        self.slots[index]
            .frames
            .last_mut()
            .expect("frame")
            .registers[register as usize] = value;
    }
    fn builtin(
        &mut self,
        index: usize,
        builtin: Builtin,
        args: [Value; 8],
        world: &mut Simulation,
    ) -> Result<(Value, Control), (DiagnosticKind, &'static str)> {
        let bad = (DiagnosticKind::Argument, "invalid builtin argument");
        let entity_error = (DiagnosticKind::Entity, "entity handle is stale or invalid");
        let task_error = (DiagnosticKind::Task, "invalid task handle/join dependency");
        let int = |i: usize| {
            if let Value::Int(v) = args[i] {
                Ok(v)
            } else {
                Err(bad)
            }
        };
        let fixed = |i: usize| {
            if let Value::Fixed(v) = args[i] {
                Ok(v)
            } else {
                Err(bad)
            }
        };
        let vector = |i: usize| {
            if let Value::Vec(v) = args[i] {
                Ok(v)
            } else {
                Err(bad)
            }
        };
        let entity = |i: usize| {
            if let Value::Entity(Some(v)) = args[i] {
                Ok(v)
            } else {
                Err(entity_error)
            }
        };
        let task = |i: usize| {
            if let Value::Task(Some(v)) = args[i] {
                Ok(v)
            } else {
                Err(task_error)
            }
        };
        let host = |_: SimulationError| {
            (
                DiagnosticKind::Host,
                "simulation rejected script command (capacity, bounds or entity)",
            )
        };
        let overflow = (DiagnosticKind::Overflow, "checked arithmetic overflow");
        let value = match builtin {
            Builtin::Vector => Value::Vec(Vec2::new(fixed(0)?, fixed(1)?)),
            Builtin::Fixed => Value::Fixed(Fixed::from_int(int(0)?).ok_or(overflow)?),
            Builtin::Int => Value::Int(fixed(0)?.bits() / 65536),
            Builtin::X => Value::Fixed(vector(0)?.x),
            Builtin::Y => Value::Fixed(vector(0)?.y),
            Builtin::Width => Value::Fixed(world.config().width),
            Builtin::Height => Value::Fixed(world.config().height),
            Builtin::Player => Value::Vec(world.player().position),
            Builtin::Tick => Value::Int(i32::try_from(world.tick()).map_err(|_| overflow)?),
            Builtin::Random => {
                self.rng = self.rng.wrapping_add(0x9e3779b97f4a7c15);
                let mut z = self.rng;
                z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
                z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
                Value::Int(((z ^ (z >> 31)) >> 33) as i32)
            }
            Builtin::Aim => {
                let from = vector(0)?;
                let to = vector(1)?;
                let speed = fixed(2)?.bits();
                if speed < 0 {
                    return Err(bad);
                }
                let dx = i64::from(to.x.bits()) - i64::from(from.x.bits());
                let dy = i64::from(to.y.bits()) - i64::from(from.y.bits());
                let scale = dx.abs().max(dy.abs()).max(1);
                Value::Vec(Vec2::new(
                    Fixed::from_bits((dx * i64::from(speed) / scale) as i32),
                    Fixed::from_bits((dy * i64::from(speed) / scale) as i32),
                ))
            }
            Builtin::Enemy => {
                let hp = int(3)?;
                let lifetime = int(4)?;
                if hp <= 0 || lifetime < 0 {
                    return Err(bad);
                }
                Value::Entity(Some(
                    world
                        .spawn_enemy(Enemy {
                            position: vector(0)?,
                            velocity: vector(1)?,
                            radius: fixed(2)?,
                            health: hp as u32,
                            contact_damage: 1,
                            lifetime: lifetime as u32,
                            bounds: if lifetime == 0 {
                                BoundsBehavior::Keep
                            } else {
                                BoundsBehavior::Despawn
                            },
                            rgba: int(5)? as u32,
                        })
                        .map_err(host)?,
                ))
            }
            Builtin::Emit => {
                let damage = int(3)?;
                let lifetime = int(4)?;
                if damage < 0 || lifetime < 0 {
                    return Err(bad);
                }
                let collider = Collider::circle(fixed(2)?).map_err(|_| bad)?;
                Value::Entity(Some(
                    world
                        .spawn_projectile(Projectile {
                            position: vector(0)?,
                            velocity: vector(1)?,
                            collider,
                            faction: Faction::Enemy,
                            damage: damage as u32,
                            lifetime: lifetime as u32,
                            bounds: BoundsBehavior::Despawn,
                            rgba: 0xf58ac7ff,
                        })
                        .map_err(host)?,
                ))
            }
            Builtin::Alive => Value::Bool(if let Value::Entity(Some(handle)) = args[0] {
                entity_alive(world, handle)
            } else {
                false
            }),
            Builtin::Position => {
                let h = entity(0)?;
                let position = match h.kind() {
                    EntityKind::Player if world.player().health > 0 => {
                        Some(world.player().position)
                    }
                    EntityKind::Enemy => world.enemy(h).map(|e| e.position),
                    EntityKind::Projectile => world.projectile(h).map(|p| p.position),
                    EntityKind::Drop => world.drop_snapshot(h).map(|p| p.position),
                    _ => None,
                };
                Value::Vec(position.ok_or(entity_error)?)
            }
            Builtin::Move => {
                world.set_velocity(entity(0)?, vector(1)?).map_err(host)?;
                Value::Unit
            }
            Builtin::ClearEnemies => {
                world.clear_enemies();
                Value::Unit
            }
            Builtin::ClearShots => {
                world.clear_hostile_projectiles();
                Value::Unit
            }
            Builtin::Wave => {
                let wave = int(0)?;
                if wave < 0 {
                    return Err(bad);
                }
                self.status.wave = wave as u32;
                Value::Unit
            }
            Builtin::Boss => {
                let h = entity(0)?;
                let hp = int(1)?;
                if hp <= 0 || h.kind() != EntityKind::Enemy || world.enemy(h).is_none() {
                    return Err(entity_error);
                }
                self.status.boss = Some(h);
                self.status.boss_max_health = hp as u32;
                Value::Unit
            }
            Builtin::Complete => {
                self.status.complete = true;
                Value::Unit
            }
            Builtin::Wait => {
                let ticks = int(0)?;
                if ticks <= 0 {
                    return Err(bad);
                }
                let wake = world.tick().checked_add(ticks as u64).ok_or(overflow)?;
                return Ok((Value::Unit, Control::Wait(wake)));
            }
            Builtin::Join => {
                let handle = task(0)?;
                let current = TaskHandle {
                    slot: index as u32,
                    generation: self.slots[index].generation,
                };
                if current == handle || self.descendant(current, handle) {
                    return Err(task_error);
                }
                if self.live(handle) {
                    return Ok((Value::Unit, Control::Join(handle)));
                }
                Value::Unit
            }
            Builtin::Cancel => {
                self.cancel_inner(task(0)?, true);
                Value::Unit
            }
            Builtin::CancelChildren => {
                let current = TaskHandle {
                    slot: index as u32,
                    generation: self.slots[index].generation,
                };
                self.cancel_inner(current, false);
                Value::Unit
            }
            Builtin::Attach => {
                let owner = entity(0)?;
                if !entity_alive(world, owner) {
                    return Err(entity_error);
                }
                self.slots[index].owner = Some(owner);
                Value::Unit
            }
            Builtin::CurrentTask => Value::Task(Some(TaskHandle {
                slot: index as u32,
                generation: self.slots[index].generation,
            })),
            Builtin::Polar => {
                Value::Vec(crate::advanced::polar(fixed(0)?, fixed(1)?).map_err(host)?)
            }
            Builtin::Ring | Builtin::Fan | Builtin::Aimed | Builtin::Spiral => {
                use crate::advanced::{Pattern, PatternShot};
                let (count, speed, radius, lifetime, pattern) = match builtin {
                    Builtin::Ring => (
                        int(1)?,
                        fixed(2)?,
                        fixed(4)?,
                        int(5)?,
                        Pattern::Ring { angle: fixed(3)? },
                    ),
                    Builtin::Fan => (
                        int(1)?,
                        fixed(2)?,
                        fixed(5)?,
                        int(6)?,
                        Pattern::Fan {
                            angle: fixed(3)?,
                            spread: fixed(4)?,
                        },
                    ),
                    Builtin::Aimed => (
                        int(2)?,
                        fixed(3)?,
                        fixed(5)?,
                        int(6)?,
                        Pattern::Aimed {
                            target: vector(1)?,
                            spread: fixed(4)?,
                        },
                    ),
                    _ => (
                        int(1)?,
                        fixed(2)?,
                        fixed(5)?,
                        int(6)?,
                        Pattern::Spiral {
                            angle: fixed(3)?,
                            step: fixed(4)?,
                        },
                    ),
                };
                if count <= 0 || lifetime < 0 {
                    return Err(bad);
                }
                world
                    .emit_pattern(
                        pattern,
                        PatternShot {
                            origin: vector(0)?,
                            count: count as u32,
                            speed,
                            radius,
                            lifetime: lifetime as u32,
                            rgba: match builtin {
                                Builtin::Ring => 0xf5a2d9ff,
                                Builtin::Fan => 0xffbd76ff,
                                Builtin::Aimed => 0xff8297ff,
                                _ => 0xb0a4ffff,
                            },
                        },
                    )
                    .map_err(host)?;
                Value::Unit
            }
            Builtin::Compose => {
                world
                    .compose_motion(
                        entity(0)?,
                        crate::advanced::Motion {
                            acceleration: vector(1)?,
                            turn_per_tick: fixed(2)?,
                        },
                    )
                    .map_err(host)?;
                Value::Unit
            }
            Builtin::Laser | Builtin::CurveLaser => {
                use crate::advanced::{LaserTiming, bezier_collider};
                let shift = usize::from(builtin == Builtin::CurveLaser);
                let warmup = int(3 + shift)?;
                let active = int(4 + shift)?;
                let fade = int(5 + shift)?;
                if warmup < 0 || active <= 0 || fade < 0 {
                    return Err(bad);
                }
                let collider = if shift == 0 {
                    Collider::capsule(Vec2::ZERO, vector(1)?, fixed(2)?).map_err(|_| bad)?
                } else {
                    bezier_collider(vector(1)?, vector(2)?, fixed(3)?).map_err(host)?
                };
                Value::Entity(Some(
                    world
                        .spawn_laser(
                            vector(0)?,
                            Vec2::ZERO,
                            collider,
                            LaserTiming {
                                warmup: warmup as u32,
                                active: active as u32,
                                fade: fade as u32,
                            },
                            if shift == 0 { 0x65dfffff } else { 0xb4a0ffff },
                        )
                        .map_err(host)?,
                ))
            }
            Builtin::Drop | Builtin::EnemyDrop => {
                use crate::advanced::{DropKind, DropReward};
                let kind = DropKind::from_u32(int(1)? as u32).ok_or(bad)?;
                let value = int(2)?;
                if value <= 0 {
                    return Err(bad);
                }
                let reward = DropReward {
                    kind,
                    value: value as u32,
                };
                if builtin == Builtin::Drop {
                    Value::Entity(Some(world.drop_item(vector(0)?, reward).map_err(host)?))
                } else {
                    world.enemy_drop(entity(0)?, reward).map_err(host)?;
                    Value::Unit
                }
            }
            Builtin::Difficulty => Value::Int(world.difficulty().ok_or(bad)? as i32),
            Builtin::Phase => {
                let phase = int(1)?;
                let duration = int(2)?;
                if phase <= 0 || duration <= 0 {
                    return Err(bad);
                }
                world
                    .begin_boss_phase(entity(0)?, phase as u32, duration as u32)
                    .map_err(host)?;
                Value::Unit
            }
            Builtin::Despawn => {
                world.despawn(entity(0)?).map_err(host)?;
                Value::Unit
            }
            Builtin::CancelShots => {
                let Value::Bool(reward) = args[0] else {
                    return Err(bad);
                };
                Value::Int(world.cancel_shots(reward).map_err(host)? as i32)
            }
            Builtin::Colour => {
                world.colour(entity(0)?, int(1)? as u32).map_err(host)?;
                Value::Unit
            }
        };
        Ok((value, Control::Continue))
    }
    pub fn state_hash(&self) -> u64 {
        let mut hash = Fingerprint::new();
        hash.u32(if self.program.uses_advanced() {
            VM_PROTOCOL_VERSION
        } else {
            1
        });
        hash.u64(self.program.content_hash());
        for v in [
            self.limits.tasks,
            self.limits.call_depth,
            self.limits.instructions_per_tick,
            self.limits.instructions_per_task,
            self.limits.commands_per_tick,
            self.limits.births_per_tick,
        ] {
            hash.u32(v);
        }
        hash.u64(self.rng);
        hash.u32(u32::from(self.last_tick.is_some()));
        if let Some(tick) = self.last_tick {
            hash.u64(tick);
        }
        hash.u32(self.status.wave);
        hash.u32(self.status.boss_max_health);
        hash.u32(u32::from(self.status.complete));
        hash_value(&mut hash, Value::Entity(self.status.boss));
        for slot in &self.slots {
            hash.u32(slot.generation);
            hash.u32(u32::from(slot.active));
            if slot.active {
                hash_value(&mut hash, Value::Task(slot.parent));
                hash_value(&mut hash, Value::Entity(slot.owner));
                hash.u64(slot.wake);
                hash_value(&mut hash, Value::Task(slot.joining));
                hash.u32(slot.frames.len() as u32);
                for frame in &slot.frames {
                    hash.u32(u32::from(frame.function));
                    hash.u32(frame.pc);
                    hash.u32(u32::from(frame.return_dst));
                    for &value in &frame.registers[..self.program.functions
                        [frame.function as usize]
                        .registers
                        .len()]
                    {
                        hash_value(&mut hash, value);
                    }
                }
            }
        }
        hash.u32(self.order.len() as u32);
        for &index in &self.order {
            hash.u32(index);
        }
        hash.u32(self.free.len() as u32);
        for &index in &self.free {
            hash.u32(index);
        }
        hash.u32(u32::from(self.fault.is_some()));
        if let Some(fault) = &self.fault {
            hash.u32(fault.kind as u32);
            hash.u32(fault.span.start);
            hash.u32(fault.span.end);
            hash_value(&mut hash, Value::Task(fault.task));
            hash.bytes(fault.message.as_bytes());
        }
        hash.finish()
    }
    pub(crate) fn validate_restored(&self) -> Result<(), Diagnostic> {
        let error = || Diagnostic::data(DiagnosticKind::Snapshot, "invalid VM task/frame state");
        let mut listed = vec![false; self.slots.len()];
        for &index in &self.order {
            let slot = self.slots.get(index as usize).ok_or_else(error)?;
            if !slot.active || listed[index as usize] {
                return Err(error());
            }
            listed[index as usize] = true;
        }
        let mut free = vec![false; self.slots.len()];
        for &index in &self.free {
            let slot = self.slots.get(index as usize).ok_or_else(error)?;
            if slot.active || free[index as usize] {
                return Err(error());
            }
            free[index as usize] = true;
        }
        for (index, slot) in self.slots.iter().enumerate() {
            if slot.generation == 0
                || slot.done
                || slot.active != listed[index]
                || !slot.active && !free[index] && slot.generation != u32::MAX
            {
                return Err(error());
            }
            if !slot.active {
                if !slot.frames.is_empty() {
                    return Err(error());
                }
                continue;
            }
            if slot.frames.is_empty() || slot.frames.len() > self.limits.call_depth as usize {
                return Err(error());
            }
            let current = TaskHandle {
                slot: index as u32,
                generation: slot.generation,
            };
            if let Some(parent) = slot.parent
                && (!self.live(parent) || self.descendant(parent, current))
            {
                return Err(error());
            }
            if let Some(joining) = slot.joining
                && (joining.slot as usize >= self.slots.len()
                    || joining.generation == 0
                    || joining == current
                    || self.descendant(current, joining))
            {
                return Err(error());
            }
            for (frame_index, frame) in slot.frames.iter().enumerate() {
                let function = self
                    .program
                    .functions
                    .get(frame.function as usize)
                    .ok_or_else(error)?;
                if frame.pc as usize >= function.code.len()
                    || frame_index == 0 && (!function.task || frame.return_dst != u8::MAX)
                    || frame_index > 0 && function.task
                {
                    return Err(error());
                }
                for (register, &value) in frame.registers.iter().enumerate() {
                    if value.ty()
                        != function
                            .registers
                            .get(register)
                            .copied()
                            .unwrap_or(Type::Unit)
                    {
                        return Err(error());
                    }
                    if let Value::Task(Some(task)) = value
                        && task.slot as usize >= self.slots.len()
                    {
                        return Err(error());
                    }
                }
                if frame_index > 0 {
                    let caller = &slot.frames[frame_index - 1];
                    let caller_function = &self.program.functions[caller.function as usize];
                    if caller_function.registers.get(frame.return_dst as usize)
                        != Some(&function.result)
                    {
                        return Err(error());
                    }
                    let Some(instruction) = caller
                        .pc
                        .checked_sub(1)
                        .and_then(|pc| caller_function.code.get(pc as usize))
                    else {
                        return Err(error());
                    };
                    if !matches!(instruction.op,Op::Call{function:f,dst,..} if f==frame.function&&dst==frame.return_dst)
                    {
                        return Err(error());
                    }
                }
            }
        }
        Ok(())
    }
}
fn entity_alive(world: &Simulation, h: EntityHandle) -> bool {
    match h.kind() {
        EntityKind::Player => world.player().health > 0,
        EntityKind::Enemy => world.enemy(h).is_some_and(|e| e.health > 0),
        EntityKind::Projectile => world.projectile(h).is_some(),
        EntityKind::Drop => world.drop_snapshot(h).is_some(),
    }
}
pub(crate) fn hash_value(hash: &mut Fingerprint, value: Value) {
    hash.u32(value.ty() as u32);
    match value {
        Value::Unit => {}
        Value::Int(v) => hash.u32(v as u32),
        Value::Bool(v) => hash.u32(u32::from(v)),
        Value::Fixed(v) => hash.u32(v.bits() as u32),
        Value::Vec(v) => {
            hash.u32(v.x.bits() as u32);
            hash.u32(v.y.bits() as u32);
        }
        Value::Entity(v) => {
            hash.u32(u32::from(v.is_some()));
            if let Some(v) = v {
                hash.u32(v.kind() as u32);
                hash.u32(v.slot());
                hash.u32(v.generation());
            }
        }
        Value::Task(v) => {
            hash.u32(u32::from(v.is_some()));
            if let Some(v) = v {
                hash.u32(v.slot);
                hash.u32(v.generation);
            }
        }
    }
}
fn unary(op: Unary, value: Value) -> Result<Value, (DiagnosticKind, &'static str)> {
    let overflow = (DiagnosticKind::Overflow, "checked negation overflow");
    Ok(match (op, value) {
        (Unary::Not, Value::Bool(v)) => Value::Bool(!v),
        (Unary::Neg, Value::Int(v)) => Value::Int(v.checked_neg().ok_or(overflow)?),
        (Unary::Neg, Value::Fixed(v)) => {
            Value::Fixed(Fixed::from_bits(v.bits().checked_neg().ok_or(overflow)?))
        }
        (Unary::Neg, Value::Vec(v)) => Value::Vec(Vec2::new(
            Fixed::from_bits(v.x.bits().checked_neg().ok_or(overflow)?),
            Fixed::from_bits(v.y.bits().checked_neg().ok_or(overflow)?),
        )),
        _ => return Err((DiagnosticKind::Bytecode, "invalid unary type")),
    })
}
fn binary(op: Binary, left: Value, right: Value) -> Result<Value, (DiagnosticKind, &'static str)> {
    use Binary::*;
    let overflow = (DiagnosticKind::Overflow, "checked arithmetic overflow");
    let zero = (DiagnosticKind::DivisionByZero, "division by zero");
    if op == Eq {
        return Ok(Value::Bool(left == right));
    }
    if op == Ne {
        return Ok(Value::Bool(left != right));
    }
    Ok(match (left, right) {
        (Value::Int(a), Value::Int(b)) => match op {
            Add => Value::Int(a.checked_add(b).ok_or(overflow)?),
            Sub => Value::Int(a.checked_sub(b).ok_or(overflow)?),
            Mul => Value::Int(a.checked_mul(b).ok_or(overflow)?),
            Div | Rem => {
                if b == 0 {
                    return Err(zero);
                }
                Value::Int(
                    if op == Div {
                        a.checked_div(b)
                    } else {
                        a.checked_rem(b)
                    }
                    .ok_or(overflow)?,
                )
            }
            Lt => Value::Bool(a < b),
            Le => Value::Bool(a <= b),
            Gt => Value::Bool(a > b),
            Ge => Value::Bool(a >= b),
            _ => return Err((DiagnosticKind::Bytecode, "invalid integer operator")),
        },
        (Value::Fixed(a), Value::Fixed(b)) => match op {
            Add => Value::Fixed(a.checked_add(b).ok_or(overflow)?),
            Sub => Value::Fixed(a.checked_sub(b).ok_or(overflow)?),
            Mul => Value::Fixed(a.checked_mul(b).ok_or(overflow)?),
            Div => {
                if b == Fixed::ZERO {
                    return Err(zero);
                }
                Value::Fixed(a.checked_div(b).ok_or(overflow)?)
            }
            Lt => Value::Bool(a < b),
            Le => Value::Bool(a <= b),
            Gt => Value::Bool(a > b),
            Ge => Value::Bool(a >= b),
            _ => return Err((DiagnosticKind::Bytecode, "invalid fixed operator")),
        },
        (Value::Vec(a), Value::Vec(b)) => Value::Vec(match op {
            Add => a.checked_add(b).ok_or(overflow)?,
            Sub => Vec2::new(
                a.x.checked_sub(b.x).ok_or(overflow)?,
                a.y.checked_sub(b.y).ok_or(overflow)?,
            ),
            _ => return Err((DiagnosticKind::Bytecode, "invalid vector operator")),
        }),
        (Value::Vec(a), Value::Fixed(b)) => Value::Vec(match op {
            Mul => Vec2::new(
                a.x.checked_mul(b).ok_or(overflow)?,
                a.y.checked_mul(b).ok_or(overflow)?,
            ),
            Div => {
                if b == Fixed::ZERO {
                    return Err(zero);
                }
                Vec2::new(
                    a.x.checked_div(b).ok_or(overflow)?,
                    a.y.checked_div(b).ok_or(overflow)?,
                )
            }
            _ => return Err((DiagnosticKind::Bytecode, "invalid vector/scalar operator")),
        }),
        (Value::Fixed(a), Value::Vec(b)) if op == Mul => Value::Vec(Vec2::new(
            a.checked_mul(b.x).ok_or(overflow)?,
            a.checked_mul(b.y).ok_or(overflow)?,
        )),
        _ => return Err((DiagnosticKind::Bytecode, "invalid binary types")),
    })
}
