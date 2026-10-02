use super::*;
pub(crate) struct Writer(pub Vec<u8>);
impl Writer {
    pub fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn text(&mut self, v: &str) {
        self.u32(v.len() as u32);
        self.0.extend_from_slice(v.as_bytes());
    }
    pub fn value(&mut self, v: Value) {
        self.u8(v.ty() as u8);
        match v {
            Value::Unit => {}
            Value::Int(v) => self.u32(v as u32),
            Value::Bool(v) => self.u8(u8::from(v)),
            Value::Fixed(v) => self.u32(v.bits() as u32),
            Value::Vec(v) => {
                self.u32(v.x.bits() as u32);
                self.u32(v.y.bits() as u32);
            }
            Value::Entity(v) => {
                self.u8(u8::from(v.is_some()));
                if let Some(v) = v {
                    self.u32(v.kind() as u32);
                    self.u32(v.slot());
                    self.u32(v.generation());
                }
            }
            Value::Task(v) => {
                self.u8(u8::from(v.is_some()));
                if let Some(v) = v {
                    self.u32(v.slot);
                    self.u32(v.generation);
                }
            }
        }
    }
}
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
    kind: DiagnosticKind,
}
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8], kind: DiagnosticKind) -> Self {
        Self {
            bytes,
            offset: 0,
            kind,
        }
    }
    pub fn error(&self, message: &str) -> Diagnostic {
        Diagnostic::data(self.kind, message)
    }
    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }
    pub fn take(&mut self, count: usize) -> Result<&'a [u8], Diagnostic> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| self.error("binary length overflow"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| self.error("truncated binary"))?;
        self.offset = end;
        Ok(bytes)
    }
    pub fn u8(&mut self) -> Result<u8, Diagnostic> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16, Diagnostic> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("two bytes"),
        ))
    }
    pub fn u32(&mut self) -> Result<u32, Diagnostic> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four bytes"),
        ))
    }
    pub fn u64(&mut self) -> Result<u64, Diagnostic> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("eight bytes"),
        ))
    }
    pub fn boolean(&mut self) -> Result<bool, Diagnostic> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(self.error("invalid boolean")),
        }
    }
    pub fn ty(&mut self) -> Result<Type, Diagnostic> {
        Type::decode(self.u8()?).ok_or_else(|| self.error("invalid type tag"))
    }
    pub fn text(&mut self, limit: usize) -> Result<String, Diagnostic> {
        let len = self.u32()? as usize;
        if len > limit {
            return Err(self.error("string limit exceeded"));
        }
        std::str::from_utf8(self.take(len)?)
            .map(str::to_owned)
            .map_err(|_| self.error("invalid UTF-8"))
    }
    pub fn value(&mut self) -> Result<Value, Diagnostic> {
        Ok(match self.ty()? {
            Type::Unit => Value::Unit,
            Type::Int => Value::Int(self.u32()? as i32),
            Type::Bool => Value::Bool(self.boolean()?),
            Type::Fixed => Value::Fixed(Fixed::from_bits(self.u32()? as i32)),
            Type::Vec => Value::Vec(Vec2::new(
                Fixed::from_bits(self.u32()? as i32),
                Fixed::from_bits(self.u32()? as i32),
            )),
            Type::Entity => {
                if self.boolean()? {
                    let kind = self.u32()?;
                    let slot = self.u32()?;
                    let generation = self.u32()?;
                    Value::Entity(Some(
                        EntityHandle::from_parts(kind, slot, generation)
                            .ok_or_else(|| self.error("invalid entity handle"))?,
                    ))
                } else {
                    Value::Entity(None)
                }
            }
            Type::Task => {
                if self.boolean()? {
                    let slot = self.u32()?;
                    let generation = self.u32()?;
                    if slot >= 256 || generation == 0 {
                        return Err(self.error("invalid task handle"));
                    }
                    Value::Task(Some(TaskHandle { slot, generation }))
                } else {
                    Value::Task(None)
                }
            }
        })
    }
}
fn write_instruction(out: &mut Writer, instruction: Instruction) {
    out.u32(instruction.span.start);
    out.u32(instruction.span.end);
    match instruction.op {
        Op::Const { dst, value } => {
            out.u8(0);
            out.u8(dst);
            out.value(value);
        }
        Op::Copy { dst, src } => {
            out.u8(1);
            out.u8(dst);
            out.u8(src);
        }
        Op::Unary { dst, src, op } => {
            out.u8(2);
            out.u8(dst);
            out.u8(src);
            out.u8(match op {
                Unary::Neg => 0,
                Unary::Not => 1,
            });
        }
        Op::Binary {
            dst,
            left,
            right,
            op,
        } => {
            out.u8(3);
            out.u8(dst);
            out.u8(left);
            out.u8(right);
            out.u8(op as u8);
        }
        Op::Jump { target } => {
            out.u8(4);
            out.u32(target);
        }
        Op::Branch {
            condition,
            when,
            target,
        } => {
            out.u8(5);
            out.u8(condition);
            out.u8(u8::from(when));
            out.u32(target);
        }
        Op::Call {
            dst,
            function,
            args,
            len,
        }
        | Op::Fork {
            dst,
            function,
            args,
            len,
        } => {
            out.u8(if matches!(instruction.op, Op::Call { .. }) {
                6
            } else {
                7
            });
            out.u8(dst);
            out.u16(function);
            out.u8(len);
            out.0.extend_from_slice(&args[..len as usize]);
        }
        Op::Builtin {
            dst,
            builtin,
            args,
            len,
        } => {
            out.u8(8);
            out.u8(dst);
            out.u8(builtin as u8);
            out.u8(len);
            out.0.extend_from_slice(&args[..len as usize]);
        }
        Op::Return { value } => {
            out.u8(9);
            out.u8(value.unwrap_or(u8::MAX));
        }
    }
}
fn read_instruction(input: &mut Reader<'_>) -> Result<Instruction, Diagnostic> {
    let span = Span {
        start: input.u32()?,
        end: input.u32()?,
    };
    let tag = input.u8()?;
    let op = match tag {
        0 => Op::Const {
            dst: input.u8()?,
            value: input.value()?,
        },
        1 => Op::Copy {
            dst: input.u8()?,
            src: input.u8()?,
        },
        2 => {
            let dst = input.u8()?;
            let src = input.u8()?;
            let op = match input.u8()? {
                0 => Unary::Neg,
                1 => Unary::Not,
                _ => return Err(input.error("invalid unary operator")),
            };
            Op::Unary { dst, src, op }
        }
        3 => {
            let dst = input.u8()?;
            let left = input.u8()?;
            let right = input.u8()?;
            let op = Binary::decode(input.u8()?)
                .ok_or_else(|| input.error("invalid binary operator"))?;
            Op::Binary {
                dst,
                left,
                right,
                op,
            }
        }
        4 => Op::Jump {
            target: input.u32()?,
        },
        5 => Op::Branch {
            condition: input.u8()?,
            when: input.boolean()?,
            target: input.u32()?,
        },
        6..=8 => {
            let dst = input.u8()?;
            let function = if tag < 8 { input.u16()? } else { 0 };
            let builtin = if tag == 8 {
                Builtin::decode(input.u8()?).ok_or_else(|| input.error("unknown builtin"))?
            } else {
                Builtin::Vector
            };
            let len = input.u8()?;
            if len as usize > 8 {
                return Err(input.error("argument count exceeds eight"));
            }
            let mut args = [0; 8];
            args[..len as usize].copy_from_slice(input.take(len as usize)?);
            match tag {
                6 => Op::Call {
                    dst,
                    function,
                    args,
                    len,
                },
                7 => Op::Fork {
                    dst,
                    function,
                    args,
                    len,
                },
                _ => Op::Builtin {
                    dst,
                    builtin,
                    args,
                    len,
                },
            }
        }
        9 => {
            let register = input.u8()?;
            Op::Return {
                value: if register == u8::MAX {
                    None
                } else {
                    Some(register)
                },
            }
        }
        _ => return Err(input.error("unknown opcode")),
    };
    Ok(Instruction { op, span })
}
pub(crate) fn program_body(program: &Program) -> Vec<u8> {
    let mut out = Writer(Vec::new());
    out.u32(program.bytecode_version());
    out.text(&program.source);
    out.u16(program.entry);
    out.u16(program.functions.len() as u16);
    for function in &program.functions {
        out.text(&function.name);
        out.u8(u8::from(function.task));
        out.u8(function.result as u8);
        out.u8(function.params.len() as u8);
        for &ty in &function.params {
            out.u8(ty as u8);
        }
        out.u8(function.registers.len() as u8);
        for &ty in &function.registers {
            out.u8(ty as u8);
        }
        out.u32(function.code.len() as u32);
        for &instruction in &function.code {
            write_instruction(&mut out, instruction);
        }
    }
    out.0
}
pub(crate) fn write_program(program: &Program) -> Vec<u8> {
    let mut out = Writer(b"GZCODE01".to_vec());
    out.text(&program.file);
    out.0.extend_from_slice(&program_body(program));
    out.u64(program.content_hash());
    out.0
}
pub(crate) fn read_program(bytes: &[u8]) -> Result<Program, Diagnostic> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(Diagnostic::data(
            DiagnosticKind::Bytecode,
            "bytecode exceeds 16 MiB",
        ));
    }
    let mut input = Reader::new(bytes, DiagnosticKind::Bytecode);
    if input.take(8)? != b"GZCODE01" {
        return Err(input.error("invalid bytecode magic"));
    }
    let file = input.text(4096)?;
    let version = input.u32()?;
    if version != 1 && version != BYTECODE_VERSION {
        return Err(input.error("unsupported bytecode version"));
    }
    let source = input.text(MAX_SOURCE_BYTES)?;
    let entry = input.u16()?;
    let count = input.u16()? as usize;
    if count == 0 || count > MAX_FUNCTIONS {
        return Err(input.error("function count exceeds limit"));
    }
    let mut functions = Vec::with_capacity(count);
    let mut total = 0;
    for _ in 0..count {
        let name = input.text(256)?;
        let task = input.boolean()?;
        let result = input.ty()?;
        let count = input.u8()? as usize;
        if count > 8 {
            return Err(input.error("parameter limit"));
        }
        let mut params = Vec::new();
        for _ in 0..count {
            params.push(input.ty()?);
        }
        let count = input.u8()? as usize;
        if count > 64 {
            return Err(input.error("register limit"));
        }
        let mut registers = Vec::new();
        for _ in 0..count {
            registers.push(input.ty()?);
        }
        let count = input.u32()? as usize;
        total += count;
        if total > MAX_INSTRUCTIONS || count > input.remaining() / 10 {
            return Err(input.error("instruction limit/truncation"));
        }
        let mut code = Vec::with_capacity(count);
        for _ in 0..count {
            code.push(read_instruction(&mut input)?);
        }
        functions.push(Function {
            name,
            params,
            result,
            task,
            registers,
            code,
        });
    }
    let expected = input.u64()?;
    if input.remaining() != 0 {
        return Err(input.error("trailing bytecode bytes"));
    }
    let program = Program::checked(file, source, entry, functions)?;
    if version != program.bytecode_version() {
        return Err(input.error("bytecode version does not match its builtin set"));
    }
    if program.content_hash() != expected {
        return Err(input.error("bytecode fingerprint mismatch"));
    }
    Ok(program)
}
pub(crate) fn write_vm(vm: &Vm) -> Vec<u8> {
    let mut out = Writer(b"GZVMST01".to_vec());
    out.u32(VM_STATE_VERSION);
    out.u32(if vm.program.uses_advanced() {
        VM_PROTOCOL_VERSION
    } else {
        1
    });
    out.u64(vm.program.content_hash());
    for v in [
        vm.limits.tasks,
        vm.limits.call_depth,
        vm.limits.instructions_per_tick,
        vm.limits.instructions_per_task,
        vm.limits.commands_per_tick,
        vm.limits.births_per_tick,
    ] {
        out.u32(v);
    }
    out.u64(vm.rng);
    out.u8(u8::from(vm.last_tick.is_some()));
    if let Some(tick) = vm.last_tick {
        out.u64(tick);
    }
    out.u32(vm.last_instructions);
    out.u32(vm.status.wave);
    out.value(Value::Entity(vm.status.boss));
    out.u32(vm.status.boss_max_health);
    out.u8(u8::from(vm.status.complete));
    for slot in &vm.slots {
        out.u32(slot.generation);
        out.u8(u8::from(slot.active));
        if slot.active {
            out.value(Value::Task(slot.parent));
            out.value(Value::Entity(slot.owner));
            out.u64(slot.wake);
            out.value(Value::Task(slot.joining));
            out.u8(slot.frames.len() as u8);
            for frame in &slot.frames {
                out.u16(frame.function);
                out.u32(frame.pc);
                out.u8(frame.return_dst);
                for &value in &frame.registers {
                    out.value(value);
                }
            }
        }
    }
    out.u32(vm.order.len() as u32);
    for &index in &vm.order {
        out.u32(index);
    }
    out.u32(vm.free.len() as u32);
    for &index in &vm.free {
        out.u32(index);
    }
    out.u8(u8::from(vm.fault.is_some()));
    if let Some(fault) = &vm.fault {
        out.u32(fault.kind as u32);
        out.u32(fault.span.start);
        out.u32(fault.span.end);
        let function = vm
            .program
            .functions
            .iter()
            .position(|f| f.name == fault.function)
            .unwrap_or(vm.program.entry as usize);
        out.u16(function as u16);
        out.value(Value::Task(fault.task));
        out.text(&fault.message);
    }
    out.u64(vm.state_hash());
    out.0
}
pub(crate) fn read_vm(program: std::sync::Arc<Program>, bytes: &[u8]) -> Result<Vm, Diagnostic> {
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(Diagnostic::data(
            DiagnosticKind::Snapshot,
            "VM snapshot exceeds 32 MiB",
        ));
    }
    let mut input = Reader::new(bytes, DiagnosticKind::Snapshot);
    if input.take(8)? != b"GZVMST01"
        || input.u32()? != VM_STATE_VERSION
        || input.u32()?
            != if program.uses_advanced() {
                VM_PROTOCOL_VERSION
            } else {
                1
            }
    {
        return Err(input.error("unsupported VM snapshot"));
    }
    if input.u64()? != program.content_hash() {
        return Err(input.error("VM snapshot belongs to a different program"));
    }
    let limits = VmLimits {
        tasks: input.u32()?,
        call_depth: input.u32()?,
        instructions_per_tick: input.u32()?,
        instructions_per_task: input.u32()?,
        commands_per_tick: input.u32()?,
        births_per_tick: input.u32()?,
    };
    limits
        .validate()
        .map_err(|_| input.error("invalid snapshot limits"))?;
    let mut vm = Vm::new(program, limits, 0)?;
    vm.rng = input.u64()?;
    vm.last_tick = if input.boolean()? {
        Some(input.u64()?)
    } else {
        None
    };
    vm.last_instructions = input.u32()?;
    vm.status.wave = input.u32()?;
    vm.status.boss = match input.value()? {
        Value::Entity(entity) => entity,
        _ => return Err(input.error("invalid Boss state")),
    };
    vm.status.boss_max_health = input.u32()?;
    vm.status.complete = input.boolean()?;
    for slot in &mut vm.slots {
        slot.frames.clear();
        slot.generation = input.u32()?;
        slot.active = input.boolean()?;
        slot.done = false;
        slot.parent = None;
        slot.owner = None;
        slot.wake = 0;
        slot.joining = None;
        if slot.active {
            slot.parent = match input.value()? {
                Value::Task(task) => task,
                _ => return Err(input.error("invalid parent")),
            };
            slot.owner = match input.value()? {
                Value::Entity(owner) => owner,
                _ => return Err(input.error("invalid owner")),
            };
            slot.wake = input.u64()?;
            slot.joining = match input.value()? {
                Value::Task(task) => task,
                _ => return Err(input.error("invalid join state")),
            };
            let count = input.u8()? as usize;
            if count == 0 || count > limits.call_depth as usize {
                return Err(input.error("invalid saved call depth"));
            }
            for _ in 0..count {
                let function = input.u16()?;
                let pc = input.u32()?;
                let return_dst = input.u8()?;
                let mut registers = [Value::Unit; 64];
                for value in &mut registers {
                    *value = input.value()?;
                }
                slot.frames.push(super::vm::Frame {
                    function,
                    pc,
                    return_dst,
                    registers,
                });
            }
        }
    }
    vm.order.clear();
    let count = input.u32()? as usize;
    if count > limits.tasks as usize {
        return Err(input.error("invalid task order"));
    }
    for _ in 0..count {
        vm.order.push(input.u32()?);
    }
    vm.free.clear();
    let count = input.u32()? as usize;
    if count > limits.tasks as usize {
        return Err(input.error("invalid free list"));
    }
    for _ in 0..count {
        vm.free.push(input.u32()?);
    }
    if input.boolean()? {
        let kind = DiagnosticKind::decode(input.u32()?)
            .ok_or_else(|| input.error("invalid fault kind"))?;
        let span = Span {
            start: input.u32()?,
            end: input.u32()?,
        };
        let function = input.u16()? as usize;
        if function >= vm.program.functions.len()
            || span.start > span.end
            || span.end as usize > vm.program.source.len()
            || !vm.program.source.is_char_boundary(span.start as usize)
            || !vm.program.source.is_char_boundary(span.end as usize)
        {
            return Err(input.error("invalid fault location"));
        }
        let task = match input.value()? {
            Value::Task(task) => task,
            _ => return Err(input.error("invalid fault task")),
        };
        let message = input.text(4096)?;
        let mut diagnostic =
            Diagnostic::new(kind, &vm.program.file, &vm.program.source, span, message);
        diagnostic.function = vm.program.functions[function].name.clone();
        diagnostic.task = task;
        vm.fault = Some(diagnostic);
    } else {
        vm.fault = None;
    }
    let expected = input.u64()?;
    if input.remaining() != 0 {
        return Err(input.error("trailing snapshot bytes"));
    }
    vm.validate_restored()?;
    if vm.state_hash() != expected {
        return Err(input.error("VM snapshot fingerprint mismatch"));
    }
    Ok(vm)
}
