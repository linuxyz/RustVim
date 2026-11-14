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
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rvim_types::{ScriptId, VimString};

use crate::error::{Result, VimError};
use crate::host::{BasicHost, OptionScope as HostOptionScope, VarScope, VimHost};
use crate::runtime::VimRuntime;
use crate::value::{FuncrefData, FuncrefKind, Value, VimDict};

use super::ast::*;
use super::eval_helpers::{
    assign_to_binary, normalize_index, same_value, slice_bounds, special_string, split_command,
    strtrans_text, trim_text,
};
use super::parser;
use super::value9::{
    funcref_id, funcref_name, marker_id, prefixed_marker, Value9, CLASS_PREFIX, ENUM_PREFIX,
    ENUM_VALUE_PREFIX, OBJECT_PREFIX,
};

/// Result of running a complete Vim9 script.
pub struct RunOutput {
    /// Strings produced by :echo/:echomsg, in order.
    pub messages: Vec<String>,
}

/// Services the Vim9 evaluator needs from the embedding editor.
///
/// The standalone runner uses [BasicHost]; the editor implements this trait
/// for its own host so :syntax/:set/:normal/:autocmd and other Ex commands
/// run against the real editor state.
pub trait Vim9Host: VimHost {
    /// Execute an Ex command line from Vim9 code.  The editor temporarily
    /// installs the interpreter runtime so both engines share globals.
    fn execute_ex_command(&mut self, command: &str, _runtime: &mut VimRuntime) -> Result<Value> {
        Err(VimError::new(
            492,
            format!("E492: Not an editor command: {command}"),
        ))
    }

    /// Whether ":set"/":normal" should be handled by the editor rather than
    /// the minimal implementations used by the standalone runner.
    fn external_command(&mut self, _name: &str) -> bool {
        false
    }

    /// Display :echo/:echomsg text.  The standalone runner records messages in
    /// [RunOutput] instead.
    fn echo(&mut self, _text: &str) {}

    /// Run a legacy Vim script chunk (the "legacy" command prefix).
    fn execute_legacy(&mut self, _source: &[u8], _runtime: &mut VimRuntime) -> Result<()> {
        Err(VimError::new(
            319,
            "legacy execution is not supported by this host",
        ))
    }

    /// Handle :normal for the minimal standalone host.
    fn run_normal(&mut self, _keys: &str, _cursor: &mut usize) -> Result<()> {
        Err(VimError::new(319, "normal is not supported by this host"))
    }

    /// Extra directories for :import (the import/ or autoload/ trees).
    fn import_roots(&self, _autoload: bool) -> Vec<PathBuf> {
        Vec::new()
    }
}

impl Vim9Host for BasicHost {
    fn run_normal(&mut self, keys: &str, cursor: &mut usize) -> Result<()> {
        basic_normal(self, keys, cursor);
        Ok(())
    }
}

fn standalone_runtime() -> VimRuntime {
    let mut runtime = VimRuntime::new();
    runtime.set_vvar("exception", Value::String(VimString::new()));
    runtime.set_vvar("stacktrace", Value::list(Vec::new()));
    runtime.set_vvar("errmsg", Value::String(VimString::new()));
    runtime
}

fn standalone_host() -> BasicHost {
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
    host
}

/// Parse and execute a complete Vim9 script with a standalone host.
pub fn run_script(source: &[u8]) -> Result<RunOutput> {
    let script = parser::parse_script(source)?;
    let mut runtime = standalone_runtime();
    let mut host = standalone_host();
    let messages = {
        let mut interpreter = Interpreter::new(&mut runtime, &mut host);
        interpreter.execute_script(&script)?;
        interpreter.messages
    };
    Ok(RunOutput { messages })
}

/// Execute a parsed-or-raw Vim9 script with an editor-provided host/runtime.
pub fn run_script_in_host(
    source: &[u8],
    path: Option<&Path>,
    runtime: &mut VimRuntime,
    host: &mut dyn Vim9Host,
) -> Result<()> {
    let script = parser::parse_script(source)?;
    let dir = path
        .and_then(|path| path.parent())
        .map(|parent| parent.to_path_buf());
    let mut interpreter = Interpreter::new(runtime, host);
    interpreter.execute_script_at(&script, dir)
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
    /// Class and enum names declared in this scope.
    types: HashMap<String, TypeBinding>,
    /// Interface names (compile-time only; runtime dispatch is by object).
    interfaces: HashSet<String>,
    parent: Option<Rc<RefCell<Env>>>,
}

/// A class or enum visible by name in a script scope.
#[derive(Clone)]
enum TypeBinding {
    Class(Rc<ClassDef>),
    Enum(Rc<EnumDef>),
}

/// A callable: a compiled def or a lambda with a captured environment.
enum Callable {
    Function {
        def: Rc<DefDecl>,
        env: Rc<RefCell<Env>>,
    },
    Lambda(Rc<LambdaClosure>),
    /// A method bound to an object (instance) or class (static).
    Bound {
        def: Rc<DefDecl>,
        env: Rc<RefCell<Env>>,
        this_value: Option<Value9>,
    },
    /// A class constructor.
    Constructor {
        class: Rc<ClassDef>,
    },
}

struct LambdaClosure {
    params: Vec<Param>,
    body: LambdaBody,
    env: Rc<RefCell<Env>>,
}

/// One script execution frame: its script-local scope and export table.
struct ScriptFrame {
    env: Rc<RefCell<Env>>,
    exports: VimDict,
    /// Directory used to resolve relative imports.
    dir: Option<PathBuf>,
}

/// One variable member of a class or enum.
#[derive(Clone)]
struct VariableDef {
    name: String,
    ty: Option<TypeExpr>,
    init: Option<Expr>,
}

/// A Vim9 class (runtime side).
struct ClassDef {
    name: String,
    is_abstract: bool,
    base: Option<Rc<ClassDef>>,
    instance_vars: Vec<VariableDef>,
    static_vars: RefCell<HashMap<String, Value>>,
    methods: HashMap<String, Rc<DefDecl>>,
    static_methods: HashMap<String, Rc<DefDecl>>,
    /// Scope the class was declared in (methods resolve script names here).
    env: Rc<RefCell<Env>>,
    id: usize,
}

/// A class instance.
struct Object {
    class: Rc<ClassDef>,
    fields: RefCell<HashMap<String, Value>>,
}

/// A Vim9 enum.
struct EnumDef {
    name: String,
    members: HashMap<String, usize>,
    id: usize,
}

/// One enum member value (also used for enum constructors with fields).
struct EnumValue {
    name: String,
    ordinal: i64,
    fields: RefCell<HashMap<String, Value>>,
}

struct Interpreter<'h> {
    /// Legacy runtime, used for v: variables and builtin delegation.
    runtime: &'h mut VimRuntime,
    /// Host: editor services, options, registers and buffer lines.
    host: &'h mut dyn Vim9Host,
    messages: Vec<String>,
    frames: Vec<ScriptFrame>,
    callables: Vec<Rc<Callable>>,
    classes: Vec<Rc<ClassDef>>,
    objects: Vec<Rc<RefCell<Object>>>,
    enums: Vec<Rc<EnumDef>>,
    enum_values: Vec<Rc<EnumValue>>,
    /// Import stack, to skip cyclic imports like Vim does.
    loading: Vec<PathBuf>,
    /// Script environments of Vim9 autoload files, keyed by their prefix.
    autoload_scripts: HashMap<String, Rc<RefCell<Env>>>,
    /// Autoload prefixes currently being loaded (recursion guard).
    loading_autoload: Vec<String>,
    cursor_col: usize,
}

impl<'h> Interpreter<'h> {
    fn new(runtime: &'h mut VimRuntime, host: &'h mut dyn Vim9Host) -> Self {
        Interpreter {
            runtime,
            host,
            messages: Vec::new(),
            frames: vec![ScriptFrame {
                env: Rc::new(RefCell::new(Env::default())),
                exports: VimDict::new(),
                dir: None,
            }],
            callables: Vec::new(),
            classes: Vec::new(),
            objects: Vec::new(),
            enums: Vec::new(),
            enum_values: Vec::new(),
            loading: Vec::new(),
            autoload_scripts: HashMap::new(),
            loading_autoload: Vec::new(),
            cursor_col: 0,
        }
    }

    fn current_env(&self) -> Rc<RefCell<Env>> {
        self.frames
            .last()
            .expect("at least one script frame")
            .env
            .clone()
    }

    fn current_dir(&self) -> Option<PathBuf> {
        self.frames.last().and_then(|frame| frame.dir.clone())
    }

    fn env_type(env: &Rc<RefCell<Env>>, name: &str) -> Option<TypeBinding> {
        let mut current = Some(env.clone());
        while let Some(scope) = current {
            let borrowed = scope.borrow();
            if let Some(binding) = borrowed.types.get(name) {
                return Some(binding.clone());
            }
            current = borrowed.parent.clone();
        }
        None
    }

    fn execute_script(&mut self, script: &Script) -> Result<()> {
        self.execute_script_at(script, None)
    }

    fn execute_script_at(&mut self, script: &Script, dir: Option<PathBuf>) -> Result<()> {
        let env = self.current_env();
        if let Some(frame) = self.frames.last_mut() {
            frame.exports = VimDict::new();
            if dir.is_some() {
                frame.dir = dir;
            }
        }
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
            types: HashMap::new(),
            interfaces: HashSet::new(),
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
            Callable::Bound {
                def,
                env,
                this_value,
            } => {
                let frame = Self::frame(env);
                if let Some(this_value) = this_value {
                    Self::env_define(&frame, "this", this_value.clone(), false);
                }
                self.bind_params(&def.params, args, &frame, &def.name.to_string_lossy())?;
                let flow = self.exec_items(&def.body, &frame)?;
                Ok(match flow {
                    Flow::Return(value) => value,
                    _ => Value9::none(),
                })
            }
            Callable::Constructor { class } => self.construct_object(class, args),
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
                    None => self.var_default(declaration),
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
            Stmt::Import(import) => {
                self.exec_import(import, env)?;
                Ok(Flow::Normal)
            }
            Stmt::Export(export) => {
                let flow = self.exec_stmt(&export.item, env)?;
                if matches!(flow, Flow::Normal) {
                    self.register_export(&export.item, env)?;
                }
                Ok(flow)
            }
            Stmt::Class(class) => {
                self.define_class(class, env)?;
                Ok(Flow::Normal)
            }
            Stmt::Interface(interface) => {
                let name = interface.name.to_string_lossy().into_owned();
                env.borrow_mut().interfaces.insert(name);
                Ok(Flow::Normal)
            }
            Stmt::Enum(value) => {
                self.define_enum(value, env)?;
                Ok(Flow::Normal)
            }
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
                self.runtime.set_global(name, value.legacy());
                Ok(())
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
                let env = self.current_env();
                Self::env_assign(&env, name, value)
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
                Ok(Value9::Val(self.host.get_option(scope, option_name(name))?))
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
                self.member_get(&base, key)
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
                if let Some(binding) = Self::env_type(env, &text) {
                    return Ok(self.type_value(&binding));
                }
                Err(VimError::new(
                    121,
                    format!("E121: Undefined variable: {text}"),
                ))
            }
            Some(crate::lexer::Scope::Global) => {
                if text.is_empty() {
                    return Ok(Value9::Val(Value::dict(
                        self.runtime.globals().borrow().clone(),
                    )));
                }
                if let Some(value) = self.runtime.get_global(&text) {
                    return Ok(Value9::Val(value));
                }
                Ok(Value9::Val(self.host.get_var(VarScope::Global, &text)?))
            }
            Some(crate::lexer::Scope::Buffer) => {
                if text.is_empty() {
                    return Ok(Value9::Val(self.host.get_scope_dict(VarScope::Buffer)?));
                }
                Ok(Value9::Val(self.host.get_var(VarScope::Buffer, &text)?))
            }
            Some(crate::lexer::Scope::Window) => {
                if text.is_empty() {
                    return Ok(Value9::Val(self.host.get_scope_dict(VarScope::Window)?));
                }
                Ok(Value9::Val(self.host.get_var(VarScope::Window, &text)?))
            }
            Some(crate::lexer::Scope::Tab) => {
                if text.is_empty() {
                    return Ok(Value9::Val(self.host.get_scope_dict(VarScope::Tab)?));
                }
                Ok(Value9::Val(self.host.get_var(VarScope::Tab, &text)?))
            }
            Some(crate::lexer::Scope::Vim) => {
                if text.is_empty() {
                    return Ok(Value9::Val(Value::dict(self.runtime.vvar_dict().clone())));
                }
                self.runtime
                    .get_vvar(&text)
                    .map(Value9::Val)
                    .ok_or_else(|| {
                        VimError::new(121, format!("E121: Undefined variable: v:{text}"))
                    })
            }
            Some(crate::lexer::Scope::Script) => {
                let script_env = self.current_env();
                if text.is_empty() {
                    return Ok(Value9::Val(Value::dict(env_dict(&script_env))));
                }
                Self::env_lookup(&script_env, &text).ok_or_else(|| {
                    VimError::new(121, format!("E121: Undefined variable: s:{text}"))
                })
            }
            Some(crate::lexer::Scope::Local) | Some(crate::lexer::Scope::Argument) => {
                if text.is_empty() {
                    return Ok(Value9::Val(Value::dict(env_dict(env))));
                }
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
                    (Value9::Val(Value::List(left)), Value9::Val(Value::List(right)))
                        if op == BinaryOp::Add =>
                    {
                        let mut items = left.borrow().items.clone();
                        items.extend(right.borrow().items.iter().cloned());
                        Ok(Value9::Val(Value::list(items)))
                    }
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
                self.member_set(&base, key, value)
            }
            Expr::Option { scope, name } => {
                let scope = match scope {
                    None => HostOptionScope::Default,
                    Some(crate::lexer::OptionScope::Global) => HostOptionScope::Global,
                    Some(crate::lexer::OptionScope::Local) => HostOptionScope::Local,
                };
                self.host
                    .set_option(scope, option_name(name), value.legacy())
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
        if name.contains('#') {
            if let Some(value) = self.call_autoload(name, args.clone(), env)? {
                return Ok(value);
            }
        }
        self.call_builtin(name, args, env)
    }

    /// Load and call a Vim9 autoload function such as tohtml#GetUserSettings.
    /// Legacy autoload files are left to the legacy engine.
    fn call_autoload(
        &mut self,
        name: &str,
        args: Vec<Value9>,
        _env: &Rc<RefCell<Env>>,
    ) -> Result<Option<Value9>> {
        let Some((prefix, function)) = name.rsplit_once('#') else {
            return Ok(None);
        };
        if self.loading_autoload.iter().any(|item| item == prefix) {
            return Ok(None);
        }
        let frame_env = match self.autoload_scripts.get(prefix).cloned() {
            Some(env) => env,
            None => {
                let relative = format!("{}.vim", prefix.replace('#', "/"));
                let Some(path) = self.find_autoload(&relative) else {
                    return Ok(None);
                };
                let source = std::fs::read(&path).map_err(|error| {
                    VimError::new(
                        484,
                        format!("E484: Can't open file {}: {error}", path.display()),
                    )
                })?;
                if parser::find_vim9_header(&source).is_none() {
                    return Ok(None);
                }
                let script = parser::parse_script(&source)?;
                let env = Rc::new(RefCell::new(Env::default()));
                self.frames.push(ScriptFrame {
                    env: env.clone(),
                    exports: VimDict::new(),
                    dir: path.parent().map(|parent| parent.to_path_buf()),
                });
                self.loading_autoload.push(prefix.to_string());
                let result = self.exec_items(&script.items, &env);
                self.loading_autoload.pop();
                let frame = self.frames.pop().expect("autoload frame");
                result?;
                let env = frame.env;
                self.autoload_scripts
                    .insert(prefix.to_string(), env.clone());
                env
            }
        };
        let Some(callable) = Self::env_function(&frame_env, function) else {
            return Ok(None);
        };
        let value = self.call_callable(&callable, args)?;
        Ok(Some(value))
    }

    fn find_autoload(&self, relative: &str) -> Option<PathBuf> {
        for root in self.host.import_roots(true) {
            let candidate = root.join(relative);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        if let Ok(runtime) = std::env::var("VIMRUNTIME") {
            let candidate = Path::new(&runtime).join("autoload").join(relative);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
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
        let value = crate::eval::call_function(name, legacy, self.runtime, self.host)?;
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
            "echomsg" | "echom" | "echo" | "echon" | "echoe" => {
                let values = self.eval_command_expressions(arguments, line, env)?;
                let text = values
                    .iter()
                    .map(Value9::display)
                    .collect::<Vec<_>>()
                    .join(" ");
                self.messages.push(text.clone());
                self.host.echo(&text);
                Ok(None)
            }
            "execute" | "exe" => {
                let values = self.eval_command_expressions(arguments, line, env)?;
                let command = values
                    .iter()
                    .map(Value9::display)
                    .collect::<Vec<_>>()
                    .join(" ");
                let trimmed = command.trim();
                if trimmed.is_empty() {
                    return Ok(None);
                }
                // :execute body may contain multi-line definitions (def/enddef)
                // and other script commands, so parse it like a script body and
                // fall back to line-by-line command execution if that fails.
                match parser::parse_script(trimmed.as_bytes()) {
                    Ok(script) => {
                        let flow = self.exec_items(&script.items, env)?;
                        if matches!(flow, Flow::Finish) {
                            Ok(Some(Flow::Finish))
                        } else {
                            Ok(None)
                        }
                    }
                    Err(_) => {
                        for single in command.split('\n') {
                            if single.trim().is_empty() {
                                continue;
                            }
                            self.exec_command(
                                &VimString::from(single.trim().to_string()),
                                line,
                                env,
                            )?;
                        }
                        Ok(None)
                    }
                }
            }
            "set" => {
                if self.host.external_command("set") {
                    self.host.execute_ex_command(rest, self.runtime)?;
                } else {
                    self.command_set(arguments)?;
                }
                Ok(None)
            }
            "normal" | "normal!" => {
                if self.host.external_command(name) {
                    self.host.execute_ex_command(rest, self.runtime)?;
                } else {
                    self.host.run_normal(arguments, &mut self.cursor_col)?;
                }
                Ok(None)
            }
            "call" => {
                let _ = self.eval_command_expression(arguments, line, env)?;
                Ok(None)
            }
            "finish" => Ok(Some(Flow::Finish)),
            "legacy" => {
                self.host
                    .execute_legacy(arguments.as_bytes(), self.runtime)?;
                Ok(None)
            }
            "scriptversion" | "defcompile" | "disassemble" => Ok(None),
            _ => {
                let unstripped = rest.trim_end();
                let stripped = strip_vim9_comment(rest);
                let first = self.host.execute_ex_command(unstripped, self.runtime);
                if first.is_err() && stripped != unstripped {
                    self.host.execute_ex_command(&stripped, self.runtime)?;
                } else {
                    first?;
                }
                Ok(None)
            }
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

    fn eval_command_expressions(
        &mut self,
        text: &str,
        line: u32,
        env: &Rc<RefCell<Env>>,
    ) -> Result<Vec<Value9>> {
        let expressions = parser::parse_expressions(text.as_bytes(), line)?;
        let mut values = Vec::with_capacity(expressions.len());
        for expression in &expressions {
            values.push(self.eval_expr(expression, env)?);
        }
        Ok(values)
    }

    fn command_set(&mut self, arguments: &str) -> Result<()> {
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
                    .set_option(HostOptionScope::Default, option_name(name), value)?;
            } else if let Some(name) = token.strip_prefix("no") {
                self.host.set_option(
                    HostOptionScope::Default,
                    option_name(name),
                    Value::Bool(false),
                )?;
            } else {
                self.host.set_option(
                    HostOptionScope::Default,
                    option_name(token),
                    Value::Bool(true),
                )?;
            }
        }
        Ok(())
    }

    // ---------------------------------------------------------------
    // Classes, enums, imports (Phase 3/4)
    // ---------------------------------------------------------------

    fn var_default(&self, declaration: &VarDecl) -> Value9 {
        let ty = declaration.ty.as_ref().or(match &declaration.pattern {
            VarPattern::Name(name) => name.ty.as_ref(),
            _ => None,
        });
        default_for_type(ty)
    }

    fn type_value(&mut self, binding: &TypeBinding) -> Value9 {
        match binding {
            TypeBinding::Class(class) => prefixed_marker(CLASS_PREFIX, class.id),
            TypeBinding::Enum(value) => prefixed_marker(ENUM_PREFIX, value.id),
        }
    }

    fn find_class_method(class: &Rc<ClassDef>, name: &str) -> Option<Rc<DefDecl>> {
        let mut current = Some(class.clone());
        while let Some(class) = current {
            if let Some(def) = class.methods.get(name) {
                return Some(def.clone());
            }
            current = class.base.clone();
        }
        None
    }

    fn default_for_variable(
        &mut self,
        variable: &VariableDef,
        env: &Rc<RefCell<Env>>,
    ) -> Result<Value9> {
        match &variable.init {
            Some(expression) => self.eval_expr(expression, env),
            None => Ok(default_for_type(variable.ty.as_ref())),
        }
    }

    fn define_class(&mut self, declaration: &ClassDecl, env: &Rc<RefCell<Env>>) -> Result<()> {
        let base = match &declaration.extends {
            Some(TypeExpr::Named(name)) => {
                let name = name.to_string_lossy().into_owned();
                match Self::env_type(env, &name) {
                    Some(TypeBinding::Class(class)) => Some(class),
                    _ => {
                        return Err(VimError::new(
                            1075,
                            format!("E1075: Class not found: {name}"),
                        ))
                    }
                }
            }
            _ => None,
        };
        let mut instance_vars = Vec::new();
        let mut static_vars = HashMap::new();
        let mut methods = HashMap::new();
        let mut static_methods = HashMap::new();
        for member in &declaration.members {
            match member {
                ClassMember::Var {
                    is_static,
                    pattern,
                    ty,
                    init,
                    ..
                } => {
                    let name = pattern.name.to_string_lossy().into_owned();
                    let variable = VariableDef {
                        name: name.clone(),
                        ty: ty.clone().or_else(|| pattern.ty.clone()),
                        init: init.clone(),
                    };
                    if *is_static {
                        let value = match &variable.init {
                            Some(expression) => self.eval_expr(expression, env)?,
                            None => default_for_type(variable.ty.as_ref()),
                        };
                        static_vars.insert(name, value.legacy());
                    } else {
                        instance_vars.push(variable);
                    }
                }
                ClassMember::Def { def, .. } => {
                    let name = def.name.to_string_lossy().into_owned();
                    let definition = Rc::new(def.clone());
                    if def.is_static {
                        static_methods.insert(name, definition);
                    } else {
                        methods.insert(name, definition);
                    }
                }
            }
        }
        let id = self.classes.len();
        let class = Rc::new(ClassDef {
            name: declaration.name.to_string_lossy().into_owned(),
            is_abstract: declaration.is_abstract,
            base,
            instance_vars,
            static_vars: RefCell::new(static_vars),
            methods,
            static_methods,
            env: env.clone(),
            id,
        });
        self.classes.push(class.clone());
        env.borrow_mut()
            .types
            .insert(class.name.clone(), TypeBinding::Class(class));
        Ok(())
    }

    fn define_enum(&mut self, declaration: &EnumDecl, env: &Rc<RefCell<Env>>) -> Result<()> {
        let mut fields: Vec<VariableDef> = Vec::new();
        let mut constructor_params: Vec<String> = Vec::new();
        for statement in &declaration.body {
            match statement {
                Stmt::Var(declaration) => {
                    if let VarPattern::Name(name) = &declaration.pattern {
                        fields.push(VariableDef {
                            name: name.name.to_string_lossy().into_owned(),
                            ty: declaration.ty.clone().or_else(|| name.ty.clone()),
                            init: declaration.init.clone(),
                        });
                    }
                }
                Stmt::Def(def) if def.name.as_bytes() == b"new" => {
                    for param in &def.params {
                        let name = param.name.to_string_lossy().into_owned();
                        let field = name
                            .strip_prefix("this.")
                            .map(|name| name.to_string())
                            .unwrap_or(name);
                        constructor_params.push(field);
                    }
                }
                _ => {}
            }
        }
        let id = self.enums.len();
        let mut members = HashMap::new();
        for (index, member) in declaration.members.iter().enumerate() {
            let mut values: HashMap<String, Value> = HashMap::new();
            for variable in &fields {
                let value = self.default_for_variable(variable, env)?;
                values.insert(variable.name.clone(), value.legacy());
            }
            for (position, field) in constructor_params.iter().enumerate() {
                if let Some(argument) = member.args.get(position) {
                    let value = self.eval_expr(argument, env)?;
                    values.insert(field.clone(), value.legacy());
                }
            }
            let value_id = self.enum_values.len();
            self.enum_values.push(Rc::new(EnumValue {
                name: member.name.to_string_lossy().into_owned(),
                ordinal: index as i64,
                fields: RefCell::new(values),
            }));
            members.insert(member.name.to_string_lossy().into_owned(), value_id);
        }
        let name = declaration.name.to_string_lossy().into_owned();
        let enum_def = Rc::new(EnumDef {
            name: name.clone(),
            members,
            id,
        });
        self.enums.push(enum_def.clone());
        env.borrow_mut()
            .types
            .insert(name, TypeBinding::Enum(enum_def));
        Ok(())
    }

    fn construct_object(&mut self, class: &Rc<ClassDef>, args: Vec<Value9>) -> Result<Value9> {
        if class.is_abstract {
            return Err(VimError::new(
                1325,
                format!("E1325: Cannot instantiate abstract class: {}", class.name),
            ));
        }
        let object_id = self.objects.len();
        let object = Rc::new(RefCell::new(Object {
            class: class.clone(),
            fields: RefCell::new(HashMap::new()),
        }));
        self.objects.push(object.clone());
        let marker = prefixed_marker(OBJECT_PREFIX, object_id);
        let mut chain = Vec::new();
        let mut current = Some(class.clone());
        while let Some(class) = current {
            chain.push(class.clone());
            current = class.base.clone();
        }
        chain.reverse();
        for class in chain {
            for variable in &class.instance_vars {
                let frame = Self::frame(&class.env);
                Self::env_define(&frame, "this", marker.clone(), false);
                let value = self.default_for_variable(variable, &frame)?;
                let borrowed = object.borrow();
                borrowed
                    .fields
                    .borrow_mut()
                    .insert(variable.name.clone(), value.legacy());
            }
        }
        if let Some(definition) = Self::find_class_method(class, "new") {
            let frame = Self::frame(&class.env);
            Self::env_define(&frame, "this", marker.clone(), false);
            self.bind_params(&definition.params, args, &frame, "new")?;
            self.exec_items(&definition.body, &frame)?;
        } else if !args.is_empty() {
            return Err(VimError::new(
                118,
                format!("E118: Too many arguments for function: {}", class.name),
            ));
        }
        Ok(marker)
    }

    fn member_get(&mut self, base: &Value9, key: &VimString) -> Result<Value9> {
        let text = key.to_string_lossy().into_owned();
        if let Value9::Val(Value::Dict(dict)) = base {
            let borrowed = dict.borrow();
            return match borrowed.get(key) {
                Some(value) => Ok(Value9::Val(value.clone())),
                None => Err(VimError::new(
                    716,
                    format!("E716: Key not present in Dictionary: {text}"),
                )),
            };
        }
        if let Some(id) = marker_id(base, OBJECT_PREFIX) {
            let object = self
                .objects
                .get(id)
                .cloned()
                .ok_or_else(|| VimError::new(716, "E716: Invalid object"))?;
            let field = {
                let borrowed = object.borrow();
                let fields = borrowed.fields.borrow();
                fields.get(&text).cloned()
            };
            if let Some(value) = field {
                return Ok(Value9::Val(value));
            }
            let class = object.borrow().class.clone();
            if let Some(definition) = Self::find_class_method(&class, &text) {
                let callable = Rc::new(Callable::Bound {
                    def: definition,
                    env: class.env.clone(),
                    this_value: Some(base.clone()),
                });
                return Ok(self.make_funcref(callable));
            }
            return Err(VimError::new(
                716,
                format!("E716: Key not present in Dictionary: {text}"),
            ));
        }
        if let Some(id) = marker_id(base, CLASS_PREFIX) {
            let class = self
                .classes
                .get(id)
                .cloned()
                .ok_or_else(|| VimError::new(716, "E716: Invalid class"))?;
            if text == "new" {
                let callable = Rc::new(Callable::Constructor { class });
                return Ok(self.make_funcref(callable));
            }
            if let Some(definition) = class.static_methods.get(&text).cloned() {
                let callable = Rc::new(Callable::Bound {
                    def: definition,
                    env: class.env.clone(),
                    this_value: None,
                });
                return Ok(self.make_funcref(callable));
            }
            let value = class.static_vars.borrow().get(&text).cloned();
            if let Some(value) = value {
                return Ok(Value9::Val(value));
            }
            return Err(VimError::new(
                716,
                format!("E716: Key not present in Dictionary: {text}"),
            ));
        }
        if let Some(id) = marker_id(base, ENUM_PREFIX) {
            let enum_def = self
                .enums
                .get(id)
                .cloned()
                .ok_or_else(|| VimError::new(716, "E716: Invalid enum"))?;
            if let Some(member) = enum_def.members.get(&text) {
                return Ok(prefixed_marker(ENUM_VALUE_PREFIX, *member));
            }
            return Err(VimError::new(
                716,
                format!("E716: Item not found in enum {}: {text}", enum_def.name),
            ));
        }
        if let Some(id) = marker_id(base, ENUM_VALUE_PREFIX) {
            let value = self
                .enum_values
                .get(id)
                .cloned()
                .ok_or_else(|| VimError::new(716, "E716: Invalid enum value"))?;
            if text == "name" {
                return Ok(Value9::from_string(value.name.clone()));
            }
            if text == "ordinal" {
                return Ok(Value9::number(value.ordinal));
            }
            let field = value.fields.borrow().get(&text).cloned();
            if let Some(field) = field {
                return Ok(Value9::Val(field));
            }
            return Err(VimError::new(
                716,
                format!("E716: Key not present in Dictionary: {text}"),
            ));
        }
        Err(VimError::new(
            1030,
            "E1030: Using an unsupported value as a Dictionary",
        ))
    }

    fn member_set(&mut self, base: &Value9, key: &VimString, value: Value9) -> Result<()> {
        let text = key.to_string_lossy().into_owned();
        if let Value9::Val(Value::Dict(dict)) = base {
            dict.borrow_mut().set(key.clone(), value.legacy());
            return Ok(());
        }
        if let Some(id) = marker_id(base, OBJECT_PREFIX) {
            let object = self
                .objects
                .get(id)
                .cloned()
                .ok_or_else(|| VimError::new(716, "E716: Invalid object"))?;
            let borrowed = object.borrow();
            borrowed.fields.borrow_mut().insert(text, value.legacy());
            return Ok(());
        }
        if let Some(id) = marker_id(base, CLASS_PREFIX) {
            let class = self
                .classes
                .get(id)
                .cloned()
                .ok_or_else(|| VimError::new(716, "E716: Invalid class"))?;
            class.static_vars.borrow_mut().insert(text, value.legacy());
            return Ok(());
        }
        Err(VimError::new(
            1030,
            "E1030: Using an unsupported value as a Dictionary",
        ))
    }

    fn register_export(&mut self, item: &Stmt, env: &Rc<RefCell<Env>>) -> Result<()> {
        let mut entries: Vec<(VimString, Value)> = Vec::new();
        match item {
            Stmt::Var(declaration) => {
                for name in pattern_names(&declaration.pattern) {
                    let value = Self::env_lookup(env, &name).unwrap_or_else(Value9::null);
                    entries.push((VimString::from(name), value.legacy()));
                }
            }
            Stmt::Def(def) => {
                let name = def.name.to_string_lossy().into_owned();
                if let Some(callable) = Self::env_function(env, &name) {
                    let value = self.make_funcref(callable);
                    entries.push((VimString::from(name), value.legacy()));
                }
            }
            Stmt::Class(class) => {
                let name = class.name.to_string_lossy().into_owned();
                if let Some(binding) = Self::env_type(env, &name) {
                    let value = self.type_value(&binding);
                    entries.push((VimString::from(name), value.legacy()));
                }
            }
            Stmt::Enum(value) => {
                let name = value.name.to_string_lossy().into_owned();
                if let Some(binding) = Self::env_type(env, &name) {
                    let value = self.type_value(&binding);
                    entries.push((VimString::from(name), value.legacy()));
                }
            }
            Stmt::Export(inner) => {
                // Nested export forms are not expected, but keep them working.
                return self.register_export(&inner.item, env);
            }
            Stmt::TypeAlias(_) | Stmt::Interface(_) => {}
            _ => {}
        }
        if !entries.is_empty() {
            if let Some(frame) = self.frames.last_mut() {
                for (name, value) in entries {
                    frame.exports.set(name, value);
                }
            }
        }
        Ok(())
    }

    fn exec_import(&mut self, import: &ImportDecl, env: &Rc<RefCell<Env>>) -> Result<()> {
        let path_value = self.eval_expr(&import.path, env)?;
        let path_text = match &path_value {
            Value9::Val(Value::String(string)) => string.to_string_lossy().into_owned(),
            Value9::Val(Value::Number(number)) => number.to_string(),
            other => other.display(),
        };
        let resolved = self.resolve_import(&path_text, import.autoload)?;
        if self.loading.iter().any(|path| path == &resolved) {
            return Ok(());
        }
        let source = std::fs::read(&resolved).map_err(|error| {
            VimError::new(
                1053,
                format!("E1053: Could not import {:?}: {error}", path_text),
            )
        })?;
        let script = parser::parse_script(&source)?;
        let dir = resolved.parent().map(|parent| parent.to_path_buf());
        let frame = ScriptFrame {
            env: Rc::new(RefCell::new(Env::default())),
            exports: VimDict::new(),
            dir,
        };
        self.frames.push(frame);
        self.loading.push(resolved.clone());
        let result = {
            let imported_env = self.current_env();
            self.exec_items(&script.items, &imported_env)
        };
        self.loading.pop();
        let frame = self.frames.pop().expect("import frame");
        result?;
        let mut namespace = VimDict::new();
        for entry in frame.exports.entries {
            namespace.set(entry.key, entry.value);
        }
        let alias = match &import.alias {
            Some(alias) => alias.to_string_lossy().into_owned(),
            None => Path::new(&path_text)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("imported")
                .to_string(),
        };
        let slot = Slot {
            value: Value9::Val(Value::dict(namespace)),
            locked: true,
        };
        env.borrow_mut().vars.insert(alias, slot);
        Ok(())
    }

    fn resolve_import(&self, text: &str, autoload: bool) -> Result<PathBuf> {
        let candidate = Path::new(text);
        if candidate.is_absolute() {
            return Ok(candidate.to_path_buf());
        }
        let mut roots: Vec<PathBuf> = Vec::new();
        if let Some(dir) = self.current_dir() {
            roots.push(dir);
        }
        roots.extend(self.host.import_roots(autoload));
        if let Ok(runtime) = std::env::var("VIMRUNTIME") {
            let runtime = PathBuf::from(runtime);
            if autoload {
                roots.push(runtime.join("autoload"));
            } else {
                roots.push(runtime.join("import"));
            }
            roots.push(runtime);
        }
        for root in roots {
            let path = root.join(candidate);
            if path.is_file() {
                return Ok(path);
            }
        }
        if candidate.is_file() {
            return Ok(candidate.to_path_buf());
        }
        Err(VimError::new(
            1053,
            format!("E1053: Could not import {text:?}"),
        ))
    }
}

/// Minimal :normal handling for the standalone runner: enough for the
/// differential corpus (i, a, A, o, O, gg, G).
fn basic_normal(host: &mut BasicHost, keys: &str, cursor: &mut usize) {
    if host.lines.is_empty() {
        host.lines.push(VimString::new());
    }
    let bytes = keys.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'i' | b'a' | b'A' | b'o' | b'O' => {
                basic_normal_insert(host, bytes[index], &bytes[index + 1..], cursor);
                return;
            }
            b'g' if bytes.get(index + 1) == Some(&b'g') => {
                host.current_line = 1;
                *cursor = 0;
                index += 2;
            }
            b'G' => {
                host.current_line = host.lines.len().max(1) as i64;
                *cursor = 0;
                index += 1;
            }
            _ => index += 1,
        }
    }
}

fn basic_normal_insert(host: &mut BasicHost, command: u8, text: &[u8], cursor: &mut usize) {
    let line_index = (host.current_line.max(1) - 1) as usize;
    while host.lines.len() <= line_index {
        host.lines.push(VimString::new());
    }
    match command {
        b'i' => {
            let mut line = host.lines[line_index].as_bytes().to_vec();
            let position = (*cursor).min(line.len());
            line.splice(position..position, text.iter().copied());
            host.lines[line_index] = VimString::from_bytes(line);
            *cursor = position + text.len();
        }
        b'a' => {
            let mut line = host.lines[line_index].as_bytes().to_vec();
            let position = (*cursor + 1).min(line.len());
            line.splice(position..position, text.iter().copied());
            host.lines[line_index] = VimString::from_bytes(line);
            *cursor = position + text.len();
        }
        b'A' => {
            let mut line = host.lines[line_index].as_bytes().to_vec();
            line.extend_from_slice(text);
            host.lines[line_index] = VimString::from_bytes(line);
            *cursor = host.lines[line_index].as_bytes().len();
        }
        b'o' | b'O' => {
            let position = if command == b'o' {
                line_index + 1
            } else {
                line_index
            };
            host.lines
                .insert(position, VimString::from_bytes(text.to_vec()));
            host.current_line = position as i64 + 1;
            *cursor = text.len();
        }
        _ => {}
    }
}

/// Map common Vim short option names to their long form.
fn option_name(name: &str) -> &str {
    match name {
        "ts" => "tabstop",
        "sw" => "shiftwidth",
        "sts" => "softtabstop",
        "et" => "expandtab",
        "vts" => "vartabstop",
        "fdm" => "foldmethod",
        "fde" => "foldexpr",
        "fdt" => "foldtext",
        "tw" => "textwidth",
        "se" => "selection",
        other => other,
    }
}

/// Remove a trailing Vim9 "#" comment, respecting quoted strings.
fn strip_vim9_comment(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for (index, byte) in bytes.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match quote {
            Some(active) => {
                if *byte == b'\\' {
                    escaped = true;
                } else if *byte == active {
                    quote = None;
                }
            }
            None => match byte {
                b'\'' | b'"' => quote = Some(*byte),
                b'#' if index == 0 || bytes[index - 1].is_ascii_whitespace() => {
                    return text[..index].trim_end().to_string();
                }
                _ => {}
            },
        }
    }
    text.trim_end().to_string()
}

fn env_dict(env: &Rc<RefCell<Env>>) -> VimDict {
    let mut dict = VimDict::new();
    let borrowed = env.borrow();
    for (name, slot) in &borrowed.vars {
        dict.set(VimString::from(name.as_str()), slot.value.legacy());
    }
    dict
}

fn default_for_type(ty: Option<&TypeExpr>) -> Value9 {
    match ty {
        Some(TypeExpr::Named(name)) => match name.as_bytes() {
            b"number" => Value9::number(0),
            b"float" => Value9::Val(Value::Float(0.0)),
            b"string" => Value9::from_string(""),
            b"bool" => Value9::Val(Value::Bool(false)),
            b"list" => Value9::list(Vec::new()),
            b"dict" => Value9::Val(Value::dict(VimDict::new())),
            b"blob" => Value9::Val(Value::blob(Vec::new())),
            _ => Value9::null(),
        },
        Some(TypeExpr::List(_)) => Value9::list(Vec::new()),
        Some(TypeExpr::Dict(_)) => Value9::Val(Value::dict(VimDict::new())),
        Some(TypeExpr::Tuple { .. }) => Value9::Tuple(Vec::new()),
        _ => Value9::null(),
    }
}

fn pattern_names(pattern: &VarPattern) -> Vec<String> {
    match pattern {
        VarPattern::Name(name) => vec![name.name.to_string_lossy().into_owned()],
        VarPattern::List { items, rest } => {
            let mut names = Vec::new();
            for item in items {
                names.extend(pattern_names(item));
            }
            if let Some(rest) = rest {
                names.extend(pattern_names(rest));
            }
            names
        }
    }
}

/// Parse and execute a Vim9 script file, resolving imports relative to it.
pub fn run_script_file(path: &Path) -> Result<RunOutput> {
    let source = std::fs::read(path).map_err(|error| {
        VimError::new(
            484,
            format!("E484: Can't open file {}: {error}", path.display()),
        )
    })?;
    let script = parser::parse_script(&source)?;
    let mut runtime = standalone_runtime();
    let mut host = standalone_host();
    let dir = path.parent().map(|parent| parent.to_path_buf());
    let messages = {
        let mut interpreter = Interpreter::new(&mut runtime, &mut host);
        interpreter.execute_script_at(&script, dir)?;
        interpreter.messages
    };
    Ok(RunOutput { messages })
}
