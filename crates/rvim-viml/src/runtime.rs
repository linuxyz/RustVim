//! Interpreter state: globals, scripts, functions, call frames and predefined
//! `v:` variables.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use rvim_types::{Arena, ScriptId, VimString};

use crate::error::{codes, Result, VimError};
use crate::parser::Expr;
use crate::value::{
    dict_ref, ClosureScope, DictRef, SpecialValue, Value, VimDict, VIM_T_BLOB, VIM_T_BOOL,
    VIM_T_DICT, VIM_T_FLOAT, VIM_T_FUNC, VIM_T_LIST, VIM_T_NONE, VIM_T_NUMBER, VIM_T_STRING,
};

/// One parameter of a user function.
#[derive(Clone, Debug, PartialEq)]
pub struct FuncParam {
    pub name: VimString,
    pub default: Option<Expr>,
}

/// A raw function-body line, keeping its original script line number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuncLine {
    pub line: u32,
    pub text: Vec<u8>,
}

/// A user-defined VimL function.
#[derive(Clone)]
pub struct UserFunc {
    /// Canonical name, e.g. `Foo` or `<SNR>3_Bar`.
    pub name: VimString,
    /// Name as written, e.g. `s:Bar`.
    pub display_name: VimString,
    pub params: Vec<FuncParam>,
    pub varargs: bool,
    pub dict: bool,
    pub abort: bool,
    pub range: bool,
    pub closure: bool,
    pub body: Vec<FuncLine>,
    pub script_id: ScriptId,
    pub def_line: u32,
    pub closure_scope: Option<Rc<ClosureScope>>,
    /// Parsed body, filled lazily on first call (Vim reports body syntax errors
    /// at call time).
    pub cache: Rc<RefCell<Option<Rc<Vec<crate::script::Stmt>>>>>,
}

/// A loaded script and its `s:` dictionary.
pub struct ScriptContext {
    pub id: ScriptId,
    pub name: String,
    pub vars: DictRef,
}

/// One active function call.
#[derive(Clone)]
pub struct CallFrame {
    pub function: VimString,
    pub script_id: ScriptId,
    pub locals: DictRef,
    pub args: DictRef,
    pub closure: Option<Rc<ClosureScope>>,
    pub self_dict: Option<Value>,
    pub line: u32,
}

/// The complete interpreter state.
///
/// A single `VimRuntime` owns global variables, loaded scripts, user functions,
/// the call stack and `v:` variables. It is deliberately independent of the
/// editor; all editor access goes through `VimHost` and is passed to evaluation
/// separately.
pub struct VimRuntime {
    globals: DictRef,
    scripts: Arena<ScriptContext>,
    script_stack: Vec<ScriptId>,
    functions: BTreeMap<String, UserFunc>,
    frames: Vec<CallFrame>,
    vvars: VimDict,
    next_script_id: u64,
    maxfuncdepth: usize,
    capture: Vec<String>,
    last_error: Option<VimError>,
    source_depth: usize,
}

impl Default for VimRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl VimRuntime {
    pub fn new() -> Self {
        let mut runtime = Self {
            globals: dict_ref(VimDict::new()),
            scripts: Arena::new(),
            script_stack: Vec::new(),
            functions: BTreeMap::new(),
            frames: Vec::new(),
            vvars: VimDict::new(),
            next_script_id: 0,
            maxfuncdepth: 100,
            capture: Vec::new(),
            last_error: None,
            source_depth: 0,
        };
        runtime.init_vvars();
        runtime
    }

    fn init_vvars(&mut self) {
        let version = rvim_types::VIM_VERSION;
        let mut parts = version
            .split('.')
            .map(|part| part.parse::<i64>().unwrap_or(0));
        let versionlong = parts.next().unwrap_or(0) * 1_000_000
            + parts.next().unwrap_or(0) * 10_000
            + parts.next().unwrap_or(0);
        let numbers = [
            ("version", Value::Number(902)),
            ("versionlong", Value::Number(versionlong)),
            ("vim_did_enter", Value::Number(1)),
            ("testing", Value::Number(0)),
            ("shell_error", Value::Number(0)),
            ("count", Value::Number(0)),
            ("count1", Value::Number(1)),
            ("lnum", Value::Number(0)),
            ("val", Value::Number(0)),
            ("key", Value::Number(0)),
            ("numbermax", Value::Number(i64::MAX)),
            ("numbermin", Value::Number(i64::MIN)),
            ("numbersize", Value::Number(64)),
            ("hlsearch", Value::Number(0)),
            ("searchforward", Value::Number(1)),
            ("profiling", Value::Number(0)),
            ("t_number", Value::Number(VIM_T_NUMBER)),
            ("t_string", Value::Number(VIM_T_STRING)),
            ("t_func", Value::Number(VIM_T_FUNC)),
            ("t_list", Value::Number(VIM_T_LIST)),
            ("t_dict", Value::Number(VIM_T_DICT)),
            ("t_float", Value::Number(VIM_T_FLOAT)),
            ("t_bool", Value::Number(VIM_T_BOOL)),
            ("t_none", Value::Number(VIM_T_NONE)),
            ("t_blob", Value::Number(VIM_T_BLOB)),
        ];
        for (name, value) in numbers {
            self.vvars.set_str(name, value);
        }
        let strings = [
            ("progname", "rvim"),
            ("progpath", "rvim"),
            ("errmsg", ""),
            ("statusmsg", ""),
            ("warningmsg", ""),
            ("exception", ""),
            ("throwpoint", ""),
            ("register", "\""),
            ("char", ""),
            ("termresponse", ""),
            ("servername", ""),
            ("scrollstart", ""),
            ("ctype", ""),
            ("collate", ""),
            ("imagebackend", "none"),
        ];
        for (name, value) in strings {
            self.vvars
                .set_str(name, Value::String(VimString::from(value)));
        }
        self.vvars.set_str("true", Value::Bool(true));
        self.vvars.set_str("false", Value::Bool(false));
        self.vvars
            .set_str("null", Value::Special(SpecialValue::Null));
        self.vvars
            .set_str("none", Value::Special(SpecialValue::None));
        self.vvars.set_str("oldfiles", Value::list(Vec::new()));
        self.vvars.set_str("argv", Value::list(Vec::new()));
        self.vvars
            .set_str("completed_item", Value::dict(VimDict::new()));
        self.vvars.set_str("event", Value::dict(VimDict::new()));
        self.vvars
            .set_str("exiting", Value::Special(SpecialValue::Null));
    }

    /// The live global dictionary (g:). Mutations are visible to the runtime.
    pub fn globals(&self) -> DictRef {
        self.globals.clone()
    }

    pub fn get_global(&self, name: &str) -> Option<Value> {
        self.globals
            .try_borrow()
            .ok()
            .and_then(|globals| globals.get_str(name).cloned())
    }

    pub fn set_global(&mut self, name: &str, value: Value) {
        if let Ok(mut globals) = self.globals.try_borrow_mut() {
            globals.set_str(name, value);
        }
    }

    pub fn global_locked(&self, name: &str) -> bool {
        let key = VimString::from(name);
        self.globals
            .try_borrow()
            .map(|globals| globals.entry_locked(&key) || globals.locked)
            .unwrap_or(false)
    }

    pub fn set_global_checked(&mut self, name: &str, value: Value) -> Result<()> {
        let mut globals = self
            .globals
            .try_borrow_mut()
            .map_err(|_| VimError::plain("Global dictionary is already borrowed"))?;
        if globals.entry_locked(&VimString::from(name)) || globals.locked {
            return Err(codes::e741(name));
        }
        globals.set_str(name, value);
        Ok(())
    }

    pub fn remove_global(&mut self, name: &str) -> Option<Value> {
        self.globals
            .try_borrow_mut()
            .ok()
            .and_then(|mut globals| globals.remove(&VimString::from(name)))
    }

    pub fn remove_global_checked(&mut self, name: &str) -> Result<Option<Value>> {
        let mut globals = self
            .globals
            .try_borrow_mut()
            .map_err(|_| VimError::plain("Global dictionary is already borrowed"))?;
        let key = VimString::from(name);
        if globals.entry_locked(&key) || globals.locked {
            return Err(codes::e741(name));
        }
        Ok(globals.remove(&key))
    }

    pub fn lock_global(&mut self, name: &str, locked: bool, depth: i32) -> Result<()> {
        let mut globals = self
            .globals
            .try_borrow_mut()
            .map_err(|_| VimError::plain("Global dictionary is already borrowed"))?;
        let key = VimString::from(name);
        let Some(entry) = globals.entries.iter_mut().find(|entry| entry.key == key) else {
            return Err(codes::e108(name));
        };
        entry.locked = locked;
        crate::value::lock_container(&entry.value, locked, depth);
        Ok(())
    }

    pub fn script_count(&self) -> usize {
        self.scripts.len()
    }

    pub fn push_script(&mut self, name: &str) -> ScriptId {
        self.next_script_id += 1;
        let id = ScriptId::from_raw((1u64 << 32) | self.next_script_id);
        let vars = dict_ref(VimDict::new());
        self.scripts.insert_at(
            id,
            ScriptContext {
                id,
                name: name.to_string(),
                vars,
            },
        );
        self.script_stack.push(id);
        id
    }

    pub fn pop_script(&mut self, id: ScriptId) {
        if self.script_stack.last() == Some(&id) {
            self.script_stack.pop();
        } else if let Some(position) = self.script_stack.iter().rposition(|current| *current == id)
        {
            self.script_stack.remove(position);
        }
    }

    pub fn current_script_id(&self) -> Option<ScriptId> {
        if let Some(frame) = self.frames.last() {
            if !frame.script_id.is_nil() {
                return Some(frame.script_id);
            }
        }
        self.script_stack.last().copied()
    }

    pub fn script_name(&self, id: ScriptId) -> Option<String> {
        self.scripts.get(id).map(|script| script.name.clone())
    }

    pub fn script_vars(&self, id: ScriptId) -> Option<DictRef> {
        self.scripts.get(id).map(|script| script.vars.clone())
    }

    pub fn script_ids(&self) -> Vec<ScriptId> {
        self.scripts.ids().collect()
    }

    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    pub fn get_function(&self, name: &str) -> Option<&UserFunc> {
        self.functions.get(name)
    }

    pub fn define_function(&mut self, function: UserFunc) -> Option<UserFunc> {
        self.functions.insert(function.name.to_string(), function)
    }

    pub fn remove_function(&mut self, name: &str) -> Option<UserFunc> {
        self.functions.remove(name)
    }

    pub fn function_names(&self) -> Vec<String> {
        self.functions.keys().cloned().collect()
    }

    pub fn frames(&self) -> &[CallFrame] {
        &self.frames
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn current_frame(&self) -> Option<&CallFrame> {
        self.frames.last()
    }

    pub fn current_frame_mut(&mut self) -> Option<&mut CallFrame> {
        self.frames.last_mut()
    }

    pub fn push_frame(&mut self, mut frame: CallFrame) {
        frame.line = 0;
        self.frames.push(frame);
    }

    pub fn pop_frame(&mut self) -> Option<CallFrame> {
        self.frames.pop()
    }

    pub fn set_current_line(&mut self, line: u32) {
        if let Some(frame) = self.frames.last_mut() {
            frame.line = line;
        }
    }

    pub fn current_line_number(&self) -> u32 {
        self.frames.last().map_or(1, |frame| frame.line)
    }

    pub fn current_function_name(&self) -> Option<String> {
        self.frames.last().map(|frame| frame.function.to_string())
    }

    pub fn maxfuncdepth(&self) -> usize {
        self.maxfuncdepth
    }

    pub fn set_maxfuncdepth(&mut self, depth: usize) {
        self.maxfuncdepth = depth.clamp(1, 1000);
    }

    pub fn get_vvar(&self, name: &str) -> Option<Value> {
        self.vvars.get_str(name).cloned()
    }

    pub fn set_vvar(&mut self, name: &str, value: Value) {
        self.vvars.set_str(name, value);
    }

    /// Set a `v:` variable from script code, enforcing read-only names.
    pub fn set_vvar_checked(&mut self, name: &str, value: Value) -> Result<()> {
        if !is_writable_vvar(name) {
            return Err(codes::e46(&format!("v:{name}")));
        }
        self.vvars.set_str(name, value);
        Ok(())
    }

    /// Remove a `v:` variable (not supported by Vim; used by map/filter
    /// scoped temporaries only through restore).
    pub fn vvar_dict(&self) -> &VimDict {
        &self.vvars
    }

    pub fn begin_capture(&mut self) {
        self.capture.push(String::new());
    }

    pub fn end_capture(&mut self) -> Option<String> {
        self.capture.pop()
    }

    pub fn append_capture(&mut self, text: &str) {
        if let Some(buffer) = self.capture.last_mut() {
            buffer.push_str(text);
        }
    }

    pub fn is_capturing(&self) -> bool {
        !self.capture.is_empty()
    }

    pub fn source_depth(&self) -> usize {
        self.source_depth
    }

    pub fn enter_source(&mut self) {
        self.source_depth += 1;
    }

    pub fn leave_source(&mut self) {
        self.source_depth = self.source_depth.saturating_sub(1);
    }

    pub fn set_last_error(&mut self, error: &VimError) {
        self.set_vvar("errmsg", Value::String(VimString::from(error.to_string())));
        self.last_error = Some(error.clone());
    }

    pub fn take_last_error(&mut self) -> Option<VimError> {
        self.last_error.take()
    }

    /// Run the cycle collector now. Safe between interpreter operations.
    pub fn collect_cycles(&mut self) {
        crate::gc::collect_cycles();
    }
}

/// `v:` names that scripts may assign.
pub fn is_writable_vvar(name: &str) -> bool {
    matches!(
        name,
        "errmsg"
            | "statusmsg"
            | "warningmsg"
            | "val"
            | "key"
            | "lnum"
            | "char"
            | "completed_item"
            | "event"
            | "count"
            | "count1"
            | "register"
            | "searchforward"
            | "hlsearch"
            | "profiling"
            | "shell_error"
            | "exiting"
    ) || name.starts_with("option_")
}

impl VimRuntime {
    /// Calls a VimL function by name (used by the editor bridge and tests).
    pub fn call(
        &mut self,
        name: &str,
        args: Vec<Value>,
        host: &mut dyn crate::host::VimHost,
    ) -> Result<Value> {
        crate::eval::call_function(name, args, self, host)
    }

    /// Executes a script with this runtime and the given host.
    pub fn execute(
        &mut self,
        source: &[u8],
        name: &str,
        host: &mut dyn crate::host::VimHost,
    ) -> Result<()> {
        crate::script::execute_script(source, name, self, host)
    }

    /// Evaluates an expression with this runtime and the given host.
    pub fn eval(&mut self, source: &[u8], host: &mut dyn crate::host::VimHost) -> Result<Value> {
        crate::eval::eval_expression(source, self, host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_vvars() {
        let runtime = VimRuntime::new();
        assert_eq!(runtime.get_vvar("version"), Some(Value::Number(902)));
        assert_eq!(runtime.get_vvar("true"), Some(Value::Bool(true)));
        assert!(matches!(
            runtime.get_vvar("null"),
            Some(Value::Special(SpecialValue::Null))
        ));
        assert!(!is_writable_vvar("version"));
        assert!(is_writable_vvar("errmsg"));
    }

    #[test]
    fn script_scope() {
        let mut runtime = VimRuntime::new();
        let id = runtime.push_script("/tmp/a.vim");
        assert_eq!(runtime.current_script_id(), Some(id));
        runtime
            .script_vars(id)
            .unwrap()
            .borrow_mut()
            .set_str("x", Value::Number(1));
        assert_eq!(
            runtime.script_vars(id).unwrap().borrow().get_str("x"),
            Some(&Value::Number(1))
        );
        runtime.pop_script(id);
        assert_eq!(runtime.current_script_id(), None);
    }
}
