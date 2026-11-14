//! Vim9 script evaluator (Phase 2).
//!
//! This is an AST interpreter over the parser output.  It is deliberately
//! independent from the legacy VimL interpreter: values are the legacy
//! [Value] type (plus tuples), builtins are delegated to the legacy builtin
//! implementations, and higher order builtins are implemented here so Vim9
//! closures and nested def functions work.
//!
//! Script variables are script-local (no s: prefix), nested defs and lambdas
//! capture the defining scope by reference.  Classes, interfaces, enums and
//! imports are Phase 3/4 and currently report a clear not-implemented error.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rvim_types::{ScriptId, VimString};

use crate::error::{Result, VimError};
use crate::host::{BasicHost, OptionScope as HostOptionScope, VarScope, VimHost};
use crate::runtime::VimRuntime;
use crate::value::{FuncrefData, FuncrefKind, Value, VimDict};

use super::ast::*;
use super::eval_helpers::{
    assign_to_binary, normalize_index, not_implemented, same_value, slice_bounds, special_string,
    split_command, strtrans_text, trim_text,
};
use super::parser;
use super::value9::{funcref_id, funcref_name, Value9};

/// Result of running a complete Vim9 script.
pub struct RunOutput {
    /// Strings produced by :echo/:echomsg, in order.
    pub messages: Vec<String>,
}

/// Parse and execute a complete Vim9 script.
pub fn run_script(source: &[u8]) -> Result<RunOutput> {
    let script = parser::parse_script(source)?;
    let mut interpreter = Interpreter::new();
    interpreter.execute_script(&script)?;
    Ok(RunOutput {
        messages: interpreter.messages,
    })
}

/// Control flow inside a function body or block.
#[derive(Debug)]
enum Flow {
    Normal,
    Break,
    Continue,
    Return(Value9),
    Finish,
}

#[derive(Clone, Debug)]
struct Slot {
    value: Value9,
    locked: bool,
}

/// A lexical environment: locals, functions and a link to the defining scope.
#[derive(Default)]
struct Env {
    vars: HashMap<String, Slot>,
    functions: HashMap<String, Rc<Callable>>,
    parent: Option<Rc<RefCell<Env>>>,
}

/// A callable: a compiled def or a lambda with a captured environment.
enum Callable {
    Function {
        def: Rc<DefDecl>,
        env: Rc<RefCell<Env>>,
    },
    Lambda(Rc<LambdaClosure>),
}

struct LambdaClosure {
    params: Vec<Param>,
    body: LambdaBody,
    env: Rc<RefCell<Env>>,
}

struct Interpreter {
    /// Legacy runtime, used for v: variables and builtin delegation.
    runtime: VimRuntime,
    /// In-memory host: options, buffer lines and registers.
    host: BasicHost,
    messages: Vec<String>,
    script_env: Rc<RefCell<Env>>,
    callables: Vec<Rc<Callable>>,
    cursor_col: usize,
}

impl Interpreter {
    fn new() -> Self {
        let mut runtime = VimRuntime::new();
        runtime.set_vvar("exception", Value::String(VimString::new()));
        runtime.set_vvar("stacktrace", Value::list(Vec::new()));
        runtime.set_vvar("errmsg", Value::String(VimString::new()));
        let mut host = BasicHost::new();
        for (name, value) in [
            ("tabstop", Value::Number(8)),
            ("shiftwidth", Value::Number(8)),
            ("softtabstop", Value::Number(0)),
            ("number", Value::Bool(false)),
            ("laststatus", Value::Number(1)),
            ("eventignore", Value::String(VimString::new())),
            ("cpoptions", Value::String(VimString::from("aABceFs"))),
            ("ignorecase", Value::Bool(false)),
            ("encoding", Value::String(VimString::from("utf-8"))),
            ("filetype", Value::String(VimString::new())),
        ] {
            host.options
                .insert((HostOptionScope::Default, name.to_string()), value);
        }
        host.lines.push(VimString::new());
        host.current_line = 1;
        Interpreter {
            runtime,
            host,
            messages: Vec::new(),
            script_env: Rc::new(RefCell::new(Env::default())),
            callables: Vec::new(),
            cursor_col: 0,
        }
    }

    fn execute_script(&mut self, script: &Script) -> Result<()> {
        let env = self.script_env.clone();
        let flow = self.exec_items(&script.items, &env)?;
        match flow {
            Flow::Normal | Flow::Finish | Flow::Return(_) => Ok(()),
            Flow::Break | Flow::Continue => {
                Err(VimError::new(587, "E587: :break/:continue without loop"))
            }
        }
    }

    // ---------------------------------------------------------------
    // Environment helpers
    // ---------------------------------------------------------------

    fn frame(parent: &Rc<RefCell<Env>>) -> Rc<RefCell<Env>> {
        Rc::new(RefCell::new(Env {
            vars: HashMap::new(),
            functions: HashMap::new(),
            parent: Some(parent.clone()),
        }))
    }

    fn env_lookup(env: &Rc<RefCell<Env>>, name: &str) -> Option<Value9> {
        let mut current = Some(env.clone());
        while let Some(scope) = current {
            let borrowed = scope.borrow();
            if let Some(slot) = borrowed.vars.get(name) {
                return Some(slot.value.clone());
            }
            current = borrowed.parent.clone();
        }
        None
    }

    /// Assign to an existing variable in the enclosing scopes.
    fn env_assign(env: &Rc<RefCell<Env>>, name: &str, value: Value9) -> Result<()> {
        let mut current = Some(env.clone());
        while let Some(scope) = current {
            let mut borrowed = scope.borrow_mut();
            if let Some(slot) = borrowed.vars.get_mut(name) {
                if slot.locked {
                    return Err(VimError::new(
                        1018,
                        format!("E1018: Cannot change read-only variable: {name}"),
                    ));
                }
                slot.value = value;
                return Ok(());
            }
            current = borrowed.parent.clone();
        }
        // Permissive: an undeclared variable is created in the current scope.
        env.borrow_mut().vars.insert(
            name.to_string(),
            Slot {
                value,
                locked: false,
            },
        );
        Ok(())
    }

    fn env_define(env: &Rc<RefCell<Env>>, name: &str, value: Value9, locked: bool) {
        env.borrow_mut()
            .vars
            .insert(name.to_string(), Slot { value, locked });
    }

    fn env_function(env: &Rc<RefCell<Env>>, name: &str) -> Option<Rc<Callable>> {
        let mut current = Some(env.clone());
        while let Some(scope) = current {
            let borrowed = scope.borrow();
            if let Some(function) = borrowed.functions.get(name) {
                return Some(function.clone());
            }
            current = borrowed.parent.clone();
        }
        None
    }

    fn env_define_function(env: &Rc<RefCell<Env>>, name: &str, callable: Rc<Callable>) {
        env.borrow_mut()
            .functions
            .insert(name.to_string(), callable);
    }

    // ---------------------------------------------------------------
    // Callables and function calls
    // ---------------------------------------------------------------

    fn make_funcref(&mut self, callable: Rc<Callable>) -> Value9 {
        let id = self.callables.len();
        self.callables.push(callable);
        let data = FuncrefData::new(funcref_name(id), FuncrefKind::Lambda, ScriptId::default());
        Value9::Val(Value::funcref(data))
    }

    fn callable_from_value(&self, value: &Value) -> Option<Rc<Callable>> {
        let id = funcref_id(value)?;
        self.callables.get(id).cloned()
    }

    fn invoke_value(
        &mut self,
        value: &Value9,
        args: Vec<Value9>,
        env: &Rc<RefCell<Env>>,
    ) -> Result<Value9> {
        match value {
            Value9::Val(Value::Funcref(data)) => {
                if let Some(callable) = self.callable_from_value(&value.legacy()) {
                    return self.call_callable(&callable, args);
                }
                let name = data.borrow().name.to_string_lossy().into_owned();
                self.call_named(&name, args, env)
            }
            _ => Err(VimError::new(
                15,
                "E15: Invalid expression: value is not a function",
            )),
        }
    }

    fn call_callable(&mut self, callable: &Rc<Callable>, args: Vec<Value9>) -> Result<Value9> {
        match callable.as_ref() {
            Callable::Function { def, env } => {
                let frame = Self::frame(env);
                self.bind_params(&def.params, args, &frame, &def.name.to_string_lossy())?;
                let flow = self.exec_items(&def.body, &frame)?;
                Ok(match flow {
                    Flow::Return(value) => value,
                    _ => Value9::none(),
                })
            }
            Callable::Lambda(closure) => {
                let frame = Self::frame(&closure.env);
                self.bind_params(&closure.params, args, &frame, "<lambda>")?;
                match &closure.body {
                    LambdaBody::Expr(expression) => self.eval_expr(expression, &frame),
                    LambdaBody::Block(statements) => {
                        let flow = self.exec_items(statements, &frame)?;
                        Ok(match flow {
                            Flow::Return(value) => value,
                            _ => Value9::none(),
                        })
                    }
                }
            }
        }
    }

    /// Bind actual arguments to parameters, evaluating defaults and collecting
    /// varargs into a list.
    fn bind_params(
        &mut self,
        params: &[Param],
        args: Vec<Value9>,
        frame: &Rc<RefCell<Env>>,
        function: &str,
    ) -> Result<()> {
        let mut args = args.into_iter();
        for param in params {
            let name = param.name.to_string_lossy().into_owned();
            if param.varargs {
                let rest: Vec<Value9> = args.by_ref().collect();
                let list = Value9::list(rest);
                Self::env_define(frame, &name, list, false);
                continue;
            }
            let value = match args.next() {
                Some(value) => value,
                None => {
                    if let Some(default) = &param.default {
                        self.eval_expr(default, frame)?
                    } else if param.optional {
                        Value9::null()
                    } else {
                        return Err(VimError::new(
                            119,
                            format!("E119: Not enough arguments for function: {function}"),
                        ));
                    }
                }
            };
            Self::env_define(frame, &name, value, false);
        }
        if args.next().is_some() {
            return Err(VimError::new(
                118,
                format!("E118: Too many arguments for function: {function}"),
            ));
        }
        Ok(())
    }

    // ---------------------------------------------------------------
    // Statement execution
    // ---------------------------------------------------------------

    fn exec_items(&mut self, statements: &[Stmt], env: &Rc<RefCell<Env>>) -> Result<Flow> {
        for statement in statements {
            let flow = self.exec_stmt(statement, env)?;
            if !matches!(flow, Flow::Normal) {
                return Ok(flow);
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_stmt(&mut self, statement: &Stmt, env: &Rc<RefCell<Env>>) -> Result<Flow> {
        match statement {
            Stmt::Var(declaration) => {
                let value = match &declaration.init {
                    Some(expression) => self.eval_expr(expression, env)?,
                    None => Value9::null(),
                };
                let locked = declaration.kind != VarKind::Var;
                self.bind_pattern(&declaration.pattern, value, env, true, locked)?;
                Ok(Flow::Normal)
            }
            Stmt::Def(def) => {
                let callable = Rc::new(Callable::Function {
                    def: Rc::new(def.clone()),
                    env: env.clone(),
                });
                let name = def.name.to_string_lossy().into_owned();
                Self::env_define_function(env, &name, callable);
                Ok(Flow::Normal)
            }
            Stmt::If(statement) => {
                for branch in &statement.branches {
                    let condition = self.eval_expr(&branch.cond, env)?;
                    if condition.is_truthy() {
                        return self.exec_items(&branch.body, env);
                    }
                }
                if let Some(body) = &statement.else_body {
                    return self.exec_items(body, env);
                }
                Ok(Flow::Normal)
            }
            Stmt::While(statement) => {
                let mut guard = 0u64;
                loop {
                    guard += 1;
                    if guard > 10_000_000 {
                        return Err(VimError::new(
                            132,
                            "E132: Function call depth is higher than 'maxfuncdepth'",
                        ));
                    }
                    let condition = self.eval_expr(&statement.cond, env)?;
                    if !condition.is_truthy() {
                        return Ok(Flow::Normal);
                    }
                    match self.exec_items(&statement.body, env)? {
                        Flow::Normal | Flow::Continue => {}
                        Flow::Break => return Ok(Flow::Normal),
                        other => return Ok(other),
                    }
                }
            }
            Stmt::For(statement) => {
                let iterable = self.eval_expr(&statement.iterable, env)?;
                let items = self.sequence_values(&iterable)?;
                for item in items {
                    self.bind_pattern(&statement.pattern, item, env, false, false)?;
                    match self.exec_items(&statement.body, env)? {
                        Flow::Normal | Flow::Continue => {}
                        Flow::Break => return Ok(Flow::Normal),
                        other => return Ok(other),
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::Try(statement) => self.exec_try(statement, env),
            Stmt::Return { value, .. } => {
                let value = match value {
                    Some(expression) => self.eval_expr(expression, env)?,
                    None => Value9::none(),
                };
                Ok(Flow::Return(value))
            }
            Stmt::Throw { value, .. } => {
                let thrown = self.eval_expr(value, env)?;
                let text = match &thrown {
                    Value9::Val(Value::String(string)) => {
                        String::from_utf8_lossy(string.as_bytes()).into_owned()
                    }
                    other => other.display(),
                };
                Err(VimError::thrown(text))
            }
            Stmt::Break { .. } => Ok(Flow::Break),
            Stmt::Continue { .. } => Ok(Flow::Continue),
            Stmt::Defer { expr, .. } => {
                // Approximate: evaluate now.  Full Vim9 defers the call to the
                // end of the enclosing scope; the corpus does not rely on the
                // timing, and runtime files are handled in later phases.
                self.eval_expr(expr, env)?;
                Ok(Flow::Normal)
            }
            Stmt::Assign {
                target, op, value, ..
            } => {
                let value = self.eval_expr(value, env)?;
                let value = if *op == AssignOp::Assign {
                    value
                } else {
                    let current = self.eval_expr(target, env)?;
                    self.binary_value(assign_to_binary(*op), &current, &value)?
                };
                self.assign_target(target, value, env)?;
                Ok(Flow::Normal)
            }
            Stmt::Expr { expr, .. } => {
                self.eval_expr(expr, env)?;
                Ok(Flow::Normal)
            }
            Stmt::Import(_) => Err(not_implemented("import")),
            Stmt::Export(export) => self.exec_stmt(&export.item, env),
            Stmt::Class(_) => Err(not_implemented("class")),
            Stmt::Interface(_) => Err(not_implemented("interface")),
            Stmt::Enum(_) => Err(not_implemented("enum")),
            Stmt::TypeAlias(_) => Ok(Flow::Normal),
            Stmt::Command { text, line } => {
                self.exec_command(text, *line, env)?;
                Ok(Flow::Normal)
            }
        }
    }

    fn exec_try(&mut self, statement: &TryStmt, env: &Rc<RefCell<Env>>) -> Result<Flow> {
        let mut result = self.exec_items(&statement.body, env);
        if let Err(error) = &result {
            let exception = error.exception_string();
            for catch in &statement.catches {
                if self.catch_matches(catch, &exception)? {
                    self.runtime.set_vvar(
                        "exception",
                        Value::String(VimString::from(exception.clone())),
                    );
                    result = self.exec_items(&catch.body, env);
                    break;
                }
            }
        }
        if let Some(finally) = &statement.finally_body {
            let finish = self.exec_items(finally, env)?;
            if !matches!(finish, Flow::Normal) {
                return Ok(finish);
            }
        }
        result
    }

    fn catch_matches(&mut self, catch: &CatchClause, exception: &str) -> Result<bool> {
        let Some(pattern) = &catch.pattern else {
            return Ok(true);
        };
        let mut text = pattern.to_string_lossy().into_owned();
        if text.starts_with('/') && text.ends_with('/') && text.len() >= 2 {
            text = text[1..text.len() - 1].to_string();
        }
        let compiled = crate::pattern::compile(text.as_bytes())?;
        Ok(crate::pattern::find(&compiled, exception.as_bytes(), 0, false).is_some())
    }

    /// Bind a declaration/for-loop pattern to a value.
    fn bind_pattern(
        &mut self,
        pattern: &VarPattern,
        value: Value9,
        env: &Rc<RefCell<Env>>,
        declare: bool,
        locked: bool,
    ) -> Result<()> {
        match pattern {
            VarPattern::Name(name) => {
                let text = name.name.to_string_lossy().into_owned();
                match name.scope {
                    None => {
                        if declare {
                            Self::env_define(env, &text, value, locked);
                        } else {
                            Self::env_assign(env, &text, value)?;
                        }
                    }
                    Some(scope) => self.assign_scoped(scope, &text, value)?,
                }
                Ok(())
            }
            VarPattern::List { items, rest } => {
                let values = self.sequence_values(&value)?;
                for (index, item) in items.iter().enumerate() {
                    let item_value = values.get(index).cloned().unwrap_or_else(Value9::null);
                    self.bind_pattern(item, item_value, env, declare, locked)?;
                }
                if let Some(rest_pattern) = rest {
                    let remaining = if values.len() > items.len() {
                        values[items.len()..].to_vec()
                    } else {
                        Vec::new()
                    };
                    self.bind_pattern(rest_pattern, Value9::list(remaining), env, declare, locked)?;
                }
                Ok(())
            }
        }
    }

    /// Split a value into an ordered sequence (list, tuple, string, blob).
    fn sequence_values(&self, value: &Value9) -> Result<Vec<Value9>> {
        match value {
            Value9::Tuple(items) => Ok(items.clone()),
            Value9::Val(Value::List(list)) => Ok(list
                .borrow()
                .items
                .iter()
                .cloned()
                .map(Value9::Val)
                .collect()),
            Value9::Val(Value::String(string)) => Ok(string
                .as_bytes()
                .iter()
                .map(|byte| Value9::Val(Value::String(VimString::from_bytes(vec![*byte]))))
                .collect()),
            Value9::Val(Value::Blob(blob)) => Ok(blob
                .borrow()
                .iter()
                .map(|byte| Value9::number(i64::from(*byte)))
                .collect()),
            _ => Err(VimError::new(714, "E714: List required")),
        }
    }

    fn assign_scoped(
        &mut self,
        scope: crate::lexer::Scope,
        name: &str,
        value: Value9,
    ) -> Result<()> {
        match scope {
            crate::lexer::Scope::Global => {
                self.host.set_var(VarScope::Global, name, value.legacy())
            }
            crate::lexer::Scope::Buffer => {
                self.host.set_var(VarScope::Buffer, name, value.legacy())
            }
            crate::lexer::Scope::Window => {
                self.host.set_var(VarScope::Window, name, value.legacy())
            }
            crate::lexer::Scope::Tab => self.host.set_var(VarScope::Tab, name, value.legacy()),
            crate::lexer::Scope::Vim => {
                self.runtime.set_vvar(name, value.legacy());
                Ok(())
            }
            crate::lexer::Scope::Script
            | crate::lexer::Scope::Local
            | crate::lexer::Scope::Argument => {
                Self::env_assign(&self.script_env.clone(), name, value)
            }
        }
    }

    // ---------------------------------------------------------------
    // Expressions
    // ---------------------------------------------------------------

    fn eval_expr(&mut self, expression: &Expr, env: &Rc<RefCell<Env>>) -> Result<Value9> {
        match expression {
            Expr::Number(value) => Ok(Value9::number(*value)),
            Expr::Float(value) => Ok(Value9::Val(Value::Float(*value))),
            Expr::String(value) => Ok(Value9::Val(Value::String(value.clone()))),
            Expr::Blob(bytes) => Ok(Value9::Val(Value::blob(bytes.clone()))),
            Expr::Bool(value) => Ok(Value9::Val(Value::Bool(*value))),
            Expr::Special(_) => Ok(Value9::null()),
            Expr::Heredoc { lines, flags } => {
                let mut result = Vec::new();
                for line in lines {
                    let mut bytes = line.as_bytes().to_vec();
                    if flags.trim {
                        let mut start = 0;
                        while start < bytes.len() && matches!(bytes[start], b' ' | b'\t') {
                            start += 1;
                        }
                        bytes = bytes[start..].to_vec();
                    }
                    if flags.eval {
                        let text = self.interpolate_eval_line(&bytes, env)?;
                        result.push(Value::String(VimString::from_bytes(
                            text.as_bytes().to_vec(),
                        )));
                    } else {
                        result.push(Value::String(VimString::from_bytes(bytes)));
                    }
                }
                Ok(Value9::Val(Value::list(result)))
            }
            Expr::Interpolated(segments) => {
                let mut bytes = Vec::new();
                for segment in segments {
                    match segment {
                        InterpSegment::Text(text) => bytes.extend_from_slice(text.as_bytes()),
                        InterpSegment::Value(expression) => {
                            let value = self.eval_expr(expression, env)?;
                            bytes.extend_from_slice(value.display().as_bytes());
                        }
                    }
                }
                Ok(Value9::Val(Value::String(VimString::from_bytes(bytes))))
            }
            Expr::List(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.eval_expr(item, env)?.legacy());
                }
                Ok(Value9::Val(Value::list(values)))
            }
            Expr::Tuple(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.eval_expr(item, env)?);
                }
                Ok(Value9::Tuple(values))
            }
            Expr::Dict(entries) => {
                let mut dict = VimDict::new();
                for (key, value) in entries {
                    let key = match key {
                        DictKey::Literal(text) => text.clone(),
                        DictKey::String(text) => text.clone(),
                        DictKey::Expr(expression) => {
                            let evaluated = self.eval_expr(expression, env)?;
                            evaluated.to_key_string()?
                        }
                    };
                    let value = self.eval_expr(value, env)?;
                    dict.set(key, value.legacy());
                }
                Ok(Value9::Val(Value::dict(dict)))
            }
            Expr::Ident { scope, name } => self.lookup_ident(*scope, name, env),
            Expr::Option { scope, name } => {
                let scope = match scope {
                    None => HostOptionScope::Default,
                    Some(crate::lexer::OptionScope::Global) => HostOptionScope::Global,
                    Some(crate::lexer::OptionScope::Local) => HostOptionScope::Local,
                };
                Ok(Value9::Val(self.host.get_option(scope, name)?))
            }
            Expr::Env(name) => Ok(Value9::Val(self.host.get_env(name)?)),
            Expr::Register(register) => {
                Ok(Value9::Val(self.host.get_register(char::from(*register))?))
            }
            Expr::Unary { op, operand } => {
                let value = self.eval_expr(operand, env)?;
                match op {
                    UnaryOp::Not => Ok(Value9::Val(Value::Bool(!value.is_truthy()))),
                    UnaryOp::Negate => match &value {
                        Value9::Val(Value::Number(number)) => Ok(Value9::number(-number)),
                        Value9::Val(Value::Float(float)) => Ok(Value9::Val(Value::Float(-float))),
                        _ => Err(VimError::new(
                            1030,
                            "E1030: Using a special value as a Number",
                        )),
                    },
                    UnaryOp::Plus => Ok(value),
                }
            }
            Expr::Binary { op, left, right } => self.eval_binary_expr(*op, left, right, env),
            Expr::Ternary {
                cond,
                then_value,
                else_value,
            } => {
                let condition = self.eval_expr(cond, env)?;
                if condition.is_truthy() {
                    self.eval_expr(then_value, env)
                } else {
                    self.eval_expr(else_value, env)
                }
            }
            Expr::Call {
                callee,
                args,
                type_args: _,
            } => {
                let mut values = Vec::with_capacity(args.len());
                for argument in args {
                    values.push(self.eval_expr(argument, env)?);
                }
                match callee.as_ref() {
                    Expr::Ident { scope: None, name } => {
                        self.call_named(&name.to_string_lossy(), values, env)
                    }
                    _ => {
                        let callee = self.eval_expr(callee, env)?;
                        self.invoke_value(&callee, values, env)
                    }
                }
            }
            Expr::MethodCall {
                receiver,
                callee,
                args,
            } => {
                let receiver = self.eval_expr(receiver, env)?;
                let mut values = Vec::with_capacity(args.len() + 1);
                values.push(receiver);
                for argument in args {
                    values.push(self.eval_expr(argument, env)?);
                }
                match callee.as_ref() {
                    Expr::Ident { scope: None, name } => {
                        self.call_named_method(&name.to_string_lossy(), values, env)
                    }
                    _ => {
                        let callee = self.eval_expr(callee, env)?;
                        self.invoke_value(&callee, values, env)
                    }
                }
            }
            Expr::Index { base, index } => {
                let base = self.eval_expr(base, env)?;
                let index = self.eval_expr(index, env)?;
                self.index_value(&base, &index)
            }
            Expr::Slice { base, start, end } => {
                let base = self.eval_expr(base, env)?;
                let start = match start {
                    Some(expression) => Some(
                        self.eval_expr(expression, env)?
                            .as_number()
                            .ok_or_else(|| {
                                VimError::new(1023, "E1023: Using a special value as a Number")
                            })?,
                    ),
                    None => None,
                };
                let end = match end {
                    Some(expression) => Some(
                        self.eval_expr(expression, env)?
                            .as_number()
                            .ok_or_else(|| {
                                VimError::new(1023, "E1023: Using a special value as a Number")
                            })?,
                    ),
                    None => None,
                };
                self.slice_value(&base, start, end)
            }
            Expr::Member { base, key } => {
                let base = self.eval_expr(base, env)?;
                match &base {
                    Value9::Val(Value::Dict(dict)) => {
                        let borrowed = dict.borrow();
                        match borrowed.get(key) {
                            Some(value) => Ok(Value9::Val(value.clone())),
                            None => Err(VimError::new(
                                716,
                                format!(
                                    "E716: Key not present in Dictionary: {}",
                                    key.to_string_lossy()
                                ),
                            )),
                        }
                    }
                    _ => Err(VimError::new(
                        1030,
                        "E1030: Using an unsupported value as a Dictionary",
                    )),
                }
            }
            Expr::Lambda(lambda) => {
                let callable = Rc::new(Callable::Lambda(Rc::new(LambdaClosure {
                    params: lambda.params.clone(),
                    body: lambda.body.clone(),
                    env: env.clone(),
                })));
                Ok(self.make_funcref(callable))
            }
        }
    }

    fn lookup_ident(
        &mut self,
        scope: Option<crate::lexer::Scope>,
        name: &VimString,
        env: &Rc<RefCell<Env>>,
    ) -> Result<Value9> {
        let text = name.to_string_lossy().into_owned();
        match scope {
            None => {
                if let Some(value) = Self::env_lookup(env, &text) {
                    return Ok(value);
                }
                if let Some(callable) = Self::env_function(env, &text) {
                    return Ok(self.make_funcref(callable));
                }
                Err(VimError::new(
                    121,
                    format!("E121: Undefined variable: {text}"),
                ))
            }
            Some(crate::lexer::Scope::Global) => {
                Ok(Value9::Val(self.host.get_var(VarScope::Global, &text)?))
            }
            Some(crate::lexer::Scope::Buffer) => {
                Ok(Value9::Val(self.host.get_var(VarScope::Buffer, &text)?))
            }
            Some(crate::lexer::Scope::Window) => {
                Ok(Value9::Val(self.host.get_var(VarScope::Window, &text)?))
            }
            Some(crate::lexer::Scope::Tab) => {
                Ok(Value9::Val(self.host.get_var(VarScope::Tab, &text)?))
            }
            Some(crate::lexer::Scope::Vim) => self
                .runtime
                .get_vvar(&text)
                .map(Value9::Val)
                .ok_or_else(|| VimError::new(121, format!("E121: Undefined variable: v:{text}"))),
            Some(crate::lexer::Scope::Script) => {
                let script_env = self.script_env.clone();
                Self::env_lookup(&script_env, &text).ok_or_else(|| {
                    VimError::new(121, format!("E121: Undefined variable: s:{text}"))
                })
            }
            Some(crate::lexer::Scope::Local) | Some(crate::lexer::Scope::Argument) => {
                Self::env_lookup(env, &text)
                    .ok_or_else(|| VimError::new(121, format!("E121: Undefined variable: {text}")))
            }
        }
    }

    fn eval_binary_expr(
        &mut self,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
        env: &Rc<RefCell<Env>>,
    ) -> Result<Value9> {
        match op {
            BinaryOp::And => {
                let left = self.eval_expr(left, env)?;
                if !left.is_truthy() {
                    return Ok(Value9::Val(Value::Bool(false)));
                }
                let right = self.eval_expr(right, env)?;
                return Ok(Value9::Val(Value::Bool(right.is_truthy())));
            }
            BinaryOp::Or => {
                let left = self.eval_expr(left, env)?;
                if left.is_truthy() {
                    return Ok(Value9::Val(Value::Bool(true)));
                }
                let right = self.eval_expr(right, env)?;
                return Ok(Value9::Val(Value::Bool(right.is_truthy())));
            }
            _ => {}
        }
        let left = self.eval_expr(left, env)?;
        let right = self.eval_expr(right, env)?;
        self.binary_value(op, &left, &right)
    }

    fn binary_value(&mut self, op: BinaryOp, left: &Value9, right: &Value9) -> Result<Value9> {
        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                match (left, right) {
                    (Value9::Val(Value::Number(left)), Value9::Val(Value::Number(right))) => {
                        let value = match op {
                            BinaryOp::Add => left.wrapping_add(*right),
                            BinaryOp::Sub => left.wrapping_sub(*right),
                            BinaryOp::Mul => left.wrapping_mul(*right),
                            BinaryOp::Div => {
                                if *right == 0 {
                                    return Err(VimError::new(805, "E805: Division by zero"));
                                }
                                left / right
                            }
                            _ => {
                                if *right == 0 {
                                    return Err(VimError::new(805, "E805: Division by zero"));
                                }
                                left % right
                            }
                        };
                        Ok(Value9::number(value))
                    }
                    _ => {
                        let left = left.as_float().ok_or_else(|| {
                            VimError::new(1030, "E1030: Using a special value as a Number")
                        })?;
                        let right = right.as_float().ok_or_else(|| {
                            VimError::new(1030, "E1030: Using a special value as a Number")
                        })?;
                        let value = match op {
                            BinaryOp::Add => left + right,
                            BinaryOp::Sub => left - right,
                            BinaryOp::Mul => left * right,
                            BinaryOp::Div => {
                                if right == 0.0 {
                                    return Err(VimError::new(805, "E805: Division by zero"));
                                }
                                left / right
                            }
                            _ => {
                                if right == 0.0 {
                                    return Err(VimError::new(805, "E805: Division by zero"));
                                }
                                left % right
                            }
                        };
                        Ok(Value9::Val(Value::Float(value)))
                    }
                }
            }
            BinaryOp::Concat => {
                let mut bytes = left.display().into_bytes();
                bytes.extend_from_slice(right.display().as_bytes());
                Ok(Value9::Val(Value::String(VimString::from_bytes(bytes))))
            }
            BinaryOp::Eq => Ok(Value9::Val(Value::Bool(self.values_equal(left, right)))),
            BinaryOp::Ne => Ok(Value9::Val(Value::Bool(!self.values_equal(left, right)))),
            BinaryOp::EqIgnoreCase => Ok(Value9::Val(Value::Bool(
                self.values_equal_ignore_case(left, right),
            ))),
            BinaryOp::NeIgnoreCase => Ok(Value9::Val(Value::Bool(
                !self.values_equal_ignore_case(left, right),
            ))),
            BinaryOp::Gt | BinaryOp::Ge | BinaryOp::Lt | BinaryOp::Le => {
                let ordering = self.compare_values(left, right)?;
                let value = match op {
                    BinaryOp::Gt => ordering == std::cmp::Ordering::Greater,
                    BinaryOp::Ge => ordering != std::cmp::Ordering::Less,
                    BinaryOp::Lt => ordering == std::cmp::Ordering::Less,
                    _ => ordering != std::cmp::Ordering::Greater,
                };
                Ok(Value9::Val(Value::Bool(value)))
            }
            BinaryOp::Is => Ok(Value9::Val(Value::Bool(same_value(left, right)))),
            BinaryOp::IsNot => Ok(Value9::Val(Value::Bool(!same_value(left, right)))),
            BinaryOp::Match
            | BinaryOp::NotMatch
            | BinaryOp::MatchIgnoreCase
            | BinaryOp::NotMatchIgnoreCase => {
                let text = left.display();
                let pattern = right.display();
                let ignore_case =
                    matches!(op, BinaryOp::MatchIgnoreCase | BinaryOp::NotMatchIgnoreCase);
                let negate = matches!(op, BinaryOp::NotMatch | BinaryOp::NotMatchIgnoreCase);
                let compiled = crate::pattern::compile(pattern.as_bytes())?;
                let matched =
                    crate::pattern::find(&compiled, text.as_bytes(), 0, ignore_case).is_some();
                Ok(Value9::Val(Value::Bool(matched != negate)))
            }
            BinaryOp::In | BinaryOp::NotIn | BinaryOp::Coalesce | BinaryOp::And | BinaryOp::Or => {
                Err(VimError::new(1030, "E1030: Unsupported operator"))
            }
        }
    }

    fn values_equal_ignore_case(&self, left: &Value9, right: &Value9) -> bool {
        match (left, right) {
            (Value9::Val(Value::String(left)), Value9::Val(Value::String(right))) => {
                left.as_bytes().eq_ignore_ascii_case(right.as_bytes())
            }
            _ => self.values_equal(left, right),
        }
    }

    fn values_equal(&self, left: &Value9, right: &Value9) -> bool {
        match (left, right) {
            (Value9::Val(left), Value9::Val(right)) => left == right,
            (Value9::Tuple(left), Value9::Tuple(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right.iter())
                        .all(|(left, right)| self.values_equal(left, right))
            }
            (Value9::Tuple(left), Value9::Val(Value::List(right)))
            | (Value9::Val(Value::List(right)), Value9::Tuple(left)) => {
                let right = right.borrow();
                left.len() == right.items.len()
                    && left
                        .iter()
                        .zip(right.items.iter())
                        .all(|(left, right)| self.values_equal(left, &Value9::Val(right.clone())))
            }
            _ => false,
        }
    }

    fn compare_values(&self, left: &Value9, right: &Value9) -> Result<std::cmp::Ordering> {
        match (left, right) {
            (Value9::Val(Value::Number(left)), Value9::Val(Value::Number(right))) => {
                Ok(left.cmp(right))
            }
            (Value9::Val(Value::String(left)), Value9::Val(Value::String(right))) => {
                Ok(left.as_bytes().cmp(right.as_bytes()))
            }
            _ => {
                let left = left.as_float().ok_or_else(|| {
                    VimError::new(1030, "E1030: Using a special value as a Number")
                })?;
                let right = right.as_float().ok_or_else(|| {
                    VimError::new(1030, "E1030: Using a special value as a Number")
                })?;
                Ok(left
                    .partial_cmp(&right)
                    .unwrap_or(std::cmp::Ordering::Equal))
            }
        }
    }

    fn interpolate_eval_line(&mut self, line: &[u8], env: &Rc<RefCell<Env>>) -> Result<String> {
        // Reuse the lexer/parser for the interpolation: wrap the line in a
        // single-quoted interpolated string.
        let mut source = b"$'".to_vec();
        for byte in line {
            if *byte == b'\'' {
                source.extend_from_slice(b"''");
            } else {
                source.push(*byte);
            }
        }
        source.push(b'\'');
        let tokens = super::lexer::tokenize(&source)?;
        let mut out = String::new();
        for token in tokens {
            if let super::token::TokenKind::InterpolatedString(parts) = token.kind {
                for part in parts {
                    match part {
                        super::token::InterpPart::Text(text) => {
                            out.push_str(&String::from_utf8_lossy(text.as_bytes()));
                        }
                        super::token::InterpPart::Expr { source, line } => {
                            let expression =
                                parser::parse_expression_source(source.as_bytes(), line)?;
                            let value = self.eval_expr(&expression, env)?;
                            out.push_str(&value.display());
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    fn index_value(&mut self, base: &Value9, index: &Value9) -> Result<Value9> {
        match base {
            Value9::Val(Value::List(list)) => {
                let length = list.borrow().items.len() as i64;
                let position = normalize_index(index, length)?;
                Ok(Value9::Val(list.borrow().items[position].clone()))
            }
            Value9::Val(Value::String(string)) => {
                let length = string.as_bytes().len() as i64;
                let position = normalize_index(index, length)?;
                Ok(Value9::Val(Value::String(VimString::from_bytes(vec![
                    string.as_bytes()[position],
                ]))))
            }
            Value9::Val(Value::Blob(blob)) => {
                let length = blob.borrow().len() as i64;
                let position = normalize_index(index, length)?;
                Ok(Value9::number(i64::from(blob.borrow()[position])))
            }
            Value9::Val(Value::Dict(dict)) => {
                let key = index.to_key_string()?;
                match dict.borrow().get(&key) {
                    Some(value) => Ok(Value9::Val(value.clone())),
                    None => Err(VimError::new(
                        716,
                        format!(
                            "E716: Key not present in Dictionary: {}",
                            key.to_string_lossy()
                        ),
                    )),
                }
            }
            Value9::Tuple(items) => {
                let position = normalize_index(index, items.len() as i64)?;
                Ok(items[position].clone())
            }
            _ => Err(VimError::new(
                1030,
                "E1030: Using an unsupported value as a List",
            )),
        }
    }

    fn slice_value(
        &mut self,
        base: &Value9,
        start: Option<i64>,
        end: Option<i64>,
    ) -> Result<Value9> {
        let (start, end) = (start.unwrap_or(0), end);
        match base {
            Value9::Val(Value::List(list)) => {
                let items = list.borrow().items.clone();
                let (start, end) = slice_bounds(start, end, items.len());
                Ok(Value9::Val(Value::list(items[start..end].to_vec())))
            }
            Value9::Val(Value::String(string)) => {
                let bytes = string.as_bytes();
                let (start, end) = slice_bounds(start, end, bytes.len());
                Ok(Value9::Val(Value::String(VimString::from_bytes(
                    bytes[start..end].to_vec(),
                ))))
            }
            Value9::Val(Value::Blob(blob)) => {
                let bytes = blob.borrow().clone();
                let (start, end) = slice_bounds(start, end, bytes.len());
                Ok(Value9::Val(Value::blob(bytes[start..end].to_vec())))
            }
            Value9::Tuple(items) => {
                let (start, end) = slice_bounds(start, end, items.len());
                Ok(Value9::Tuple(items[start..end].to_vec()))
            }
            _ => Err(VimError::new(
                1030,
                "E1030: Using an unsupported value as a List",
            )),
        }
    }

    fn assign_target(
        &mut self,
        target: &Expr,
        value: Value9,
        env: &Rc<RefCell<Env>>,
    ) -> Result<()> {
        match target {
            Expr::Ident { scope, name } => {
                let text = name.to_string_lossy().into_owned();
                match scope {
                    None => Self::env_assign(env, &text, value),
                    Some(scope) => self.assign_scoped(*scope, &text, value),
                }
            }
            Expr::Index { base, index } => {
                let base = self.eval_expr(base, env)?;
                let index = self.eval_expr(index, env)?;
                match &base {
                    Value9::Val(Value::List(list)) => {
                        let length = list.borrow().items.len() as i64;
                        let position = normalize_index(&index, length)?;
                        list.borrow_mut().items[position] = value.legacy();
                        Ok(())
                    }
                    Value9::Val(Value::Dict(dict)) => {
                        let key = index.to_key_string()?;
                        dict.borrow_mut().set(key, value.legacy());
                        Ok(())
                    }
                    Value9::Val(Value::Blob(blob)) => {
                        let length = blob.borrow().len() as i64;
                        let position = normalize_index(&index, length)?;
                        let byte = value.as_number().ok_or_else(|| {
                            VimError::new(1023, "E1023: Using a special value as a Number")
                        })? as u8;
                        blob.borrow_mut()[position] = byte;
                        Ok(())
                    }
                    _ => Err(VimError::new(
                        1030,
                        "E1030: Using an unsupported value as a List",
                    )),
                }
            }
            Expr::Member { base, key } => {
                let base = self.eval_expr(base, env)?;
                match &base {
                    Value9::Val(Value::Dict(dict)) => {
                        dict.borrow_mut().set(key.clone(), value.legacy());
                        Ok(())
                    }
                    _ => Err(VimError::new(
                        1030,
                        "E1030: Using an unsupported value as a Dictionary",
                    )),
                }
            }
            Expr::Option { scope, name } => {
                let scope = match scope {
                    None => HostOptionScope::Default,
                    Some(crate::lexer::OptionScope::Global) => HostOptionScope::Global,
                    Some(crate::lexer::OptionScope::Local) => HostOptionScope::Local,
                };
                self.host.set_option(scope, name, value.legacy())
            }
            Expr::Env(name) => {
                let text = value.as_string().ok_or_else(|| {
                    VimError::new(1023, "E1023: Using a special value as a String")
                })?;
                self.host.set_env(name, &text)
            }
            Expr::Register(register) => self
                .host
                .set_register(char::from(*register), value.legacy()),
            _ => Err(VimError::new(488, "E488: Invalid assignment target")),
        }
    }

    fn call_named(
        &mut self,
        name: &str,
        args: Vec<Value9>,
        env: &Rc<RefCell<Env>>,
    ) -> Result<Value9> {
        if let Some(value) = Self::env_lookup(env, name) {
            if matches!(value.as_legacy(), Some(Value::Funcref(_))) {
                return self.invoke_value(&value, args, env);
            }
        }
        if let Some(callable) = Self::env_function(env, name) {
            return self.call_callable(&callable, args);
        }
        self.call_builtin(name, args, env)
    }

    fn call_named_method(
        &mut self,
        name: &str,
        args: Vec<Value9>,
        env: &Rc<RefCell<Env>>,
    ) -> Result<Value9> {
        if let Some(value) = Self::env_lookup(env, name) {
            if matches!(value.as_legacy(), Some(Value::Funcref(_))) {
                return self.invoke_value(&value, args, env);
            }
        }
        if let Some(callable) = Self::env_function(env, name) {
            return self.call_callable(&callable, args);
        }
        self.call_builtin(name, args, env)
    }

    // ---------------------------------------------------------------
    // Builtins
    // ---------------------------------------------------------------

    fn is_callable(value: &Value9) -> bool {
        matches!(value, Value9::Val(Value::Funcref(_)))
    }

    fn call_builtin(
        &mut self,
        name: &str,
        args: Vec<Value9>,
        env: &Rc<RefCell<Env>>,
    ) -> Result<Value9> {
        match name {
            "map" | "mapnew" | "filter" | "reduce" | "foreach" | "any" | "all" => {
                if args.len() == 2 && Self::is_callable(&args[1]) {
                    return self.higher_order(name, &args, env);
                }
            }
            "sort" => {
                if args.len() >= 2 && Self::is_callable(&args[1]) {
                    return self.sort_with_comparator(&args, env);
                }
            }
            "string" if args.len() == 1 => {
                if let Some(text) = special_string(&args[0])? {
                    return Ok(Value9::from_string(text));
                }
            }
            "strtrans" if args.len() == 1 => {
                return Ok(Value9::from_string(strtrans_text(&args[0].display())));
            }
            "trim" if !args.is_empty() => {
                let mask = args
                    .get(1)
                    .map(|value| value.display())
                    .unwrap_or_else(|| " \t\r\n\x0b\x0c".to_string());
                return Ok(Value9::from_string(trim_text(&args[0].display(), &mask)));
            }
            "len" if args.len() == 1 => {
                if let Value9::Tuple(items) = &args[0] {
                    return Ok(Value9::number(items.len() as i64));
                }
            }
            "type" if args.len() == 1 => {
                if matches!(args[0], Value9::Tuple(_)) {
                    return Ok(Value9::number(17));
                }
            }
            _ => {}
        }
        self.call_builtin_legacy(name, args)
    }

    fn call_builtin_legacy(&mut self, name: &str, args: Vec<Value9>) -> Result<Value9> {
        let legacy: Vec<Value> = args.iter().map(Value9::legacy).collect();
        let value = crate::eval::call_function(name, legacy, &mut self.runtime, &mut self.host)?;
        Ok(Value9::Val(value))
    }

    fn higher_order(
        &mut self,
        name: &str,
        args: &[Value9],
        env: &Rc<RefCell<Env>>,
    ) -> Result<Value9> {
        let callable = &args[1];
        match name {
            "map" | "mapnew" => {
                let items = self.sequence_values(&args[0])?;
                let mut mapped = Vec::with_capacity(items.len());
                for (index, item) in items.into_iter().enumerate() {
                    let value =
                        self.invoke_value(callable, vec![Value9::number(index as i64), item], env)?;
                    mapped.push(value);
                }
                if name == "mapnew" {
                    return Ok(Value9::list(mapped));
                }
                match &args[0] {
                    Value9::Val(Value::List(list)) => {
                        let legacy: Vec<Value> = mapped.iter().map(Value9::legacy).collect();
                        list.borrow_mut().items = legacy;
                        Ok(args[0].clone())
                    }
                    Value9::Tuple(_) => Ok(Value9::Tuple(mapped)),
                    _ => Ok(args[0].clone()),
                }
            }
            "filter" => {
                let items = self.sequence_values(&args[0])?;
                let mut kept = Vec::new();
                for (index, item) in items.into_iter().enumerate() {
                    let keep = self.invoke_value(
                        callable,
                        vec![Value9::number(index as i64), item.clone()],
                        env,
                    )?;
                    if keep.is_truthy() {
                        kept.push(item);
                    }
                }
                match &args[0] {
                    Value9::Val(Value::List(list)) => {
                        let legacy: Vec<Value> = kept.iter().map(Value9::legacy).collect();
                        list.borrow_mut().items = legacy;
                        Ok(args[0].clone())
                    }
                    _ => Ok(Value9::list(kept)),
                }
            }
            "reduce" => {
                let items = self.sequence_values(&args[0])?;
                let mut iterator = items.into_iter();
                let mut accumulator = if let Some(initial) = args.get(2) {
                    initial.clone()
                } else {
                    match iterator.next() {
                        Some(first) => first,
                        None => return Ok(Value9::number(0)),
                    }
                };
                for item in iterator {
                    accumulator = self.invoke_value(callable, vec![accumulator, item], env)?;
                }
                Ok(accumulator)
            }
            "foreach" => {
                let items = self.sequence_values(&args[0])?;
                for (index, item) in items.into_iter().enumerate() {
                    self.invoke_value(callable, vec![Value9::number(index as i64), item], env)?;
                }
                Ok(args[0].clone())
            }
            "any" | "all" => {
                let items = self.sequence_values(&args[0])?;
                let mut result = name == "all";
                for (index, item) in items.into_iter().enumerate() {
                    let value =
                        self.invoke_value(callable, vec![Value9::number(index as i64), item], env)?;
                    if name == "any" && value.is_truthy() {
                        result = true;
                        break;
                    }
                    if name == "all" && !value.is_truthy() {
                        result = false;
                        break;
                    }
                }
                Ok(Value9::Val(Value::Bool(result)))
            }
            _ => self.call_builtin_legacy(name, args.to_vec()),
        }
    }

    fn sort_with_comparator(&mut self, args: &[Value9], env: &Rc<RefCell<Env>>) -> Result<Value9> {
        let callable = &args[1];
        let mut items = self.sequence_values(&args[0])?;
        let mut index = 1;
        while index < items.len() {
            let mut position = index;
            while position > 0 {
                let order = self.invoke_value(
                    callable,
                    vec![items[position - 1].clone(), items[position].clone()],
                    env,
                )?;
                let order = order.as_number().ok_or_else(|| {
                    VimError::new(1023, "E1023: Using a special value as a Number")
                })?;
                if order <= 0 {
                    break;
                }
                items.swap(position - 1, position);
                position -= 1;
            }
            index += 1;
        }
        match &args[0] {
            Value9::Val(Value::List(list)) => {
                let legacy: Vec<Value> = items.iter().map(Value9::legacy).collect();
                list.borrow_mut().items = legacy;
                Ok(args[0].clone())
            }
            _ => Ok(Value9::list(items)),
        }
    }

    // ---------------------------------------------------------------
    // Commands
    // ---------------------------------------------------------------

    fn exec_command(
        &mut self,
        text: &VimString,
        line: u32,
        env: &Rc<RefCell<Env>>,
    ) -> Result<Option<Flow>> {
        let mut text = String::from_utf8_lossy(text.as_bytes()).trim().to_string();
        while let Some(rest) = text.strip_prefix(':') {
            text = rest.trim().to_string();
        }
        if text.is_empty() {
            return Ok(None);
        }
        let mut rest: &str = &text;
        loop {
            if let Some(stripped) = rest.strip_prefix("silent!") {
                rest = stripped.trim_start();
                continue;
            }
            if let Some(stripped) = rest.strip_prefix("silent") {
                rest = stripped.trim_start();
                continue;
            }
            if let Some(stripped) = rest.strip_prefix("verbose") {
                rest = stripped.trim_start();
                continue;
            }
            break;
        }
        let (name, arguments) = split_command(rest);
        match name {
            "echomsg" | "echo" => {
                let value = self.eval_command_expression(arguments, line, env)?;
                self.messages.push(value.display());
                Ok(None)
            }
            "execute" => {
                let value = self.eval_command_expression(arguments, line, env)?;
                let command = value.display();
                for single in command.split('\n') {
                    if single.trim().is_empty() {
                        continue;
                    }
                    self.exec_command(&VimString::from(single.trim().to_string()), line, env)?;
                }
                Ok(None)
            }
            "set" => {
                self.command_set(arguments);
                Ok(None)
            }
            "normal" | "normal!" => {
                self.command_normal(arguments);
                Ok(None)
            }
            "call" => {
                let _ = self.eval_command_expression(arguments, line, env)?;
                Ok(None)
            }
            "finish" => Ok(Some(Flow::Finish)),
            "scriptversion" | "defcompile" | "disassemble" => Ok(None),
            _ => Err(VimError::new(
                492,
                format!("E492: Not an editor command: {name}"),
            )),
        }
    }

    fn eval_command_expression(
        &mut self,
        text: &str,
        line: u32,
        env: &Rc<RefCell<Env>>,
    ) -> Result<Value9> {
        let expression = parser::parse_expression_source(text.as_bytes(), line)?;
        self.eval_expr(&expression, env)
    }

    fn command_set(&mut self, arguments: &str) {
        for token in arguments.split_whitespace() {
            if let Some((name, value)) = token.split_once('=') {
                let value = if let Ok(number) = value.parse::<i64>() {
                    Value::Number(number)
                } else if let Ok(float) = value.parse::<f64>() {
                    Value::Float(float)
                } else {
                    Value::String(VimString::from(value))
                };
                self.host
                    .options
                    .insert((HostOptionScope::Default, name.to_string()), value);
            } else if let Some(name) = token.strip_prefix("no") {
                self.host.options.insert(
                    (HostOptionScope::Default, name.to_string()),
                    Value::Bool(false),
                );
            } else {
                self.host.options.insert(
                    (HostOptionScope::Default, token.to_string()),
                    Value::Bool(true),
                );
            }
        }
    }

    fn command_normal(&mut self, keys: &str) {
        if self.host.lines.is_empty() {
            self.host.lines.push(VimString::new());
        }
        let bytes = keys.as_bytes();
        let mut index = 0usize;
        while index < bytes.len() {
            match bytes[index] {
                b'i' | b'a' | b'A' | b'o' | b'O' => {
                    self.normal_insert(bytes[index], &bytes[index + 1..]);
                    return;
                }
                b'g' if bytes.get(index + 1) == Some(&b'g') => {
                    self.host.current_line = 1;
                    self.cursor_col = 0;
                    index += 2;
                }
                b'G' => {
                    self.host.current_line = self.host.lines.len().max(1) as i64;
                    self.cursor_col = 0;
                    index += 1;
                }
                _ => index += 1,
            }
        }
    }

    fn normal_insert(&mut self, command: u8, text: &[u8]) {
        let line_index = (self.host.current_line.max(1) - 1) as usize;
        while self.host.lines.len() <= line_index {
            self.host.lines.push(VimString::new());
        }
        match command {
            b'i' => {
                let mut line = self.host.lines[line_index].as_bytes().to_vec();
                let position = self.cursor_col.min(line.len());
                line.splice(position..position, text.iter().copied());
                self.host.lines[line_index] = VimString::from_bytes(line);
                self.cursor_col = position + text.len();
            }
            b'a' => {
                let mut line = self.host.lines[line_index].as_bytes().to_vec();
                let position = (self.cursor_col + 1).min(line.len());
                line.splice(position..position, text.iter().copied());
                self.host.lines[line_index] = VimString::from_bytes(line);
                self.cursor_col = position + text.len();
            }
            b'A' => {
                let mut line = self.host.lines[line_index].as_bytes().to_vec();
                line.extend_from_slice(text);
                self.host.lines[line_index] = VimString::from_bytes(line);
                self.cursor_col = self.host.lines[line_index].as_bytes().len();
            }
            b'o' | b'O' => {
                let position = if command == b'o' {
                    line_index + 1
                } else {
                    line_index
                };
                self.host
                    .lines
                    .insert(position, VimString::from_bytes(text.to_vec()));
                self.host.current_line = position as i64 + 1;
                self.cursor_col = text.len();
            }
            _ => {}
        }
    }
}
