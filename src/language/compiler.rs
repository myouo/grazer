use super::*;

#[derive(Clone)]
enum TokenKind {
    Name(String),
    Number(String),
    Symbol(&'static str),
    End,
}
#[derive(Clone)]
struct Token {
    kind: TokenKind,
    span: Span,
}
fn lex(file: &str, source: &str) -> Result<Vec<Token>, Diagnostic> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(Diagnostic::new(
            DiagnosticKind::Syntax,
            file,
            source,
            Span::default(),
            "source exceeds 1 MiB",
        ));
    }
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes[i..].starts_with(b"/*") {
            let start = i;
            i += 2;
            while i + 1 < bytes.len() && !bytes[i..].starts_with(b"*/") {
                i += 1;
            }
            if i + 1 == bytes.len() || i >= bytes.len() {
                return Err(Diagnostic::new(
                    DiagnosticKind::Syntax,
                    file,
                    source,
                    Span {
                        start: start as u32,
                        end: bytes.len() as u32,
                    },
                    "unterminated comment",
                ));
            }
            i += 2;
            continue;
        }
        let start = i;
        let kind = if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            TokenKind::Name(source[start..i].into())
        } else if bytes[i].is_ascii_digit() {
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'.' | b'_'))
            {
                i += 1;
            }
            TokenKind::Number(source[start..i].into())
        } else {
            let pair = if i + 1 < bytes.len() {
                &bytes[i..i + 2]
            } else {
                b""
            };
            let symbol = match pair {
                b"->" => Some("->"),
                b"==" => Some("=="),
                b"!=" => Some("!="),
                b"<=" => Some("<="),
                b">=" => Some(">="),
                b"&&" => Some("&&"),
                b"||" => Some("||"),
                _ => None,
            };
            if let Some(symbol) = symbol {
                i += 2;
                TokenKind::Symbol(symbol)
            } else {
                let symbol = match bytes[i] {
                    b'(' => "(",
                    b')' => ")",
                    b'{' => "{",
                    b'}' => "}",
                    b',' => ",",
                    b';' => ";",
                    b':' => ":",
                    b'=' => "=",
                    b'+' => "+",
                    b'-' => "-",
                    b'*' => "*",
                    b'/' => "/",
                    b'%' => "%",
                    b'!' => "!",
                    b'<' => "<",
                    b'>' => ">",
                    _ => {
                        return Err(Diagnostic::new(
                            DiagnosticKind::Syntax,
                            file,
                            source,
                            Span {
                                start: start as u32,
                                end: (start
                                    + source[start..]
                                        .chars()
                                        .next()
                                        .expect("character")
                                        .len_utf8()) as u32,
                            },
                            "unexpected character; identifiers are ASCII",
                        ));
                    }
                };
                i += 1;
                TokenKind::Symbol(symbol)
            }
        };
        out.push(Token {
            kind,
            span: Span {
                start: start as u32,
                end: i as u32,
            },
        });
        if out.len() > 65536 {
            return Err(Diagnostic::new(
                DiagnosticKind::Syntax,
                file,
                source,
                out.last().expect("token").span,
                "token limit exceeded",
            ));
        }
    }
    out.push(Token {
        kind: TokenKind::End,
        span: Span {
            start: i as u32,
            end: i as u32,
        },
    });
    Ok(out)
}
#[derive(Clone)]
struct Expr {
    kind: ExprKind,
    span: Span,
}
#[derive(Clone)]
enum ExprKind {
    Number(String),
    Bool(bool),
    Name(String),
    Unary(&'static str, Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>, bool),
}
#[derive(Clone)]
struct Statement {
    kind: StatementKind,
    span: Span,
}
#[derive(Clone)]
enum StatementKind {
    Let(String, Option<Type>, Expr),
    Assign(String, Expr),
    Expr(Expr),
    If(Expr, Vec<Statement>, Vec<Statement>),
    While(Expr, Vec<Statement>),
    Return(Option<Expr>),
}
struct AstFunction {
    name: String,
    params: Vec<(String, Type)>,
    result: Type,
    task: bool,
    body: Vec<Statement>,
    span: Span,
}
struct Parser<'a> {
    file: &'a str,
    source: &'a str,
    tokens: Vec<Token>,
    cursor: usize,
    depth: usize,
}
impl<'a> Parser<'a> {
    fn error(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(
            DiagnosticKind::Syntax,
            self.file,
            self.source,
            span,
            message,
        )
    }
    fn token(&self) -> &Token {
        &self.tokens[self.cursor]
    }
    fn take(&mut self) -> Token {
        let token = self.token().clone();
        if !matches!(token.kind, TokenKind::End) {
            self.cursor += 1;
        }
        token
    }
    fn is(&self, text: &str) -> bool {
        match &self.token().kind {
            TokenKind::Name(name) => name == text,
            TokenKind::Symbol(symbol) => *symbol == text,
            _ => false,
        }
    }
    fn eat(&mut self, text: &str) -> bool {
        if self.is(text) {
            self.take();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, text: &str) -> Result<Token, Diagnostic> {
        if self.is(text) {
            Ok(self.take())
        } else {
            Err(self.error(self.token().span, format!("expected '{text}'")))
        }
    }
    fn name(&mut self) -> Result<(String, Span), Diagnostic> {
        let token = self.take();
        if let TokenKind::Name(name) = token.kind {
            Ok((name, token.span))
        } else {
            Err(self.error(token.span, "expected identifier"))
        }
    }
    fn ty(&mut self) -> Result<Type, Diagnostic> {
        let (name, span) = self.name()?;
        match name.as_str() {
            "unit" => Ok(Type::Unit),
            "int" => Ok(Type::Int),
            "bool" => Ok(Type::Bool),
            "fixed" => Ok(Type::Fixed),
            "vec" => Ok(Type::Vec),
            "entity" => Ok(Type::Entity),
            "task" => Ok(Type::Task),
            _ => Err(self.error(span, "unknown type")),
        }
    }
    fn enter(&mut self) -> Result<(), Diagnostic> {
        self.depth += 1;
        if self.depth > 64 {
            return Err(self.error(self.token().span, "syntax nesting exceeds 64"));
        }
        Ok(())
    }
    fn program(&mut self) -> Result<Vec<AstFunction>, Diagnostic> {
        let mut functions = Vec::new();
        while !matches!(self.token().kind, TokenKind::End) {
            let start = self.token().span;
            let task = if self.eat("task") {
                true
            } else {
                self.expect("fn")?;
                false
            };
            let (name, _) = self.name()?;
            self.expect("(")?;
            let mut params = Vec::new();
            if !self.is(")") {
                loop {
                    let (name, _) = self.name()?;
                    self.expect(":")?;
                    params.push((name, self.ty()?));
                    if params.len() > MAX_ARGUMENTS {
                        return Err(self.error(start, "at most eight parameters"));
                    }
                    if !self.eat(",") {
                        break;
                    }
                }
            }
            self.expect(")")?;
            let result = if self.eat("->") {
                self.ty()?
            } else {
                Type::Unit
            };
            if task && result != Type::Unit {
                return Err(self.error(start, "tasks cannot return values"));
            }
            let body = self.block()?;
            functions.push(AstFunction {
                name,
                params,
                result,
                task,
                body,
                span: start,
            });
            if functions.len() > MAX_FUNCTIONS {
                return Err(self.error(start, "function limit exceeded"));
            }
        }
        Ok(functions)
    }
    fn block(&mut self) -> Result<Vec<Statement>, Diagnostic> {
        self.enter()?;
        self.expect("{")?;
        let mut out = Vec::new();
        while !self.is("}") {
            if matches!(self.token().kind, TokenKind::End) {
                return Err(self.error(self.token().span, "unterminated block"));
            }
            out.push(self.statement()?);
        }
        self.expect("}")?;
        self.depth -= 1;
        Ok(out)
    }
    fn statement(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.token().span;
        let kind = if self.eat("let") {
            let (name, _) = self.name()?;
            let ty = if self.eat(":") {
                Some(self.ty()?)
            } else {
                None
            };
            self.expect("=")?;
            let value = self.expression(0)?;
            self.expect(";")?;
            StatementKind::Let(name, ty, value)
        } else if self.eat("if") {
            let condition = self.expression(0)?;
            let yes = self.block()?;
            let no = if self.eat("else") {
                self.block()?
            } else {
                Vec::new()
            };
            StatementKind::If(condition, yes, no)
        } else if self.eat("while") {
            let condition = self.expression(0)?;
            StatementKind::While(condition, self.block()?)
        } else if self.eat("return") {
            let value = if self.is(";") {
                None
            } else {
                Some(self.expression(0)?)
            };
            self.expect(";")?;
            StatementKind::Return(value)
        } else if matches!(&self.token().kind, TokenKind::Name(_))
            && self
                .tokens
                .get(self.cursor + 1)
                .is_some_and(|t| matches!(t.kind, TokenKind::Symbol("=")))
        {
            let (name, _) = self.name()?;
            self.expect("=")?;
            let value = self.expression(0)?;
            self.expect(";")?;
            StatementKind::Assign(name, value)
        } else {
            let value = self.expression(0)?;
            self.expect(";")?;
            StatementKind::Expr(value)
        };
        Ok(Statement {
            kind,
            span: Span {
                start: start.start,
                end: self.tokens[self.cursor.saturating_sub(1)].span.end,
            },
        })
    }
    fn expression(&mut self, minimum: u8) -> Result<Expr, Diagnostic> {
        self.enter()?;
        let token = self.take();
        let mut left = match token.kind {
            TokenKind::Number(number) => Expr {
                kind: ExprKind::Number(number),
                span: token.span,
            },
            TokenKind::Symbol("(") => {
                let expr = self.expression(0)?;
                self.expect(")")?;
                expr
            }
            TokenKind::Symbol(op @ ("-" | "!")) => {
                let right = self.expression(7)?;
                Expr {
                    span: Span {
                        start: token.span.start,
                        end: right.span.end,
                    },
                    kind: ExprKind::Unary(op, Box::new(right)),
                }
            }
            TokenKind::Name(name) if name == "true" || name == "false" => Expr {
                kind: ExprKind::Bool(name == "true"),
                span: token.span,
            },
            TokenKind::Name(name) => {
                let (name, fork) = if name == "fork" {
                    let (name, _) = self.name()?;
                    (name, true)
                } else {
                    (name, false)
                };
                if self.eat("(") {
                    let args = self.arguments()?;
                    let end = self.tokens[self.cursor - 1].span.end;
                    Expr {
                        kind: ExprKind::Call(name, args, fork),
                        span: Span {
                            start: token.span.start,
                            end,
                        },
                    }
                } else if fork {
                    return Err(self.error(token.span, "fork requires a task call"));
                } else {
                    Expr {
                        kind: ExprKind::Name(name),
                        span: token.span,
                    }
                }
            }
            _ => return Err(self.error(token.span, "expected expression")),
        };
        loop {
            let (operator, priority) = match self.token().kind {
                TokenKind::Symbol(op @ "||") => (op, 1),
                TokenKind::Symbol(op @ "&&") => (op, 2),
                TokenKind::Symbol(op @ ("==" | "!=")) => (op, 3),
                TokenKind::Symbol(op @ ("<" | "<=" | ">" | ">=")) => (op, 4),
                TokenKind::Symbol(op @ ("+" | "-")) => (op, 5),
                TokenKind::Symbol(op @ ("*" | "/" | "%")) => (op, 6),
                _ => break,
            };
            if priority < minimum {
                break;
            }
            self.take();
            let right = self.expression(priority + 1)?;
            let span = Span {
                start: left.span.start,
                end: right.span.end,
            };
            left = Expr {
                kind: ExprKind::Binary(operator, Box::new(left), Box::new(right)),
                span,
            };
        }
        self.depth -= 1;
        Ok(left)
    }
    fn arguments(&mut self) -> Result<Vec<Expr>, Diagnostic> {
        let mut args = Vec::new();
        if !self.is(")") {
            loop {
                args.push(self.expression(0)?);
                if args.len() > MAX_ARGUMENTS {
                    return Err(self.error(self.token().span, "at most eight arguments"));
                }
                if !self.eat(",") {
                    break;
                }
            }
        }
        self.expect(")")?;
        Ok(args)
    }
}
struct Binding {
    name: String,
    register: u8,
    depth: usize,
}
struct Generator<'a> {
    file: &'a str,
    source: &'a str,
    functions: &'a [AstFunction],
    current: &'a AstFunction,
    bindings: Vec<Binding>,
    registers: Vec<Type>,
    code: Vec<Instruction>,
    depth: usize,
}
impl Generator<'_> {
    fn error(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(DiagnosticKind::Type, self.file, self.source, span, message)
    }
    fn allocate(&mut self, ty: Type, span: Span) -> Result<u8, Diagnostic> {
        if self.registers.len() == MAX_REGISTERS {
            return Err(self.error(span, "function exceeds 64 registers"));
        }
        let reg = self.registers.len() as u8;
        self.registers.push(ty);
        Ok(reg)
    }
    fn emit(&mut self, op: Op, span: Span) -> usize {
        let index = self.code.len();
        self.code.push(Instruction { op, span });
        index
    }
    fn patch(&mut self, index: usize, target: usize) {
        match &mut self.code[index].op {
            Op::Jump { target: t } | Op::Branch { target: t, .. } => *t = target as u32,
            _ => unreachable!("jump patch"),
        }
    }
    fn require(&self, actual: Type, expected: Type, span: Span) -> Result<(), Diagnostic> {
        if actual != expected {
            Err(self.error(span, format!("expected {expected}, got {actual}")))
        } else {
            Ok(())
        }
    }
    fn binding(&self, name: &str, span: Span) -> Result<u8, Diagnostic> {
        self.bindings
            .iter()
            .rev()
            .find(|b| b.name == name)
            .map(|b| b.register)
            .ok_or_else(|| self.error(span, format!("unknown variable {name}")))
    }
    fn block(&mut self, body: &[Statement]) -> Result<(), Diagnostic> {
        let mark = self.bindings.len();
        self.depth += 1;
        for statement in body {
            self.statement(statement)?;
        }
        self.bindings.truncate(mark);
        self.depth -= 1;
        Ok(())
    }
    fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        let span = statement.span;
        match &statement.kind {
            StatementKind::Let(name, annotation, expr) => {
                if self
                    .bindings
                    .iter()
                    .any(|b| b.name == *name && b.depth == self.depth)
                {
                    return Err(self.error(span, "duplicate variable"));
                }
                let (src, ty) = self.expression(expr)?;
                if let Some(expected) = annotation {
                    self.require(ty, *expected, span)?;
                }
                if ty == Type::Unit {
                    return Err(self.error(span, "unit cannot be a variable"));
                }
                let dst = self.allocate(ty, span)?;
                self.emit(Op::Copy { dst, src }, span);
                self.bindings.push(Binding {
                    name: name.clone(),
                    register: dst,
                    depth: self.depth,
                });
            }
            StatementKind::Assign(name, expr) => {
                let dst = self.binding(name, span)?;
                let (src, ty) = self.expression(expr)?;
                self.require(ty, self.registers[dst as usize], span)?;
                self.emit(Op::Copy { dst, src }, span);
            }
            StatementKind::Expr(expr) => {
                self.expression(expr)?;
            }
            StatementKind::If(condition, yes, no) => {
                let (reg, ty) = self.expression(condition)?;
                self.require(ty, Type::Bool, condition.span)?;
                let branch = self.emit(
                    Op::Branch {
                        condition: reg,
                        when: false,
                        target: 0,
                    },
                    span,
                );
                self.block(yes)?;
                let end = self.emit(Op::Jump { target: 0 }, span);
                self.patch(branch, self.code.len());
                self.block(no)?;
                self.patch(end, self.code.len());
            }
            StatementKind::While(condition, body) => {
                let start = self.code.len();
                let (reg, ty) = self.expression(condition)?;
                self.require(ty, Type::Bool, condition.span)?;
                let exit = self.emit(
                    Op::Branch {
                        condition: reg,
                        when: false,
                        target: 0,
                    },
                    span,
                );
                self.block(body)?;
                self.emit(
                    Op::Jump {
                        target: start as u32,
                    },
                    span,
                );
                self.patch(exit, self.code.len());
            }
            StatementKind::Return(expr) => {
                let value = if let Some(expr) = expr {
                    let (reg, ty) = self.expression(expr)?;
                    self.require(ty, self.current.result, span)?;
                    Some(reg)
                } else {
                    self.require(Type::Unit, self.current.result, span)?;
                    None
                };
                self.emit(Op::Return { value }, span);
            }
        }
        Ok(())
    }
    fn constant(&mut self, value: Value, span: Span) -> Result<(u8, Type), Diagnostic> {
        let ty = value.ty();
        let dst = self.allocate(ty, span)?;
        self.emit(Op::Const { dst, value }, span);
        Ok((dst, ty))
    }
    fn expression(&mut self, expr: &Expr) -> Result<(u8, Type), Diagnostic> {
        let span = expr.span;
        match &expr.kind {
            ExprKind::Number(number) => {
                let value = literal(number, false)
                    .ok_or_else(|| self.error(span, "invalid or overflowing number literal"))?;
                self.constant(value, span)
            }
            ExprKind::Bool(value) => self.constant(Value::Bool(*value), span),
            ExprKind::Name(name) => {
                let reg = self.binding(name, span)?;
                Ok((reg, self.registers[reg as usize]))
            }
            ExprKind::Unary("-", right) if matches!(right.kind, ExprKind::Number(_)) => {
                let ExprKind::Number(number) = &right.kind else {
                    unreachable!()
                };
                let value = literal(number, true)
                    .ok_or_else(|| self.error(span, "invalid or overflowing negative literal"))?;
                self.constant(value, span)
            }
            ExprKind::Unary(operator, right) => {
                let (src, ty) = self.expression(right)?;
                let op = if *operator == "!" {
                    self.require(ty, Type::Bool, span)?;
                    Unary::Not
                } else {
                    if !matches!(ty, Type::Int | Type::Fixed | Type::Vec) {
                        return Err(self.error(span, "numeric negation required"));
                    }
                    Unary::Neg
                };
                let dst = self.allocate(ty, span)?;
                self.emit(Op::Unary { dst, src, op }, span);
                Ok((dst, ty))
            }
            ExprKind::Binary(operator, left, right) => {
                let (left_reg, left_type) = self.expression(left)?;
                if matches!(*operator, "&&" | "||") {
                    self.require(left_type, Type::Bool, span)?;
                    let dst = self.allocate(Type::Bool, span)?;
                    self.emit(Op::Copy { dst, src: left_reg }, span);
                    let end = self.emit(
                        Op::Branch {
                            condition: left_reg,
                            when: *operator == "||",
                            target: 0,
                        },
                        span,
                    );
                    let (reg, ty) = self.expression(right)?;
                    self.require(ty, Type::Bool, span)?;
                    self.emit(Op::Copy { dst, src: reg }, span);
                    self.patch(end, self.code.len());
                    return Ok((dst, Type::Bool));
                }
                let (right_reg, right_type) = self.expression(right)?;
                let op = match *operator {
                    "+" => Binary::Add,
                    "-" => Binary::Sub,
                    "*" => Binary::Mul,
                    "/" => Binary::Div,
                    "%" => Binary::Rem,
                    "==" => Binary::Eq,
                    "!=" => Binary::Ne,
                    "<" => Binary::Lt,
                    "<=" => Binary::Le,
                    ">" => Binary::Gt,
                    _ => Binary::Ge,
                };
                let ty =
                    super::program::binary_type(op, left_type, right_type).ok_or_else(|| {
                        self.error(
                            span,
                            format!(
                                "operator {operator} cannot combine {left_type} and {right_type}"
                            ),
                        )
                    })?;
                let dst = self.allocate(ty, span)?;
                self.emit(
                    Op::Binary {
                        dst,
                        left: left_reg,
                        right: right_reg,
                        op,
                    },
                    span,
                );
                Ok((dst, ty))
            }
            ExprKind::Call(name, args, fork) => {
                let builtin = Builtin::ALL.into_iter().find(|b| b.name() == name);
                let function = self.functions.iter().position(|f| f.name == *name);
                let (params, result) = if *fork {
                    let Some(index) = function else {
                        return Err(self.error(span, "unknown task"));
                    };
                    let f = &self.functions[index];
                    if !f.task || !self.current.task {
                        return Err(self.error(span, "fork is only valid for tasks inside tasks"));
                    }
                    (f.params.iter().map(|p| p.1).collect::<Vec<_>>(), Type::Task)
                } else if let Some(builtin) = builtin {
                    if builtin.task_only() && !self.current.task {
                        return Err(self.error(span, "task control is not allowed in a function"));
                    }
                    let (p, r) = builtin.signature();
                    (p.to_vec(), r)
                } else if let Some(index) = function {
                    let f = &self.functions[index];
                    if f.task {
                        return Err(self.error(span, "tasks must be forked"));
                    }
                    (f.params.iter().map(|p| p.1).collect(), f.result)
                } else {
                    return Err(self.error(span, format!("unknown function {name}")));
                };
                if args.len() != params.len() {
                    return Err(
                        self.error(span, format!("{name} expects {} arguments", params.len()))
                    );
                }
                let mut registers = [0; 8];
                for (index, (arg, expected)) in args.iter().zip(params).enumerate() {
                    let (reg, ty) = self.expression(arg)?;
                    self.require(ty, expected, arg.span)?;
                    registers[index] = reg;
                }
                let dst = self.allocate(result, span)?;
                let len = args.len() as u8;
                let op = if *fork {
                    Op::Fork {
                        dst,
                        function: function.expect("task") as u16,
                        args: registers,
                        len,
                    }
                } else if let Some(builtin) = builtin {
                    Op::Builtin {
                        dst,
                        builtin,
                        args: registers,
                        len,
                    }
                } else {
                    Op::Call {
                        dst,
                        function: function.expect("function") as u16,
                        args: registers,
                        len,
                    }
                };
                self.emit(op, span);
                Ok((dst, result))
            }
        }
    }
}
fn literal(text: &str, negative: bool) -> Option<Value> {
    let text = text.replace('_', "");
    if let Some(hex) = text.strip_prefix("0x") {
        let raw = u32::from_str_radix(hex, 16).ok()?;
        let value = raw as i32;
        return Some(Value::Int(if negative {
            value.checked_neg()?
        } else {
            value
        }));
    }
    if text.contains('.') || text.ends_with('f') {
        let text = text.strip_suffix('f').unwrap_or(&text);
        let mut parts = text.split('.');
        let whole = parts.next()?.parse::<i64>().ok()?;
        let fractional = parts.next().unwrap_or("");
        if parts.next().is_some() || fractional.len() > 9 {
            return None;
        }
        let fraction = if fractional.is_empty() {
            0
        } else {
            fractional.parse::<i64>().ok()?
        };
        let scale = 10i64.pow(fractional.len() as u32);
        let raw = whole
            .checked_mul(65536)?
            .checked_add(fraction.checked_mul(65536)? / scale)?;
        let raw = if negative { raw.checked_neg()? } else { raw };
        Some(Value::Fixed(Fixed::from_bits(i32::try_from(raw).ok()?)))
    } else {
        let raw = text.parse::<i64>().ok()?;
        Some(Value::Int(
            i32::try_from(if negative { -raw } else { raw }).ok()?,
        ))
    }
}
fn returns(body: &[Statement]) -> bool {
    body.iter().any(|s| match &s.kind {
        StatementKind::Return(_) => true,
        StatementKind::If(_, yes, no) => returns(yes) && returns(no),
        _ => false,
    })
}
pub(super) fn compile(file: &str, source: &str) -> Result<Program, Diagnostic> {
    if file.len() > 4096 {
        return Err(Diagnostic::data(
            DiagnosticKind::Syntax,
            "source name exceeds 4096 bytes",
        ));
    }
    let tokens = lex(file, source)?;
    let functions = Parser {
        file,
        source,
        tokens,
        cursor: 0,
        depth: 0,
    }
    .program()?;
    for (index, function) in functions.iter().enumerate() {
        if functions[..index].iter().any(|f| f.name == function.name)
            || Builtin::ALL.iter().any(|b| b.name() == function.name)
        {
            return Err(Diagnostic::new(
                DiagnosticKind::Type,
                file,
                source,
                function.span,
                "duplicate or reserved function name",
            ));
        }
        for (i, (name, ty)) in function.params.iter().enumerate() {
            if *ty == Type::Unit || function.params[..i].iter().any(|p| p.0 == *name) {
                return Err(Diagnostic::new(
                    DiagnosticKind::Type,
                    file,
                    source,
                    function.span,
                    "invalid or duplicate parameter",
                ));
            }
        }
    }
    let entry = functions
        .iter()
        .position(|f| f.name == "main" && f.task && f.params.is_empty())
        .ok_or_else(|| {
            Diagnostic::new(
                DiagnosticKind::Type,
                file,
                source,
                Span::default(),
                "declare task main()",
            )
        })?;
    let mut compiled = Vec::new();
    for function in &functions {
        let mut generator = Generator {
            file,
            source,
            functions: &functions,
            current: function,
            bindings: Vec::new(),
            registers: Vec::new(),
            code: Vec::new(),
            depth: 0,
        };
        for (name, ty) in &function.params {
            let reg = generator.allocate(*ty, function.span)?;
            generator.bindings.push(Binding {
                name: name.clone(),
                register: reg,
                depth: 0,
            });
        }
        generator.block(&function.body)?;
        if function.result != Type::Unit && !returns(&function.body) {
            return Err(generator.error(
                function.span,
                "function may finish without returning its declared type",
            ));
        }
        if function.result == Type::Unit {
            generator.emit(Op::Return { value: None }, function.span);
        } else {
            let (reg, _) = generator.constant(function.result.zero(), function.span)?;
            generator.emit(Op::Return { value: Some(reg) }, function.span);
        }
        compiled.push(Function {
            name: function.name.clone(),
            params: function.params.iter().map(|p| p.1).collect(),
            result: function.result,
            task: function.task,
            registers: generator.registers,
            code: generator.code,
        });
    }
    Program::checked(file.into(), source.into(), entry as u16, compiled)
}
