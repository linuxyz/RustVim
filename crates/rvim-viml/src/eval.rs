//! Expression evaluation, variable access and function-call dispatch.

use std::rc::Rc;

use rvim_types::{ScriptId, VimString};

use crate::builtins;
use crate::error::{codes, Result, VimError};
use crate::host::{ExContext, OptionScope as HostOptionScope, VimHost};
use crate::lexer::{AssignOp, OptionScope, Scope};
use crate::ops;
use crate::parser::{BinaryOp, Expr, UnaryOp};
use crate::runtime::VimRuntime;
use crate::script::{self, Flow};
use crate::value::{ClosureScope, DictRef, FuncrefData, FuncrefKind, Value, VimDict};

/// Execution context shared by the evaluator, builtins and statement executor.
pub(crate) struct Ctx<'a> {
    pub runtime: &'a mut VimRuntime,
    pub host: &'a mut dyn VimHost,
    pub script_id: ScriptId,
    pub script_name: String,
    pub line: u32,
    pub function: Option<String>,
    pub command: Option<&'static str>,
    pub silent: bool,
}

impl<'a> Ctx<'a> {
    pub(crate) fn new(runtime: &'a mut VimRuntime, host: &'a mut dyn VimHost) -> Self {
        let script_id = runtime
            .current_script_id()
            .unwrap_or_else(|| ScriptId::from_raw(0));
        let script_name = runtime.script_name(script_id).unwrap_or_default();
        let line = runtime.current_line_number();
        let function = runtime.current_function_name();
        Self {
            runtime,
            host,
            script_id,
            script_name,
            line,
            function,
            command: None,
            silent: false,
        }
    }

    pub(crate) fn throwpoint(&self) -> String {
        if let Some(function) = &self.function {
            return format!("function {function}, line {}", self.line);
        }
        if !self.script_id.is_nil() && !self.script_name.is_empty() {
            return format!("script {}, line {}", self.script_name, self.line);
        }
        format!("line {}", self.line)
    }

    /// Give a parser-produced `line N` throwpoint the script/function name.
    pub(crate) fn attribute_parse_error(&self, mut error: VimError) -> VimError {
        if let Some(relative) = error
            .throwpoint
            .as_deref()
            .and_then(|point| point.strip_prefix("line "))
            .map(str::to_string)
        {
            error.throwpoint = Some(if let Some(function) = &self.function {
                format!("function {function}, line {relative}")
            } else if !self.script_id.is_nil() && !self.script_name.is_empty() {
                format!("script {}, line {relative}", self.script_name)
            } else {
                format!("line {relative}")
            });
        }
        error
    }

    pub(crate) fn decorate(&mut self, error: VimError) -> VimError {
        let mut error = error;
        if error.throwpoint.is_none() {
            error.throwpoint = Some(self.throwpoint());
        }
        if error.kind == crate::error::ErrorKind::Error && error.exception.is_none() {
            let command = self.command.unwrap_or("eval");
            error.exception = Some(format!("Vim({command}):{}", error.error_text()));
        }
        self.runtime.set_last_error(&error);
        error
    }

    pub(crate) fn emit(&mut self, text: &str, newline: bool) {
        if self.runtime.is_capturing() {
            self.runtime.append_capture(text);
            if newline {
                self.runtime.append_capture("\n");
            }
        } else if !self.silent {
            self.host.message(text);
        }
    }

    pub(crate) fn emit_error(&mut self, error: &VimError) {
        if self.runtime.is_capturing() {
            self.runtime.append_capture(&error.to_string());
            self.runtime.append_capture("\n");
        } else if !self.silent {
            self.host.error(error);
        }
    }

    pub(crate) fn ignorecase(&mut self) -> bool {
        let value = self
            .host
            .get_option(HostOptionScope::Default, "ignorecase")
            .or_else(|_| self.host.get_option(HostOptionScope::Global, "ignorecase"))
            .or_else(|_| self.host.get_option(HostOptionScope::Local, "ignorecase"));
        match value {
            Ok(value) => ops::truthy(&value).unwrap_or(false),
            Err(_) => false,
        }
    }

    pub(crate) fn current_script_id(&self) -> Option<ScriptId> {
        if self.script_id.is_nil() {
            self.runtime.current_script_id()
        } else {
            Some(self.script_id)
        }
    }

    pub(crate) fn current_script_vars(&self) -> Option<DictRef> {
        self.current_script_id()
            .and_then(|id| self.runtime.script_vars(id))
    }

    pub(crate) fn set_statement(&mut self, line: u32, command: &'static str) {
        self.line = line;
        self.command = Some(command);
        self.runtime.set_current_line(line);
    }

    pub(crate) fn ex_context(&self) -> ExContext {
        ExContext::new(
            self.script_id,
            self.script_name.clone(),
            self.line,
            self.function.clone(),
        )
    }
}

/// Evaluate a complete expression in the runtime's current context.
pub fn eval_expression(
    input: &[u8],
    runtime: &mut VimRuntime,
    host: &mut dyn VimHost,
) -> Result<Value> {
    let mut ctx = Ctx::new(runtime, host);
    let expression =
        crate::parser::Parser::parse_complete(input).map_err(|error| ctx.decorate(error))?;
    match eval(&mut ctx, &expression) {
        Ok(value) => Ok(value),
        Err(error) => Err(ctx.decorate(error).into_uncaught_throw()),
    }
}

/// Call a named function (user, builtin or host-provided).
pub fn call_function(
    name: &str,
    args: Vec<Value>,
    runtime: &mut VimRuntime,
    host: &mut dyn VimHost,
) -> Result<Value> {
    let mut ctx = Ctx::new(runtime, host);
    match call_named(&mut ctx, name, args, None) {
        Ok(value) => Ok(value),
        Err(error) => Err(ctx.decorate(error).into_uncaught_throw()),
    }
}

pub(crate) fn eval(ctx: &mut Ctx, expression: &Expr) -> Result<Value> {
    match expression {
        Expr::Number(number) => Ok(Value::Number(*number)),
        Expr::Float(float) => Ok(Value::Float(*float)),
        Expr::String(string) => Ok(Value::String(string.clone())),
        Expr::Blob(bytes) => Ok(Value::blob(bytes.clone())),
        Expr::List(items) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(eval(ctx, item)?);
            }
            Ok(Value::list(values))
        }
        Expr::Dict(entries) => {
            let mut dict = VimDict::new();
            for (key, value) in entries {
                let key = ops::to_string(&eval(ctx, key)?)?;
                if dict.contains_key(&key) {
                    return Err(VimError::new(
                        721,
                        format!("Duplicate key in dictionary: {}", key.to_string_lossy()),
                    ));
                }
                dict.set(key, eval(ctx, value)?);
            }
            Ok(Value::dict(dict))
        }
        Expr::Var { scope, name } => get_var(ctx, *scope, name),
        Expr::Option { scope, name } => get_option(ctx, *scope, name),
        Expr::Env(name) => ctx.host.get_env(name),
        Expr::Register(name) => ctx.host.get_register(*name as char),
        Expr::Index { base, index } => {
            let base = eval(ctx, base)?;
            let index = eval(ctx, index)?;
            index_value(ctx, &base, &index)
        }
        Expr::Slice { base, start, end } => {
            let base = eval(ctx, base)?;
            let start = match start {
                Some(start) => Some(eval(ctx, start)?),
                None => None,
            };
            let end = match end {
                Some(end) => Some(eval(ctx, end)?),
                None => None,
            };
            slice_value(&base, start.as_ref(), end.as_ref())
        }
        Expr::Member { base, key } => {
            let base = eval(ctx, base)?;
            member_value(ctx, &base, key)
        }
        Expr::Call { callee, args } => eval_call(ctx, callee, args),
        Expr::MethodCall {
            receiver,
            callee,
            args,
        } => {
            let receiver = eval(ctx, receiver)?;
            let mut values = Vec::with_capacity(args.len() + 1);
            values.push(receiver);
            for argument in args {
                values.push(eval(ctx, argument)?);
            }
            match callee.as_ref() {
                Expr::Var { scope: None, name } => call_named_or_variable(ctx, name, values),
                Expr::Var {
                    scope: Some(Scope::Script),
                    name,
                } => {
                    let written = format!("s:{}", name.to_string_lossy());
                    call_named(ctx, &written, values, None)
                }
                _ => {
                    let function = eval(ctx, callee)?;
                    call_funcref(ctx, &function, values, None)
                }
            }
        }
        Expr::Unary { op, operand } => {
            let operand = eval(ctx, operand)?;
            match op {
                UnaryOp::Not => Ok(Value::Number(i64::from(!ops::truthy(&operand)?))),
                UnaryOp::Negate => {
                    if let Value::Float(float) = operand {
                        Ok(Value::Float(-float))
                    } else {
                        Ok(Value::Number(ops::to_number(&operand)?.wrapping_neg()))
                    }
                }
                UnaryOp::Plus => {
                    if let Value::Float(float) = operand {
                        Ok(Value::Float(float))
                    } else {
                        Ok(Value::Number(ops::to_number(&operand)?))
                    }
                }
            }
        }
        Expr::Binary { op, left, right } => match op {
            BinaryOp::And => {
                let left = eval(ctx, left)?;
                if !ops::truthy(&left)? {
                    return Ok(Value::Number(0));
                }
                let right = eval(ctx, right)?;
                Ok(Value::Number(i64::from(ops::truthy(&right)?)))
            }
            BinaryOp::Or => {
                let left = eval(ctx, left)?;
                if ops::truthy(&left)? {
                    return Ok(Value::Number(1));
                }
                let right = eval(ctx, right)?;
                Ok(Value::Number(i64::from(ops::truthy(&right)?)))
            }
            _ => {
                let left = eval(ctx, left)?;
                let right = eval(ctx, right)?;
                ops::apply_binary(*op, &left, &right, ctx.ignorecase())
            }
        },
        Expr::Ternary {
            condition,
            then_value,
            else_value,
        } => {
            if ops::truthy(&eval(ctx, condition)?)? {
                eval(ctx, then_value)
            } else {
                eval(ctx, else_value)
            }
        }
        Expr::Lambda(body) => {
            let closure = ctx.runtime.current_frame().map(|frame| {
                Rc::new(ClosureScope::new(
                    frame.locals.clone(),
                    frame.args.clone(),
                    frame.closure.clone(),
                ))
            });
            let mut data = FuncrefData::new(
                format!("<lambda>{}", body.line),
                FuncrefKind::Lambda,
                ctx.current_script_id()
                    .unwrap_or_else(|| ScriptId::from_raw(0)),
            );
            data.closure = closure;
            data.lambda = Some(body.clone());
            Ok(Value::funcref(data))
        }
    }
}

pub(crate) fn eval_call(ctx: &mut Ctx, callee: &Expr, args: &[Expr]) -> Result<Value> {
    let mut values = Vec::with_capacity(args.len());
    for argument in args {
        values.push(eval(ctx, argument)?);
    }
    match callee {
        Expr::Var { scope: None, name } => call_named_or_variable(ctx, name, values),
        Expr::Var {
            scope: Some(Scope::Script),
            name,
        } => {
            let written = format!("s:{}", name.to_string_lossy());
            call_named(ctx, &written, values, None)
        }
        Expr::Var {
            scope: Some(scope),
            name,
        } => {
            let value = get_var(ctx, Some(*scope), name)?;
            call_funcref(ctx, &value, values, None)
        }
        Expr::Member { base, key } => {
            let base_value = eval(ctx, base)?;
            let member = member_value(ctx, &base_value, key)?;
            call_funcref(ctx, &member, values, Some(base_value))
        }
        _ => {
            let function = eval(ctx, callee)?;
            call_funcref(ctx, &function, values, None)
        }
    }
}

/// Call a bare name: a real function wins over a Funcref variable.
pub(crate) fn call_named_or_variable(
    ctx: &mut Ctx,
    name: &VimString,
    args: Vec<Value>,
) -> Result<Value> {
    let readable = name.to_string_lossy().into_owned();
    let canonical = resolve_function_name(ctx, &readable);
    if ctx.runtime.has_function(&canonical) || builtins::is_builtin(&canonical) {
        return call_named(ctx, &readable, args, None);
    }
    match get_var(ctx, None, name) {
        Ok(value) if value.is_funcref() => call_funcref(ctx, &value, args, None),
        Ok(_) | Err(_) => call_named(ctx, &readable, args, None),
    }
}

pub(crate) fn call_named(
    ctx: &mut Ctx,
    written: &str,
    args: Vec<Value>,
    self_dict: Option<Value>,
) -> Result<Value> {
    let canonical = resolve_function_name(ctx, written);
    if let Some(function) = ctx.runtime.get_function(&canonical).cloned() {
        return script::call_user_function(ctx, &function, &args, self_dict);
    }
    if builtins::is_builtin(&canonical) {
        return builtins::call_builtin(ctx, &canonical, &args);
    }
    if let Some(value) = ctx.host.call_function(&canonical, &args)? {
        return Ok(value);
    }
    if canonical.contains('#') {
        // Load autoload/foo.vim on first use of foo#bar().
        autoload_for(ctx, &canonical)?;
        if let Some(function) = ctx.runtime.get_function(&canonical).cloned() {
            return script::call_user_function(ctx, &function, &args, self_dict);
        }
    }
    if canonical.contains('#') {
        return Err(VimError::new(
            117,
            format!(
                "Unknown function: {written} (autoload loading is not implemented; define the function explicitly)"
            ),
        ));
    }
    Err(codes::e117(written))
}

pub(crate) fn call_funcref(
    ctx: &mut Ctx,
    function: &Value,
    args: Vec<Value>,
    self_dict: Option<Value>,
) -> Result<Value> {
    let Some(reference) = function.as_funcref() else {
        return Err(VimError::new(
            1085,
            format!("Not a callable type: {}", function.type_name()),
        ));
    };
    let data = match reference.try_borrow() {
        Ok(data) => data.clone(),
        Err(_) => return Err(VimError::plain("Funcref is already borrowed")),
    };
    let mut all_args = data.args.clone();
    all_args.extend(args);
    let bound_dict = data.dict.clone().or(self_dict);
    match data.kind {
        FuncrefKind::Lambda => script::call_lambda(ctx, &data, all_args),
        FuncrefKind::Builtin => {
            let name = data.name.to_string_lossy().into_owned();
            builtins::call_builtin(ctx, &name, &all_args)
        }
        FuncrefKind::User | FuncrefKind::Partial => {
            let name = data.name.to_string_lossy().into_owned();
            let canonical = resolve_function_name(ctx, &name);
            if let Some(function) = ctx.runtime.get_function(&canonical).cloned() {
                script::call_user_function(ctx, &function, &all_args, bound_dict)
            } else if builtins::is_builtin(&canonical) {
                builtins::call_builtin(ctx, &canonical, &all_args)
            } else {
                Err(codes::e117(&name))
            }
        }
    }
}

/// Resolve s:Name and <SID>Name to the internal <SNR>{id}_Name form.
pub(crate) fn resolve_function_name(ctx: &Ctx, written: &str) -> String {
    let lower = written.to_ascii_lowercase();
    if let Some(rest) = written.strip_prefix("s:") {
        if let Some(id) = ctx.current_script_id() {
            return format!("<SNR>{}_", id.index()).to_string() + rest;
        }
        return written.to_string();
    }
    for prefix in ["<sid>", "<script>"] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            if let Some(id) = ctx.current_script_id() {
                return format!("<SNR>{}_", id.index()).to_string()
                    + &written[written.len() - rest.len()..];
            }
            return written[prefix.len()..].to_string();
        }
    }
    written.to_string()
}

pub(crate) fn get_var(ctx: &mut Ctx, scope: Option<Scope>, name: &VimString) -> Result<Value> {
    let display = format!(
        "{}{}",
        scope.map_or("", Scope::prefix),
        name.to_string_lossy()
    );
    if name.is_empty() {
        return whole_scope(ctx, scope, &display);
    }
    match scope {
        Some(Scope::Global) => {
            let key = name.to_string_lossy().into_owned();
            if let Some(value) = ctx.runtime.get_global(&key) {
                return Ok(value);
            }
            autoload_for(ctx, &key)?;
            ctx.runtime
                .get_global(&key)
                .ok_or_else(|| codes::e121(&display))
        }
        Some(Scope::Buffer) | Some(Scope::Window) | Some(Scope::Tab) => {
            let scope = host_scope(scope.unwrap());
            ctx.host.get_var(scope, &name.to_string_lossy())
        }
        Some(Scope::Script) => {
            let vars = ctx
                .current_script_vars()
                .ok_or_else(|| codes::e121(&display))?;
            dict_lookup(&vars, name).ok_or_else(|| codes::e121(&display))
        }
        Some(Scope::Vim) => ctx
            .runtime
            .get_vvar(&name.to_string_lossy())
            .ok_or_else(|| codes::e121(&display)),
        Some(Scope::Local) | Some(Scope::Argument) => {
            let frame = ctx
                .runtime
                .current_frame()
                .ok_or_else(|| codes::e121(&display))?;
            let primary = if scope == Some(Scope::Argument) {
                &frame.args
            } else {
                &frame.locals
            };
            let closure = frame.closure.clone();
            let arguments = scope == Some(Scope::Argument);
            dict_lookup(primary, name)
                .or_else(|| closure_lookup_kind(&closure, name, arguments))
                .ok_or_else(|| codes::e121(&display))
        }
        None => {
            // Legacy Vim script exposes a few bare constants.  "version" is
            // the same value as v:version and is used by runtime files such
            // as syntax/ada.vim ("if version < 600").
            if name.as_bytes() == b"version" {
                return Ok(ctx
                    .runtime
                    .get_vvar("version")
                    .unwrap_or(Value::Number(902)));
            }
            if name.as_bytes() == b"self" {
                if let Some(frame) = ctx.runtime.current_frame() {
                    if let Some(self_dict) = &frame.self_dict {
                        return Ok(self_dict.clone());
                    }
                }
            }
            let found = if let Some(frame) = ctx.runtime.current_frame() {
                let locals = frame.locals.clone();
                let args = frame.args.clone();
                let closure = frame.closure.clone();
                dict_lookup(&locals, name)
                    .or_else(|| dict_lookup(&args, name))
                    .or_else(|| closure_lookup(&closure, name))
            } else {
                ctx.runtime.get_global(&name.to_string_lossy())
            };
            if let Some(value) = found {
                return Ok(value);
            }
            autoload_for(ctx, &name.to_string_lossy())?;
            ctx.runtime
                .get_global(&name.to_string_lossy())
                .ok_or_else(|| codes::e121(&display))
        }
    }
}

/// Source the autoload script for a `prefix#name` variable or function once.
fn autoload_for(ctx: &mut Ctx, name: &str) -> Result<()> {
    let Some((prefix, _)) = name.split_once('#') else {
        return Ok(());
    };
    if prefix.is_empty() {
        return Ok(());
    }
    let expected = format!("autoload/{prefix}.vim");
    if ctx.runtime.has_script_name(&expected) {
        return Ok(());
    }
    if let Some((path, bytes)) = ctx.host.autoload_script(prefix)? {
        crate::script::run_sourced_script(ctx, &path, &bytes)?;
    }
    Ok(())
}

fn whole_scope(ctx: &mut Ctx, scope: Option<Scope>, display: &str) -> Result<Value> {
    match scope {
        Some(Scope::Global) => Ok(Value::Dict(ctx.runtime.globals())),
        Some(Scope::Script) => {
            let vars = ctx
                .current_script_vars()
                .ok_or_else(|| codes::e121(display))?;
            Ok(Value::Dict(vars))
        }
        Some(Scope::Local) | Some(Scope::Argument) => {
            let frame = ctx
                .runtime
                .current_frame()
                .ok_or_else(|| codes::e121(display))?;
            let dict = if scope == Some(Scope::Argument) {
                &frame.args
            } else {
                &frame.locals
            };
            Ok(Value::Dict(dict.clone()))
        }
        Some(Scope::Vim) => Ok(Value::dict(ctx.runtime.vvar_dict().clone())),
        Some(Scope::Buffer) | Some(Scope::Window) | Some(Scope::Tab) => {
            let scope = host_scope(scope.ok_or_else(|| codes::e15("missing scope"))?);
            ctx.host.get_scope_dict(scope)
        }
        None => Err(codes::e15("empty variable name")),
    }
}

fn dict_lookup(dict: &DictRef, name: &VimString) -> Option<Value> {
    dict.try_borrow()
        .ok()
        .and_then(|dict| dict.get(name).cloned())
}

fn closure_lookup(closure: &Option<Rc<ClosureScope>>, name: &VimString) -> Option<Value> {
    closure_lookup_kind(closure, name, false).or_else(|| closure_lookup_kind(closure, name, true))
}

fn closure_lookup_kind(
    closure: &Option<Rc<ClosureScope>>,
    name: &VimString,
    arguments: bool,
) -> Option<Value> {
    let mut current = closure.clone();
    while let Some(scope) = current {
        let dict = if arguments {
            &scope.args
        } else {
            &scope.locals
        };
        if let Some(value) = dict_lookup(dict, name) {
            return Some(value);
        }
        current = scope.parent.clone();
    }
    None
}

pub(crate) fn set_var(
    ctx: &mut Ctx,
    scope: Option<Scope>,
    name: &VimString,
    value: Value,
) -> Result<()> {
    let display = format!(
        "{}{}",
        scope.map_or("", Scope::prefix),
        name.to_string_lossy()
    );
    if name.is_empty() {
        return Err(codes::e46(&display));
    }
    match scope {
        Some(Scope::Global) => ctx
            .runtime
            .set_global_checked(&name.to_string_lossy(), value),
        Some(Scope::Buffer) | Some(Scope::Window) | Some(Scope::Tab) => {
            let scope = host_scope(scope.unwrap());
            ctx.host.set_var(scope, &name.to_string_lossy(), value)
        }
        Some(Scope::Script) => {
            let vars = ctx
                .current_script_vars()
                .ok_or_else(|| VimError::new(121, format!("Undefined variable: {display}")))?;
            set_in_dict(&vars, name, value, &display)
        }
        Some(Scope::Vim) => ctx.runtime.set_vvar_checked(&name.to_string_lossy(), value),
        Some(Scope::Argument) => Err(codes::e46(&display)),
        Some(Scope::Local) => {
            let frame = ctx
                .runtime
                .current_frame()
                .ok_or_else(|| codes::e121(&display))?;
            let closure = frame.closure.clone();
            if let Some(target) = closure_locals(&closure, name) {
                return set_in_dict(&target, name, value, &display);
            }
            let locals = frame.locals.clone();
            set_in_dict(&locals, name, value, &display)
        }
        None => {
            if let Some(frame) = ctx.runtime.current_frame() {
                let closure = frame.closure.clone();
                if let Some(target) = closure_locals(&closure, name) {
                    return set_in_dict(&target, name, value, &display);
                }
                let locals = frame.locals.clone();
                set_in_dict(&locals, name, value, &display)
            } else {
                ctx.runtime
                    .set_global_checked(&name.to_string_lossy(), value)
            }
        }
    }
}

fn closure_locals(closure: &Option<Rc<ClosureScope>>, name: &VimString) -> Option<DictRef> {
    let mut current = closure.clone();
    while let Some(scope) = current {
        let exists = scope
            .locals
            .try_borrow()
            .ok()
            .is_some_and(|dict| dict.contains_key(name));
        if exists {
            return Some(scope.locals.clone());
        }
        current = scope.parent.clone();
    }
    None
}

fn set_in_dict(dict: &DictRef, name: &VimString, value: Value, display: &str) -> Result<()> {
    let mut dict = dict
        .try_borrow_mut()
        .map_err(|_| VimError::plain(format!("Dictionary is already borrowed: {display}")))?;
    if dict.locked || dict.entry_locked(name) {
        return Err(codes::e741(display));
    }
    dict.set(name.clone(), value);
    Ok(())
}

pub(crate) fn delete_var(
    ctx: &mut Ctx,
    scope: Option<Scope>,
    name: &VimString,
    bang: bool,
) -> Result<()> {
    let display = format!(
        "{}{}",
        scope.map_or("", Scope::prefix),
        name.to_string_lossy()
    );
    let missing = || {
        if bang {
            Ok(())
        } else {
            Err(codes::e108(&display))
        }
    };
    match scope {
        Some(Scope::Global) => match ctx.runtime.remove_global_checked(&name.to_string_lossy())? {
            Some(_) => Ok(()),
            None => missing(),
        },
        Some(Scope::Buffer) | Some(Scope::Window) | Some(Scope::Tab) => ctx
            .host
            .delete_var(host_scope(scope.unwrap()), &name.to_string_lossy())
            .or_else(|error| if bang { Ok(()) } else { Err(error) }),
        Some(Scope::Script) => {
            let vars = ctx
                .current_script_vars()
                .ok_or_else(|| codes::e108(&display))?;
            remove_from_dict(&vars, name, &display, bang)
        }
        Some(Scope::Vim) | Some(Scope::Argument) => Err(codes::e46(&display)),
        Some(Scope::Local) => {
            let frame = ctx
                .runtime
                .current_frame()
                .ok_or_else(|| codes::e108(&display))?;
            let closure = frame.closure.clone();
            if let Some(target) = closure_locals(&closure, name) {
                return remove_from_dict(&target, name, &display, bang);
            }
            let locals = frame.locals.clone();
            remove_from_dict(&locals, name, &display, bang)
        }
        None => {
            if let Some(frame) = ctx.runtime.current_frame() {
                let closure = frame.closure.clone();
                if let Some(target) = closure_locals(&closure, name) {
                    return remove_from_dict(&target, name, &display, bang);
                }
                let locals = frame.locals.clone();
                remove_from_dict(&locals, name, &display, bang)
            } else {
                match ctx.runtime.remove_global_checked(&name.to_string_lossy())? {
                    Some(_) => Ok(()),
                    None => missing(),
                }
            }
        }
    }
}

fn remove_from_dict(dict: &DictRef, name: &VimString, display: &str, bang: bool) -> Result<()> {
    let mut dict = dict
        .try_borrow_mut()
        .map_err(|_| VimError::plain(format!("Dictionary is already borrowed: {display}")))?;
    if dict.locked || dict.entry_locked(name) {
        return Err(codes::e741(display));
    }
    if dict.remove(name).is_some() || bang {
        Ok(())
    } else {
        Err(codes::e108(display))
    }
}

pub(crate) fn get_option(ctx: &mut Ctx, scope: Option<OptionScope>, name: &str) -> Result<Value> {
    ctx.host.get_option(host_option_scope(scope), name)
}

pub(crate) fn set_option(
    ctx: &mut Ctx,
    scope: Option<OptionScope>,
    name: &str,
    value: Value,
) -> Result<()> {
    ctx.host.set_option(host_option_scope(scope), name, value)
}

fn host_option_scope(scope: Option<OptionScope>) -> HostOptionScope {
    match scope {
        None => HostOptionScope::Default,
        Some(OptionScope::Global) => HostOptionScope::Global,
        Some(OptionScope::Local) => HostOptionScope::Local,
    }
}

fn host_scope(scope: Scope) -> crate::host::VarScope {
    match scope {
        Scope::Global => crate::host::VarScope::Global,
        Scope::Buffer => crate::host::VarScope::Buffer,
        Scope::Window => crate::host::VarScope::Window,
        Scope::Tab => crate::host::VarScope::Tab,
        Scope::Local => crate::host::VarScope::Local,
        Scope::Argument => crate::host::VarScope::Argument,
        Scope::Script => crate::host::VarScope::Script,
        Scope::Vim => crate::host::VarScope::Vim,
    }
}

pub(crate) fn index_value(_ctx: &mut Ctx, base: &Value, index: &Value) -> Result<Value> {
    match base {
        Value::List(list) => {
            let number = ops::to_number(index)?;
            let list = list
                .try_borrow()
                .map_err(|_| VimError::plain("List is already borrowed"))?;
            let index =
                normalize_index(number, list.items.len()).ok_or_else(|| codes::e684(number))?;
            Ok(list.items[index].clone())
        }
        Value::String(string) => {
            let number = ops::to_number(index)?;
            if number < -(string.len() as i64) || number >= string.len() as i64 {
                return Ok(Value::String(VimString::new()));
            }
            let index = if number < 0 {
                string.len() as i64 + number
            } else {
                number
            } as usize;
            Ok(Value::String(VimString::from(
                &string.as_bytes()[index..index + 1],
            )))
        }
        Value::Blob(blob) => {
            let number = ops::to_number(index)?;
            let blob = blob
                .try_borrow()
                .map_err(|_| VimError::plain("Blob is already borrowed"))?;
            let index = normalize_index(number, blob.len()).ok_or_else(|| codes::e684(number))?;
            Ok(Value::Number(i64::from(blob[index])))
        }
        Value::Dict(dict) => {
            let key = key_string(index)?;
            let dict = dict
                .try_borrow()
                .map_err(|_| VimError::plain("Dictionary is already borrowed"))?;
            dict.get(&key)
                .cloned()
                .ok_or_else(|| codes::e716(&key.to_string_lossy()))
        }
        other => Err(VimError::new(
            715,
            format!("Cannot index a {} value", other.type_name()),
        )),
    }
}

pub(crate) fn member_value(_ctx: &mut Ctx, base: &Value, key: &VimString) -> Result<Value> {
    let Value::Dict(dict) = base else {
        return Err(VimError::new(
            715,
            format!(
                "Dictionary required for member access, found {}",
                base.type_name()
            ),
        ));
    };
    let dict = dict
        .try_borrow()
        .map_err(|_| VimError::plain("Dictionary is already borrowed"))?;
    dict.get(key)
        .cloned()
        .ok_or_else(|| codes::e716(&key.to_string_lossy()))
}

pub(crate) fn slice_value(
    base: &Value,
    start: Option<&Value>,
    end: Option<&Value>,
) -> Result<Value> {
    let start = match start {
        Some(value) => Some(ops::to_number(value)?),
        None => None,
    };
    let end = match end {
        Some(value) => Some(ops::to_number(value)?),
        None => None,
    };
    match base {
        Value::List(list) => {
            let list = list
                .try_borrow()
                .map_err(|_| VimError::plain("List is already borrowed"))?;
            let (start, end) = slice_range(list.items.len(), start, end);
            Ok(Value::list(list.items[start..end].to_vec()))
        }
        Value::String(string) => {
            let (start, end) = slice_range(string.len(), start, end);
            Ok(Value::String(VimString::from(
                &string.as_bytes()[start..end],
            )))
        }
        Value::Blob(blob) => {
            let blob = blob
                .try_borrow()
                .map_err(|_| VimError::plain("Blob is already borrowed"))?;
            let (start, end) = slice_range(blob.len(), start, end);
            Ok(Value::blob(blob[start..end].to_vec()))
        }
        other => Err(VimError::new(
            714,
            format!(
                "List, String or Blob required for slicing, found {}",
                other.type_name()
            ),
        )),
    }
}

fn slice_range(len: usize, start: Option<i64>, end: Option<i64>) -> (usize, usize) {
    let len_i = len as i64;
    let mut start = start.unwrap_or(0);
    if start < 0 {
        start += len_i;
    }
    start = start.clamp(0, len_i);
    let mut end = end.unwrap_or(len_i - 1);
    if end < 0 {
        end += len_i;
    }
    end = end.min(len_i - 1);
    if end < start {
        return (start as usize, start as usize);
    }
    (start as usize, (end + 1) as usize)
}

fn normalize_index(index: i64, len: usize) -> Option<usize> {
    let len_i = len as i64;
    let index = if index < 0 { len_i + index } else { index };
    if index < 0 || index >= len_i {
        None
    } else {
        Some(index as usize)
    }
}

fn key_string(value: &Value) -> Result<VimString> {
    match value {
        Value::String(string) => Ok(string.clone()),
        Value::Number(_) | Value::Float(_) | Value::Bool(_) | Value::Special(_) => {
            ops::to_string(value)
        }
        _ => Err(VimError::new(
            730,
            format!("Using a {} as a Dictionary key", value.type_name()),
        )),
    }
}

pub(crate) fn assign_target(
    ctx: &mut Ctx,
    target: &script::AssignTarget,
    value: Value,
    op: AssignOp,
) -> Result<()> {
    match target {
        script::AssignTarget::Expr(expression) => assign_expression(ctx, expression, value, op),
        script::AssignTarget::DynamicName { scope, parts } => {
            let name = resolve_dynamic_name(ctx, parts)?;
            if op == AssignOp::Assign {
                set_var(ctx, *scope, &name, value)
            } else {
                let current = get_var(ctx, *scope, &name)?;
                let updated = apply_assignment(op, current, value, ctx.ignorecase())?;
                set_var(ctx, *scope, &name, updated)
            }
        }
        script::AssignTarget::Destructure { names, rest } => {
            let list = value.as_list().ok_or_else(|| {
                VimError::new(687, "List value required for destructuring assignment")
            })?;
            let items = list
                .try_borrow()
                .map(|list| list.items.clone())
                .unwrap_or_default();
            if items.len() < names.len() {
                return Err(VimError::new(
                    687,
                    "List value is too short for destructuring assignment",
                ));
            }
            for (name, item) in names.iter().zip(items.iter()) {
                assign_expression(ctx, name, item.clone(), AssignOp::Assign)?;
            }
            if let Some(rest_target) = rest {
                let rest_items = if items.len() > names.len() {
                    items[names.len()..].to_vec()
                } else {
                    Vec::new()
                };
                assign_expression(ctx, rest_target, Value::list(rest_items), AssignOp::Assign)?;
            }
            Ok(())
        }
    }
}

/// Build the variable name of a dynamic `:let {expr}name` target.
pub(crate) fn resolve_dynamic_name(
    ctx: &mut Ctx,
    parts: &[script::DynamicNamePart],
) -> Result<VimString> {
    let mut name: Vec<u8> = Vec::new();
    for part in parts {
        match part {
            script::DynamicNamePart::Literal(text) => name.extend_from_slice(text.as_bytes()),
            script::DynamicNamePart::Expr(expression) => {
                let value = eval(ctx, expression)?;
                name.extend_from_slice(ops::to_string(&value)?.as_bytes());
            }
        }
    }
    Ok(VimString::from(String::from_utf8_lossy(&name).into_owned()))
}

pub(crate) fn assign_expression(
    ctx: &mut Ctx,
    expression: &Expr,
    value: Value,
    op: AssignOp,
) -> Result<()> {
    match expression {
        Expr::Var { scope, name } => {
            if op == AssignOp::Assign {
                set_var(ctx, *scope, name, value)
            } else {
                let current = get_var(ctx, *scope, name)?;
                let updated = apply_assignment(op, current, value, ctx.ignorecase())?;
                set_var(ctx, *scope, name, updated)
            }
        }
        Expr::Option { scope, name } => {
            if op == AssignOp::Assign {
                set_option(ctx, *scope, name, value)
            } else {
                let current = get_option(ctx, *scope, name)?;
                set_option(
                    ctx,
                    *scope,
                    name,
                    apply_assignment(op, current, value, false)?,
                )
            }
        }
        Expr::Env(name) => {
            if op == AssignOp::Assign {
                let text = ops::to_string(&value)?;
                ctx.host.set_env(name, &text)
            } else {
                let current = ctx.host.get_env(name)?;
                let updated = apply_assignment(op, current, value, false)?;
                let text = ops::to_string(&updated)?;
                ctx.host.set_env(name, &text)
            }
        }
        Expr::Register(name) => {
            if op == AssignOp::Assign {
                ctx.host.set_register(*name as char, value)
            } else {
                let current = ctx.host.get_register(*name as char)?;
                let updated = apply_assignment(op, current, value, false)?;
                ctx.host.set_register(*name as char, updated)
            }
        }
        Expr::Index { base, index } => {
            let base_value = eval(ctx, base)?;
            let index_val = eval(ctx, index)?;
            let updated = if op == AssignOp::Assign {
                value
            } else {
                let current = index_value(ctx, &base_value, &index_val)?;
                apply_assignment(op, current, value, ctx.ignorecase())?
            };
            set_index_value(&base_value, &index_val, updated)
        }
        Expr::Member { base, key } => {
            let base_value = eval(ctx, base)?;
            let updated = if op == AssignOp::Assign {
                value
            } else {
                let current = member_value(ctx, &base_value, key)?;
                apply_assignment(op, current, value, ctx.ignorecase())?
            };
            set_member_value(&base_value, key, updated)
        }
        Expr::Slice { base, start, end } => {
            if op != AssignOp::Assign {
                return Err(VimError::new(
                    15,
                    "Compound assignment to a slice is not supported",
                ));
            }
            let base_value = eval(ctx, base)?;
            let start = match start {
                Some(start) => Some(ops::to_number(&eval(ctx, start)?)?),
                None => None,
            };
            let end = match end {
                Some(end) => Some(ops::to_number(&eval(ctx, end)?)?),
                None => None,
            };
            set_slice_value(&base_value, start, end, value)
        }
        _ => Err(codes::e15("invalid assignment target")),
    }
}

fn apply_assignment(op: AssignOp, current: Value, value: Value, ignorecase: bool) -> Result<Value> {
    let binary = match op {
        AssignOp::Assign => return Ok(value),
        AssignOp::Add => BinaryOp::Add,
        AssignOp::Sub => BinaryOp::Sub,
        AssignOp::Mul => BinaryOp::Mul,
        AssignOp::Div => BinaryOp::Div,
        AssignOp::Rem => BinaryOp::Rem,
        AssignOp::Concat => BinaryOp::Concat,
    };
    ops::apply_binary(binary, &current, &value, ignorecase)
}

pub(crate) fn set_index_value(base: &Value, index: &Value, value: Value) -> Result<()> {
    match base {
        Value::List(list) => {
            let number = ops::to_number(index)?;
            let mut list = list
                .try_borrow_mut()
                .map_err(|_| VimError::plain("List is already borrowed"))?;
            if list.locked {
                return Err(codes::e741("list"));
            }
            let position =
                normalize_index(number, list.items.len()).ok_or_else(|| codes::e684(number))?;
            list.items[position] = value;
            Ok(())
        }
        Value::Dict(dict) => {
            let key = key_string(index)?;
            let mut dict = dict
                .try_borrow_mut()
                .map_err(|_| VimError::plain("Dictionary is already borrowed"))?;
            if dict.locked || dict.entry_locked(&key) {
                return Err(codes::e741(&key.to_string_lossy()));
            }
            dict.set(key, value);
            Ok(())
        }
        Value::Blob(blob) => {
            let number = ops::to_number(index)?;
            let mut blob = blob
                .try_borrow_mut()
                .map_err(|_| VimError::plain("Blob is already borrowed"))?;
            let position =
                normalize_index(number, blob.len()).ok_or_else(|| codes::e684(number))?;
            let byte = ops::to_number(&value)?;
            if !(0..=255).contains(&byte) {
                return Err(VimError::new(979, "Blob value out of range"));
            }
            blob[position] = byte as u8;
            Ok(())
        }
        other => Err(VimError::new(
            715,
            format!("Cannot index-assign a {} value", other.type_name()),
        )),
    }
}

pub(crate) fn set_member_value(base: &Value, key: &VimString, value: Value) -> Result<()> {
    let Value::Dict(dict) = base else {
        return Err(VimError::new(
            715,
            "Dictionary required for member assignment",
        ));
    };
    let mut dict = dict
        .try_borrow_mut()
        .map_err(|_| VimError::plain("Dictionary is already borrowed"))?;
    if dict.locked || dict.entry_locked(key) {
        return Err(codes::e741(&key.to_string_lossy()));
    }
    dict.set(key.clone(), value);
    Ok(())
}

fn set_slice_value(base: &Value, start: Option<i64>, end: Option<i64>, value: Value) -> Result<()> {
    let items = value
        .as_list()
        .ok_or_else(|| VimError::new(714, "List required for slice assignment"))?;
    let items = items
        .try_borrow()
        .map(|list| list.items.clone())
        .unwrap_or_default();
    match base {
        Value::List(list) => {
            let mut list = list
                .try_borrow_mut()
                .map_err(|_| VimError::plain("List is already borrowed"))?;
            if list.locked {
                return Err(codes::e741("list"));
            }
            let (start, end) = slice_range(list.items.len(), start, end);
            list.items.splice(start..end, items);
            Ok(())
        }
        Value::Blob(blob) => {
            let mut bytes = Vec::with_capacity(items.len());
            for item in items {
                let number = ops::to_number(&item)?;
                if !(0..=255).contains(&number) {
                    return Err(VimError::new(979, "Blob value out of range"));
                }
                bytes.push(number as u8);
            }
            let mut blob = blob
                .try_borrow_mut()
                .map_err(|_| VimError::plain("Blob is already borrowed"))?;
            let (start, end) = slice_range(blob.len(), start, end);
            blob.splice(start..end, bytes);
            Ok(())
        }
        other => Err(VimError::new(
            714,
            format!("Cannot slice-assign a {} value", other.type_name()),
        )),
    }
}

/// Iterator values for :for.
pub(crate) fn iterable_values(value: &Value) -> Result<Vec<Value>> {
    match value {
        Value::List(list) => Ok(list
            .try_borrow()
            .map(|list| list.items.clone())
            .unwrap_or_default()),
        Value::Blob(blob) => Ok(blob
            .try_borrow()
            .map(|blob| {
                blob.iter()
                    .map(|byte| Value::Number(i64::from(*byte)))
                    .collect()
            })
            .unwrap_or_default()),
        Value::String(string) => {
            let mut values = Vec::new();
            let mut index = 0;
            while index < string.len() {
                let next = string.next_boundary(index);
                values.push(Value::String(VimString::from(
                    &string.as_bytes()[index..next],
                )));
                index = next;
            }
            Ok(values)
        }
        other => Err(VimError::new(
            714,
            format!(
                "List, String or Blob required for :for, found {}",
                other.type_name()
            ),
        )),
    }
}

pub(crate) fn throw_value(value: &Value) -> Result<VimError> {
    let text = ops::to_string(value)?;
    Ok(VimError::thrown(
        String::from_utf8_lossy(text.as_bytes()).into_owned(),
    ))
}

#[allow(dead_code)]
fn _keep_flow_import(_flow: Flow) {}

/// Calls a callable value (Funcref, Partial or lambda) with the given
/// arguments.
pub fn call_value(
    runtime: &mut VimRuntime,
    callee: &Value,
    args: Vec<Value>,
    host: &mut dyn VimHost,
) -> Result<Value> {
    let mut ctx = Ctx::new(runtime, host);
    call_funcref(&mut ctx, callee, args, None)
}
