use super::*;
use crate::resources::Fingerprint;
use std::collections::VecDeque;

#[derive(Clone)]
pub struct Program {
    pub(crate) file: String,
    pub(crate) source: String,
    pub(crate) entry: u16,
    pub(crate) functions: Vec<Function>,
    hash: u64,
    advanced: bool,
}
impl Program {
    pub fn compile(file: &str, source: &str) -> Result<Self, Diagnostic> {
        compiler::compile(file, source)
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Diagnostic> {
        codec::read_program(bytes)
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        codec::write_program(self)
    }
    pub fn content_hash(&self) -> u64 {
        self.hash
    }
    pub fn source_name(&self) -> &str {
        &self.file
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn function_names(&self) -> impl ExactSizeIterator<Item = &str> {
        self.functions.iter().map(|f| f.name.as_str())
    }
    pub fn instruction_count(&self) -> usize {
        self.functions.iter().map(|f| f.code.len()).sum()
    }
    pub fn uses_advanced(&self) -> bool {
        self.advanced
    }
    pub fn bytecode_version(&self) -> u32 {
        if self.uses_advanced() {
            BYTECODE_VERSION
        } else {
            1
        }
    }
    pub(crate) fn diagnostic(
        &self,
        kind: DiagnosticKind,
        function: usize,
        pc: usize,
        message: impl Into<String>,
    ) -> Diagnostic {
        let span = self
            .functions
            .get(function)
            .and_then(|f| f.code.get(pc))
            .map_or(Span::default(), |i| i.span);
        let mut diagnostic = Diagnostic::new(kind, &self.file, &self.source, span, message);
        if let Some(f) = self.functions.get(function) {
            diagnostic.function = f.name.clone();
        }
        diagnostic
    }
    pub(crate) fn checked(
        file: String,
        source: String,
        entry: u16,
        functions: Vec<Function>,
    ) -> Result<Self, Diagnostic> {
        let advanced = functions
            .iter()
            .flat_map(|f| &f.code)
            .any(|i| matches!(i.op, Op::Builtin { builtin, .. } if builtin as u8 >= 27));
        let mut program = Self {
            file,
            source,
            entry,
            functions,
            hash: 0,
            advanced,
        };
        program.verify()?;
        let mut hash = Fingerprint::new();
        hash.bytes(&codec::program_body(&program));
        program.hash = hash.finish();
        Ok(program)
    }
    fn verify(&self) -> Result<(), Diagnostic> {
        if self.file.len() > 4096
            || self.source.len() > MAX_SOURCE_BYTES
            || self.functions.is_empty()
            || self.functions.len() > MAX_FUNCTIONS
            || self.instruction_count() > MAX_INSTRUCTIONS
        {
            return Err(Diagnostic::data(
                DiagnosticKind::Bytecode,
                "program limits exceeded",
            ));
        }
        let Some(entry) = self.functions.get(self.entry as usize) else {
            return Err(Diagnostic::data(
                DiagnosticKind::Bytecode,
                "invalid entry function",
            ));
        };
        if !entry.task || !entry.params.is_empty() {
            return Err(Diagnostic::data(
                DiagnosticKind::Bytecode,
                "entry must be a parameterless task",
            ));
        }
        for (index, function) in self.functions.iter().enumerate() {
            if function.name.is_empty()
                || function.name.len() > 256
                || !function.name.bytes().enumerate().all(|(i, b)| {
                    b == b'_' || b.is_ascii_alphabetic() || i > 0 && b.is_ascii_digit()
                })
                || self.functions[..index]
                    .iter()
                    .any(|f| f.name == function.name)
                || function.params.len() > MAX_ARGUMENTS
                || function.registers.len() > MAX_REGISTERS
                || function.registers.len() < function.params.len()
                || function.registers[..function.params.len()] != function.params
                || function.params.contains(&Type::Unit)
                || function.code.is_empty()
                || function.task && function.result != Type::Unit
            {
                return Err(self.diagnostic(
                    DiagnosticKind::Bytecode,
                    index,
                    0,
                    "invalid function metadata",
                ));
            }
            for (pc, instruction) in function.code.iter().enumerate() {
                let span = instruction.span;
                if span.start > span.end
                    || span.end as usize > self.source.len()
                    || !self.source.is_char_boundary(span.start as usize)
                    || !self.source.is_char_boundary(span.end as usize)
                {
                    return Err(Diagnostic::data(
                        DiagnosticKind::Bytecode,
                        "invalid source mapping",
                    ));
                }
                self.check_instruction(index, pc)?;
            }
            self.check_flow(index)?;
        }
        let mut marks = vec![0u8; self.functions.len()];
        for index in 0..self.functions.len() {
            self.visit_calls(index, &mut marks)?;
        }
        Ok(())
    }
    fn visit_calls(&self, index: usize, marks: &mut [u8]) -> Result<(), Diagnostic> {
        if marks[index] == 2 {
            return Ok(());
        }
        if marks[index] == 1 {
            return Err(self.diagnostic(
                DiagnosticKind::Bytecode,
                index,
                0,
                "recursive functions are not supported",
            ));
        }
        marks[index] = 1;
        for instruction in &self.functions[index].code {
            if let Op::Call { function, .. } = instruction.op {
                self.visit_calls(function as usize, marks)?;
            }
        }
        marks[index] = 2;
        Ok(())
    }
    fn register(&self, function: usize, pc: usize, register: u8) -> Result<Type, Diagnostic> {
        self.functions[function]
            .registers
            .get(register as usize)
            .copied()
            .ok_or_else(|| {
                self.diagnostic(
                    DiagnosticKind::Bytecode,
                    function,
                    pc,
                    "register outside function",
                )
            })
    }
    fn check_instruction(&self, index: usize, pc: usize) -> Result<(), Diagnostic> {
        let function = &self.functions[index];
        let instruction = function.code[pc];
        let error = || {
            self.diagnostic(
                DiagnosticKind::Bytecode,
                index,
                pc,
                "instruction type/target mismatch",
            )
        };
        let reg = |r| self.register(index, pc, r);
        match instruction.op {
            Op::Const { dst, value } => {
                if reg(dst)? != value.ty()
                    || matches!(value, Value::Entity(Some(_)) | Value::Task(Some(_)))
                {
                    return Err(error());
                }
            }
            Op::Copy { dst, src } => {
                if reg(dst)? != reg(src)? {
                    return Err(error());
                }
            }
            Op::Unary { dst, src, op } => {
                let ty = reg(src)?;
                if reg(dst)? != ty
                    || match op {
                        Unary::Not => ty != Type::Bool,
                        Unary::Neg => !matches!(ty, Type::Int | Type::Fixed | Type::Vec),
                    }
                {
                    return Err(error());
                }
            }
            Op::Binary {
                dst,
                left,
                right,
                op,
            } => {
                if binary_type(op, reg(left)?, reg(right)?) != Some(reg(dst)?) {
                    return Err(error());
                }
            }
            Op::Jump { target } => {
                if target as usize >= function.code.len() {
                    return Err(error());
                }
            }
            Op::Branch {
                condition, target, ..
            } => {
                if reg(condition)? != Type::Bool || target as usize >= function.code.len() {
                    return Err(error());
                }
            }
            Op::Call {
                dst,
                function: callee,
                args,
                len,
            }
            | Op::Fork {
                dst,
                function: callee,
                args,
                len,
            } => {
                let Some(callee) = self.functions.get(callee as usize) else {
                    return Err(error());
                };
                let fork = matches!(instruction.op, Op::Fork { .. });
                if len as usize > 8
                    || len as usize != callee.params.len()
                    || callee.task != fork
                    || fork && !function.task
                    || reg(dst)? != if fork { Type::Task } else { callee.result }
                {
                    return Err(error());
                }
                for (&argument, &ty) in args[..len as usize].iter().zip(&callee.params) {
                    if reg(argument)? != ty {
                        return Err(error());
                    }
                }
            }
            Op::Builtin {
                dst,
                builtin,
                args,
                len,
            } => {
                let (params, result) = builtin.signature();
                if len as usize > 8
                    || len as usize != params.len()
                    || reg(dst)? != result
                    || builtin.task_only() && !function.task
                {
                    return Err(error());
                }
                for (&arg, &ty) in args[..len as usize].iter().zip(params) {
                    if reg(arg)? != ty {
                        return Err(error());
                    }
                }
            }
            Op::Return { value } => {
                if value.map(reg).transpose()?.unwrap_or(Type::Unit) != function.result {
                    return Err(error());
                }
            }
        }
        Ok(())
    }
    fn check_flow(&self, index: usize) -> Result<(), Diagnostic> {
        let function = &self.functions[index];
        let mut incoming = vec![None; function.code.len()];
        let mut queue = VecDeque::new();
        incoming[0] = Some((1u64 << function.params.len()) - 1);
        queue.push_back(0);
        while let Some(pc) = queue.pop_front() {
            let mut defined = incoming[pc].expect("queued state");
            let instruction = function.code[pc];
            let check = |register: u8| {
                if defined & (1u64 << register) == 0 {
                    Err(self.diagnostic(
                        DiagnosticKind::Bytecode,
                        index,
                        pc,
                        "register may be read before initialization",
                    ))
                } else {
                    Ok(())
                }
            };
            let destination = match instruction.op {
                Op::Const { dst, .. } => Some(dst),
                Op::Copy { dst, src } | Op::Unary { dst, src, .. } => {
                    check(src)?;
                    Some(dst)
                }
                Op::Binary {
                    dst, left, right, ..
                } => {
                    check(left)?;
                    check(right)?;
                    Some(dst)
                }
                Op::Call { dst, args, len, .. }
                | Op::Fork { dst, args, len, .. }
                | Op::Builtin { dst, args, len, .. } => {
                    for &r in &args[..len as usize] {
                        check(r)?;
                    }
                    Some(dst)
                }
                Op::Branch { condition, .. } => {
                    check(condition)?;
                    None
                }
                Op::Return { value } => {
                    if let Some(r) = value {
                        check(r)?;
                    }
                    None
                }
                _ => None,
            };
            if let Some(dst) = destination {
                defined |= 1u64 << dst;
            }
            let successors = match instruction.op {
                Op::Return { .. } => [None, None],
                Op::Jump { target } => [Some(target as usize), None],
                Op::Branch { target, .. } => [Some(target as usize), Some(pc + 1)],
                _ => [Some(pc + 1), None],
            };
            for next in successors.into_iter().flatten() {
                if next >= function.code.len() {
                    return Err(self.diagnostic(
                        DiagnosticKind::Bytecode,
                        index,
                        pc,
                        "reachable fallthrough past function",
                    ));
                }
                let merged = incoming[next].map_or(defined, |old| old & defined);
                if incoming[next] != Some(merged) {
                    incoming[next] = Some(merged);
                    queue.push_back(next);
                }
            }
        }
        Ok(())
    }
}
pub(super) fn binary_type(op: Binary, left: Type, right: Type) -> Option<Type> {
    use Binary::*;
    use Type::*;
    match op {
        Add | Sub if left == right && matches!(left, Int | Fixed | Vec) => Some(left),
        Mul if left == right && matches!(left, Int | Fixed) => Some(left),
        Mul if left == Vec && right == Fixed || left == Fixed && right == Vec => Some(Vec),
        Div if left == right && matches!(left, Int | Fixed) || left == Vec && right == Fixed => {
            Some(left)
        }
        Rem if left == Int && right == Int => Some(Int),
        Eq | Ne if left == right && left != Unit => Some(Bool),
        Lt | Le | Gt | Ge if left == right && matches!(left, Int | Fixed) => Some(Bool),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrupt_targets_types_and_uninitialized_registers_are_rejected() {
        for op in [
            Op::Jump { target: u32::MAX },
            Op::Copy { dst: 0, src: 63 },
            Op::Binary {
                dst: 0,
                left: 0,
                right: 0,
                op: Binary::Add,
            },
            Op::Builtin {
                dst: 0,
                builtin: Builtin::Wait,
                args: [0; 8],
                len: 8,
            },
        ] {
            let mut program =
                Program::compile("test.graze", "task main() { let n = 1; wait(n); }").unwrap();
            program.functions[program.entry as usize].code[0].op = op;
            assert!(Program::from_bytes(&program.to_bytes()).is_err());
        }
    }
}
