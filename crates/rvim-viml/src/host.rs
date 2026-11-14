//! The bridge between the interpreter and the editor.

use rvim_types::{ScriptId, VimString};

use crate::error::{codes, Result, VimError};
use crate::value::{Value, VimDict};

/// Variable scopes that can cross the host boundary. The runtime owns
/// `Global`, `Local`, `Argument`, `Script` and `Vim`; hosts are asked for
/// `Buffer`, `Window` and `Tab`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VarScope {
    Global,
    Buffer,
    Window,
    Tab,
    Local,
    Argument,
    Script,
    Vim,
}

impl VarScope {
    pub fn prefix(self) -> &'static str {
        match self {
            VarScope::Global => "g:",
            VarScope::Buffer => "b:",
            VarScope::Window => "w:",
            VarScope::Tab => "t:",
            VarScope::Local => "l:",
            VarScope::Argument => "a:",
            VarScope::Script => "s:",
            VarScope::Vim => "v:",
        }
    }
}

/// Scope of an option access (`&opt`, `&g:opt`, `&l:opt`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OptionScope {
    /// `&opt`: the host chooses the effective scope.
    Default,
    Global,
    Local,
}

/// Context passed with `:execute`/`:source`-style host commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExContext {
    pub script_id: ScriptId,
    pub script_name: String,
    pub line: u32,
    pub function: Option<String>,
}

impl ExContext {
    pub fn new(
        script_id: ScriptId,
        script_name: impl Into<String>,
        line: u32,
        function: Option<String>,
    ) -> Self {
        Self {
            script_id,
            script_name: script_name.into(),
            line,
            function,
        }
    }
}

/// Editor services used by the script engine.
///
/// Every method has a default that either implements a safe fallback (messages
/// are ignored, environment access uses the process environment) or returns a
/// clear `host does not support ...` error. The trait is object-safe so the
/// interpreter can hold `&mut dyn VimHost`.
pub trait VimHost {
    fn get_var(&mut self, scope: VarScope, name: &str) -> Result<Value> {
        Err(unsupported(&format!("reading {}{}", scope.prefix(), name)))
    }

    fn set_var(&mut self, scope: VarScope, name: &str, value: Value) -> Result<()> {
        let _ = value;
        Err(unsupported(&format!("writing {}{}", scope.prefix(), name)))
    }

    fn delete_var(&mut self, scope: VarScope, name: &str) -> Result<()> {
        Err(unsupported(&format!("deleting {}{}", scope.prefix(), name)))
    }

    /// Return a whole external scope (b:, w: or t:) as a Dictionary value.
    fn get_scope_dict(&mut self, scope: VarScope) -> Result<Value> {
        Err(unsupported(&format!(
            "reading the whole {} dictionary",
            scope.prefix()
        )))
    }

    fn get_option(&mut self, scope: OptionScope, name: &str) -> Result<Value> {
        let prefix = match scope {
            OptionScope::Default => "",
            OptionScope::Global => "g:",
            OptionScope::Local => "l:",
        };
        Err(unsupported(&format!("reading option &{prefix}{name}")))
    }

    fn set_option(&mut self, scope: OptionScope, name: &str, value: Value) -> Result<()> {
        let _ = value;
        let prefix = match scope {
            OptionScope::Default => "",
            OptionScope::Global => "g:",
            OptionScope::Local => "l:",
        };
        Err(unsupported(&format!("setting option &{prefix}{name}")))
    }

    fn get_env(&mut self, name: &str) -> Result<Value> {
        match std::env::var_os(name) {
            Some(value) => Ok(Value::String(VimString::from(
                value.to_string_lossy().into_owned(),
            ))),
            None => Ok(Value::String(VimString::new())),
        }
    }

    fn set_env(&mut self, name: &str, value: &VimString) -> Result<()> {
        std::env::set_var(name, String::from_utf8_lossy(value.as_bytes()).as_ref());
        Ok(())
    }

    /// Whether an environment variable is set, even when it is empty.
    fn env_exists(&mut self, name: &str) -> bool {
        std::env::var_os(name).is_some()
    }

    /// Whether the editor implements an Ex command (used by exists(':cmd')).
    fn has_command(&mut self, name: &str) -> bool {
        let _ = name;
        false
    }

    fn get_register(&mut self, name: char) -> Result<Value> {
        Err(unsupported(&format!("reading register @{name}")))
    }

    fn set_register(&mut self, name: char, value: Value) -> Result<()> {
        let _ = value;
        Err(unsupported(&format!("writing register @{name}")))
    }

    fn get_register_list(&mut self, name: char, as_list: bool) -> Result<Value> {
        let _ = as_list;
        self.get_register(name)
    }

    fn set_register_ex(&mut self, name: char, value: Value, options: &str) -> Result<()> {
        let _ = options;
        self.set_register(name, value)
    }

    /// Display a message.
    fn message(&mut self, text: &str) {
        let _ = text;
    }

    /// Display an error. Defaults to a normal message.
    fn error(&mut self, error: &VimError) {
        let text = error.to_string();
        self.message(&text);
    }

    /// Run an Ex command that the VimL engine does not implement itself.
    /// Expand a line that invokes an editor user command.
    ///
    /// Returns the replacement text and the script id the command was defined
    /// in (if any), so the caller can run it in the defining script's `s:`
    /// context instead of creating a nested script context.
    fn user_command_expansion(
        &mut self,
        _line: &str,
    ) -> Result<Option<(String, Option<ScriptId>)>> {
        Ok(None)
    }

    /// Highlight group id for `hlID()`; 0 when the group is not defined.
    fn highlight_id(&mut self, _name: &str) -> Result<i64> {
        Ok(0)
    }

    /// indent(): width in spaces of the leading white space of line `lnum`.
    fn indent(&mut self, _lnum: i64) -> Result<i64> {
        Ok(-1)
    }

    /// foldclosed()/foldclosedend(): start/end of the closed fold containing
    /// `lnum` in window `winid` (the current window when None), or -1.
    fn fold_closed(&mut self, _lnum: i64, _winid: Option<u64>) -> Result<i64> {
        Ok(-1)
    }

    fn fold_closed_end(&mut self, _lnum: i64, _winid: Option<u64>) -> Result<i64> {
        Ok(-1)
    }

    /// foldlevel(): fold level of `lnum` (0 when not in a fold).
    fn fold_level(&mut self, _lnum: i64, _winid: Option<u64>) -> Result<i64> {
        Ok(0)
    }

    /// foldtext(): default fold text for the v:fold* variables.
    fn fold_text(&mut self) -> Result<String> {
        Ok(String::new())
    }

    /// foldtextresult(): the fold text of the fold containing `lnum`.
    fn fold_text_result(&mut self, _lnum: i64, _winid: Option<u64>) -> Result<String> {
        Ok(String::new())
    }

    /// Standard output of a shell command, used by `system()`/`systemlist()`.
    fn system_output(&mut self, _command: &str) -> Result<Vec<u8>> {
        Err(unsupported("system()"))
    }

    /// Files matched by a `:runtime` / `:runtime!` pattern: (name, bytes).
    fn runtime_files(&mut self, _pattern: &str, _all: bool) -> Result<Vec<(String, Vec<u8>)>> {
        Ok(Vec::new())
    }

    /// Bytes of the autoload script that defines `prefix#...` names.
    fn autoload_script(&mut self, _prefix: &str) -> Result<Option<(String, Vec<u8>)>> {
        Ok(None)
    }

    fn execute_ex(&mut self, command: &str, context: &ExContext) -> Result<Value> {
        let _ = context;
        Err(codes::e492(command))
    }

    /// Let the editor provide extra (host) functions. `Ok(None)` means the
    /// function is not known to the host.
    fn invoke_function(&mut self, name: &str, args: &[Value]) -> Result<Option<Value>> {
        let _ = (name, args);
        Ok(None)
    }

    /// Interpreter-facing hook for host functions. Hosts can override either
    /// this method or `invoke_function`; the interpreter calls this one.
    fn call_function(&mut self, name: &str, args: &[Value]) -> Result<Option<Value>> {
        self.invoke_function(name, args)
    }

    fn function_exists(&mut self, name: &str) -> bool {
        let _ = name;
        false
    }

    fn get_line(&mut self, lnum: i64) -> Result<VimString> {
        Err(unsupported(&format!("getline({lnum})")))
    }

    fn set_line(&mut self, lnum: i64, text: &VimString) -> Result<()> {
        let _ = (lnum, text);
        Err(unsupported("setline()"))
    }

    fn line_count(&mut self) -> Result<i64> {
        Err(unsupported("line_count()"))
    }

    fn current_line(&mut self) -> Result<i64> {
        Ok(1)
    }

    /// Evaluate an expression in a host-selected context.
    fn eval_expr(&mut self, expression: &str) -> Result<Value> {
        Err(unsupported(&format!(
            "evaluating host expression {expression:?}"
        )))
    }

    /// Read a script file for `:source`. The default reads the filesystem.
    fn read_file(&mut self, path: &str) -> Result<Vec<u8>> {
        std::fs::read(path)
            .map_err(|error| VimError::new(484, format!("Can't open file {path}: {error}")))
    }
}

fn unsupported(feature: &str) -> VimError {
    VimError::plain(format!("host does not support {feature}"))
}

/// A minimal in-memory host useful for tests and embedding.
#[derive(Default)]
pub struct BasicHost {
    pub variables: std::collections::HashMap<(VarScope, String), Value>,
    pub options: std::collections::HashMap<(OptionScope, String), Value>,
    pub registers: std::collections::HashMap<char, Value>,
    pub lines: Vec<VimString>,
    pub messages: Vec<String>,
    pub errors: Vec<String>,
    pub executed: Vec<String>,
    pub current_line: i64,
}

impl BasicHost {
    pub fn new() -> Self {
        Self {
            current_line: 1,
            ..Self::default()
        }
    }
}

impl VimHost for BasicHost {
    fn get_var(&mut self, scope: VarScope, name: &str) -> Result<Value> {
        self.variables
            .get(&(scope, name.to_string()))
            .cloned()
            .ok_or_else(|| codes::e121(name))
    }

    fn set_var(&mut self, scope: VarScope, name: &str, value: Value) -> Result<()> {
        self.variables.insert((scope, name.to_string()), value);
        Ok(())
    }

    fn delete_var(&mut self, scope: VarScope, name: &str) -> Result<()> {
        self.variables.remove(&(scope, name.to_string()));
        Ok(())
    }

    fn get_scope_dict(&mut self, scope: VarScope) -> Result<Value> {
        let mut dict = VimDict::new();
        for ((entry_scope, name), value) in &self.variables {
            if *entry_scope == scope {
                dict.set(VimString::from(name.as_str()), value.clone());
            }
        }
        Ok(Value::dict(dict))
    }

    fn get_option(&mut self, scope: OptionScope, name: &str) -> Result<Value> {
        if let Some(value) = self.options.get(&(scope, name.to_string())) {
            return Ok(value.clone());
        }
        if scope == OptionScope::Default {
            if let Some(value) = self.options.get(&(OptionScope::Global, name.to_string())) {
                return Ok(value.clone());
            }
            if let Some(value) = self.options.get(&(OptionScope::Local, name.to_string())) {
                return Ok(value.clone());
            }
        }
        Err(VimError::new(201, format!("Unknown option: {name}")))
    }

    fn set_option(&mut self, scope: OptionScope, name: &str, value: Value) -> Result<()> {
        self.options.insert((scope, name.to_string()), value);
        Ok(())
    }

    fn get_register(&mut self, name: char) -> Result<Value> {
        Ok(self
            .registers
            .get(&name)
            .cloned()
            .unwrap_or_else(|| Value::String(VimString::new())))
    }

    fn set_register(&mut self, name: char, value: Value) -> Result<()> {
        self.registers.insert(name, value);
        Ok(())
    }

    fn message(&mut self, text: &str) {
        self.messages.push(text.to_string());
    }

    fn error(&mut self, error: &VimError) {
        self.errors.push(error.to_string());
    }

    fn execute_ex(&mut self, command: &str, _context: &ExContext) -> Result<Value> {
        self.executed.push(command.to_string());
        Ok(Value::Number(0))
    }

    fn get_line(&mut self, lnum: i64) -> Result<VimString> {
        let index = lnum - 1;
        if index < 0 || index as usize >= self.lines.len() {
            return Ok(VimString::new());
        }
        Ok(self.lines[index as usize].clone())
    }

    fn set_line(&mut self, lnum: i64, text: &VimString) -> Result<()> {
        let index = lnum - 1;
        if index < 0 {
            return Err(VimError::new(16, "E16: Invalid range"));
        }
        let index = index as usize;
        while self.lines.len() <= index {
            self.lines.push(VimString::new());
        }
        self.lines[index] = text.clone();
        Ok(())
    }

    fn line_count(&mut self) -> Result<i64> {
        Ok(self.lines.len() as i64)
    }

    fn current_line(&mut self) -> Result<i64> {
        Ok(self.current_line)
    }
}
