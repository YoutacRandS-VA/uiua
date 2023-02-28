use std::{collections::HashMap, error::Error, fmt, io, mem::take, path::Path};

use crate::{
    ast::*,
    lex::Sp,
    parse::{parse, ParseError},
    types::Type,
};

#[derive(Debug)]
pub enum TranspileError {
    Io(io::Error),
    Parse(ParseError),
    InvalidInteger(String),
    InvalidReal(String),
    UnknownBinding(String),
    TypeMismatch(Type, Type),
}

impl fmt::Display for TranspileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TranspileError::Io(e) => write!(f, "{e}"),
            TranspileError::Parse(e) => write!(f, "{e}"),
            TranspileError::InvalidInteger(s) => write!(f, "invalid integer: {s}"),
            TranspileError::InvalidReal(s) => write!(f, "invalid real: {s}"),
            TranspileError::UnknownBinding(s) => write!(f, "unknown binding: {s}"),
            TranspileError::TypeMismatch(expected, actual) => {
                write!(f, "type mismatch: expected {expected}, got {actual}")
            }
        }
    }
}

impl Error for TranspileError {}

pub type TranspileResult<T = ()> = Result<T, Sp<TranspileError>>;

#[derive(Debug)]
pub struct Transpiler {
    pub(crate) code: String,
    indentation: usize,
    scopes: Vec<Scope>,
    pub(crate) function_replacements: HashMap<String, String>,
}

impl Default for Transpiler {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
pub(crate) struct Scope {
    pub bindings: HashMap<String, Type>,
}

impl Transpiler {
    pub(crate) fn new() -> Self {
        Self {
            code: String::new(),
            indentation: 0,
            scopes: vec![Scope::default()],
            function_replacements: HashMap::new(),
        }
    }
    pub(crate) fn scope_mut(&mut self) -> &mut Scope {
        self.scopes.last_mut().unwrap()
    }
    pub(crate) fn find_binding(&self, name: &str) -> Option<Type> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.bindings.get(name))
            .cloned()
    }
    pub fn transpile(&mut self, input: &str, path: &Path) -> Result<(), Vec<Sp<TranspileError>>> {
        let (items, errors) = parse(input, path);

        for item in &items {
            println!("{item:#?}");
        }

        let mut errors: Vec<_> = errors
            .into_iter()
            .map(|e| e.map(TranspileError::Parse))
            .collect();
        for item in items {
            if let Err(e) = self.item(item) {
                errors.push(e);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    fn item(&mut self, item: Item) -> TranspileResult {
        match item {
            Item::FunctionDef(def) => self.function_def(def),
            Item::Expr(expr, _) => self.expr(expr),
            Item::Binding(binding) => self.binding(binding),
        }
    }
    fn add(&mut self, s: impl Into<String>) {
        if self.code.ends_with('\n') {
            for _ in 0..self.indentation * 4 {
                self.code.push(' ');
            }
        }
        self.code.push_str(&s.into());
    }
    fn line(&mut self, s: impl Into<String>) {
        self.add(s);
        self.code.push('\n');
    }
    fn ensure_line(&mut self) {
        let ends_with_newline = self
            .code
            .chars()
            .rev()
            .find(|c| *c != ' ')
            .map_or(true, |c| c == '\n');
        if !ends_with_newline {
            self.code.push('\n');
        }
    }
    fn function_def(&mut self, def: FunctionDef) -> TranspileResult {
        self.add(format!("function {}(", def.name.value));
        for (i, param) in def.params.into_iter().enumerate() {
            if i > 0 {
                self.add(", ");
            }
            self.add(param.name.value);
        }
        self.line(")");
        self.indentation += 1;
        for binding in def.bindings {
            self.binding(binding)?;
        }
        self.add("return ");
        self.expr(def.ret)?;
        self.ensure_line();
        self.indentation -= 1;
        self.line("end");
        Ok(())
    }
    fn binding(&mut self, binding: Binding) -> TranspileResult {
        match binding.pattern.value {
            Pattern::Ident(ident) => {
                self.add(format!("local {ident} = "));
                self.expr(binding.expr)?;
                self.ensure_line();
            }
            Pattern::Tuple(items) => {
                // Initial expression binding
                self.add("local ");
                let mut groups = Vec::new();
                for (i, item) in items.into_iter().enumerate() {
                    if i > 0 {
                        self.add(", ");
                    }
                    match item.value {
                        Pattern::Ident(ident) => self.add(ident),
                        Pattern::Tuple(items) => {
                            let name = format!("tuple_{}", i);
                            self.add(name.clone());
                            groups.push((name, items));
                        }
                    }
                }
                self.add(" = ");
                self.add("unpack(");
                self.expr(binding.expr)?;
                self.add(")");
                self.ensure_line();
                // Subpattern bindings
                while !groups.is_empty() {
                    for (name, items) in take(&mut groups) {
                        self.add("local ");
                        for (i, item) in items.into_iter().enumerate() {
                            if i > 0 {
                                self.add(", ");
                            }
                            match item.value {
                                Pattern::Ident(ident) => self.add(ident),
                                Pattern::Tuple(items) => {
                                    let name = format!("tuple_{}_{}", name, i);
                                    self.add(name.clone());
                                    groups.push((name, items));
                                }
                            }
                        }
                        self.add(" = ");
                        self.add("unpack(");
                        self.add(name);
                        self.add(")");
                        self.ensure_line();
                    }
                }
            }
        }
        Ok(())
    }
    fn expr(&mut self, expr: Sp<Expr>) -> TranspileResult {
        match expr.value {
            Expr::Struct(_) => todo!(),
            Expr::Enum(_) => todo!(),
            Expr::Ident(ident) => self.add(ident),
            Expr::Tuple(items) => {
                self.add("{");
                for (i, item) in items.into_iter().enumerate() {
                    if i > 0 {
                        self.add(", ");
                    }
                    self.expr(item)?;
                }
                self.add("}");
            }
            Expr::List(_) => todo!(),
            Expr::Integer(i) => self.add(
                i.parse::<u64>()
                    .map_err(|_| expr.span.sp(TranspileError::InvalidInteger(i)))?
                    .to_string(),
            ),
            Expr::Real(r) => self.add(
                r.parse::<f64>()
                    .map_err(|_| expr.span.sp(TranspileError::InvalidReal(r)))?
                    .to_string(),
            ),
            Expr::Bool(b) => self.add(b.to_string()),
            Expr::Bin(bin) => self.bin_expr(*bin)?,
            Expr::Un(un) => self.un_expr(*un)?,
            Expr::If(if_expr) => self.if_expr(*if_expr)?,
            Expr::Call(call) => self.call(*call)?,
            Expr::Parened(inner) => {
                self.expr(expr.span.sp(*inner))?;
            }
        }
        Ok(())
    }
    fn call(&mut self, call: CallExpr) -> TranspileResult {
        self.expr(call.func)?;
        self.add("(");
        for (i, arg) in call.args.into_iter().enumerate() {
            if i > 0 {
                self.add(", ");
            }
            self.expr(arg)?;
        }
        self.add(")");
        Ok(())
    }
    fn bin_expr(&mut self, bin: BinExpr) -> TranspileResult {
        self.expr(bin.lhs)?;
        for (op, rhs) in bin.rhs {
            self.add(format!(
                " {} ",
                match op.value {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Eq => "==",
                    BinOp::Ne => "~=",
                    BinOp::Lt => "<",
                    BinOp::Le => "<=",
                    BinOp::Gt => ">",
                    BinOp::Ge => ">=",
                    BinOp::And => "and",
                    BinOp::Or => "or",
                    BinOp::RangeEx => todo!(),
                }
            ));
            self.expr(rhs)?;
        }
        Ok(())
    }
    fn un_expr(&mut self, un: UnExpr) -> TranspileResult {
        self.add(match un.op.value {
            UnOp::Neg => "-",
            UnOp::Not => "not ",
        });
        self.expr(un.expr)?;
        Ok(())
    }
    fn if_expr(&mut self, if_expr: IfExpr) -> TranspileResult {
        self.expr(if_expr.cond)?;
        self.add(" and ");
        self.expr(if_expr.if_true)?;
        self.add(" or ");
        self.expr(if_expr.if_false)?;
        Ok(())
    }
}
