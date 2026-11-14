//! The VimL value model: byte strings, numbers, floats, containers and
//! Funcrefs.
//!
//! Containers are shared, mutable and may contain cycles, matching Vim. Values
//! are cheap to clone (each clone aliases the same container) and strings are
//! byte arrays so NUL and invalid UTF-8 are preserved.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use rvim_types::{ScriptId, VimString};

use crate::gc::{register, GcTrace};
use crate::parser::LambdaBody;

pub type ListRef = Rc<RefCell<VimList>>;
pub type DictRef = Rc<RefCell<VimDict>>;
pub type BlobRef = Rc<RefCell<Vec<u8>>>;
pub type FuncrefRef = Rc<RefCell<FuncrefData>>;

/// `type()` constants.
pub const VIM_T_NUMBER: i64 = 0;
pub const VIM_T_STRING: i64 = 1;
pub const VIM_T_FUNC: i64 = 2;
pub const VIM_T_LIST: i64 = 3;
pub const VIM_T_DICT: i64 = 4;
pub const VIM_T_FLOAT: i64 = 5;
pub const VIM_T_BOOL: i64 = 6;
pub const VIM_T_NONE: i64 = 7;
pub const VIM_T_BLOB: i64 = 10;

/// The two special values `v:null` and `v:none`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecialValue {
    Null,
    None,
}

impl fmt::Display for SpecialValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpecialValue::Null => f.write_str("v:null"),
            SpecialValue::None => f.write_str("v:none"),
        }
    }
}

/// A mutable Vim list. The insertion order is stable.
#[derive(Clone, Debug, Default)]
pub struct VimList {
    pub items: Vec<Value>,
    pub locked: bool,
}

impl VimList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_items(items: Vec<Value>) -> Self {
        Self {
            items,
            locked: false,
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl GcTrace for RefCell<VimList> {
    fn trace(&self) -> Vec<Value> {
        match self.try_borrow() {
            Ok(list) => list.items.clone(),
            Err(_) => Vec::new(),
        }
    }

    fn clear(&self) {
        if let Ok(mut list) = self.try_borrow_mut() {
            list.items.clear();
            list.locked = false;
        }
    }
}

/// One dictionary entry. Insertion order is preserved; `locked` implements
/// `:lockvar` and `:const`.
#[derive(Clone, Debug)]
pub struct DictEntry {
    pub key: VimString,
    pub value: Value,
    pub locked: bool,
}

/// A mutable Vim dictionary with deterministic insertion order.
#[derive(Clone, Debug, Default)]
pub struct VimDict {
    pub entries: Vec<DictEntry>,
    pub locked: bool,
}

impl VimDict {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &VimString) -> Option<&Value> {
        self.entries
            .iter()
            .find(|entry| entry.key == *key)
            .map(|entry| &entry.value)
    }

    pub fn get_mut(&mut self, key: &VimString) -> Option<&mut Value> {
        self.entries
            .iter_mut()
            .find(|entry| entry.key == *key)
            .map(|entry| &mut entry.value)
    }

    pub fn get_str(&self, key: &str) -> Option<&Value> {
        self.entries
            .iter()
            .find(|entry| entry.key.as_bytes() == key.as_bytes())
            .map(|entry| &entry.value)
    }

    /// Insert or replace a value, preserving the position of an existing key.
    pub fn set(&mut self, key: VimString, value: Value) -> Option<Value> {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.key == key) {
            let old = std::mem::replace(&mut entry.value, value);
            return Some(old);
        }
        self.entries.push(DictEntry {
            key,
            value,
            locked: false,
        });
        None
    }

    pub fn set_str(&mut self, key: &str, value: Value) -> Option<Value> {
        self.set(VimString::from(key), value)
    }

    pub fn contains_key(&self, key: &VimString) -> bool {
        self.entries.iter().any(|entry| entry.key == *key)
    }

    pub fn remove(&mut self, key: &VimString) -> Option<Value> {
        let index = self.entries.iter().position(|entry| entry.key == *key)?;
        Some(self.entries.remove(index).value)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn keys(&self) -> Vec<VimString> {
        self.entries.iter().map(|entry| entry.key.clone()).collect()
    }

    pub fn values(&self) -> Vec<Value> {
        self.entries
            .iter()
            .map(|entry| entry.value.clone())
            .collect()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn entry_locked(&self, key: &VimString) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.key == *key && entry.locked)
    }

    pub fn set_locked(&mut self, key: &VimString, locked: bool) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.key == *key) {
            entry.locked = locked;
        }
    }
}

impl GcTrace for RefCell<VimDict> {
    fn trace(&self) -> Vec<Value> {
        match self.try_borrow() {
            Ok(dict) => dict
                .entries
                .iter()
                .map(|entry| entry.value.clone())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    fn clear(&self) {
        if let Ok(mut dict) = self.try_borrow_mut() {
            dict.entries.clear();
            dict.locked = false;
        }
    }
}

pub fn list_ref(items: Vec<Value>) -> ListRef {
    let object = Rc::new(RefCell::new(VimList::from_items(items)));
    register(&object);
    object
}

pub fn dict_ref(dict: VimDict) -> DictRef {
    let object = Rc::new(RefCell::new(dict));
    register(&object);
    object
}

pub fn blob_ref(bytes: Vec<u8>) -> BlobRef {
    Rc::new(RefCell::new(bytes))
}

/// Recursively lock or unlock a container for \`:lockvar\`/\`:unlockvar\`.
pub fn lock_container(value: &Value, locked: bool, depth: i32) {
    if depth <= 0 {
        return;
    }
    match value {
        Value::List(list) => {
            if let Ok(mut list) = list.try_borrow_mut() {
                list.locked = locked;
                if depth > 1 {
                    let items = list.items.clone();
                    drop(list);
                    for item in &items {
                        lock_container(item, locked, depth - 1);
                    }
                }
            }
        }
        Value::Dict(dict) => {
            if let Ok(mut dict) = dict.try_borrow_mut() {
                dict.locked = locked;
                if depth > 1 {
                    let values = dict.values();
                    drop(dict);
                    for value in &values {
                        lock_container(value, locked, depth - 1);
                    }
                }
            }
        }
        _ => {}
    }
}

pub fn funcref_ref(data: FuncrefData) -> FuncrefRef {
    let object = Rc::new(RefCell::new(data));
    register(&object);
    object
}

/// Captured `l:` and `a:` dictionaries of an enclosing function. The chain is
/// used by lambdas and `:function ... closure` functions.
#[derive(Clone)]
pub struct ClosureScope {
    pub locals: DictRef,
    pub args: DictRef,
    pub parent: Option<Rc<ClosureScope>>,
}

impl ClosureScope {
    pub fn new(locals: DictRef, args: DictRef, parent: Option<Rc<ClosureScope>>) -> Self {
        Self {
            locals,
            args,
            parent,
        }
    }

    fn push_dicts(&self, out: &mut Vec<Value>) {
        out.push(Value::Dict(self.locals.clone()));
        out.push(Value::Dict(self.args.clone()));
        if let Some(parent) = &self.parent {
            parent.push_dicts(out);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FuncrefKind {
    Builtin,
    User,
    Lambda,
    Partial,
}

/// A first-class function value: a name reference or an anonymous lambda, with
/// optionally bound arguments, a `self` dictionary and captured closure scope.
#[derive(Clone)]
pub struct FuncrefData {
    pub name: VimString,
    pub kind: FuncrefKind,
    pub args: Vec<Value>,
    pub dict: Option<Value>,
    pub script_id: ScriptId,
    pub closure: Option<Rc<ClosureScope>>,
    pub lambda: Option<Rc<LambdaBody>>,
}

impl FuncrefData {
    pub fn new(name: impl Into<VimString>, kind: FuncrefKind, script_id: ScriptId) -> Self {
        Self {
            name: name.into(),
            kind,
            args: Vec::new(),
            dict: None,
            script_id,
            closure: None,
            lambda: None,
        }
    }

    pub fn is_partial(&self) -> bool {
        !self.args.is_empty() || self.dict.is_some() || self.kind == FuncrefKind::Partial
    }
}

impl GcTrace for RefCell<FuncrefData> {
    fn trace(&self) -> Vec<Value> {
        let Ok(data) = self.try_borrow() else {
            return Vec::new();
        };
        let mut values = data.args.clone();
        if let Some(dict) = &data.dict {
            values.push(dict.clone());
        }
        if let Some(closure) = &data.closure {
            closure.push_dicts(&mut values);
        }
        values
    }

    fn clear(&self) {
        if let Ok(mut data) = self.try_borrow_mut() {
            data.args.clear();
            data.dict = None;
            data.closure = None;
            data.lambda = None;
        }
    }
}

/// A VimL value.
///
/// The variants mirror Vim's types. `List`, `Dict` and `Funcref` are shared
/// handles; cloning a `Value` does not copy the container.
#[derive(Clone)]
pub enum Value {
    Number(i64),
    Float(f64),
    String(VimString),
    List(ListRef),
    Dict(DictRef),
    Blob(BlobRef),
    Funcref(FuncrefRef),
    Bool(bool),
    Special(SpecialValue),
}

impl Value {
    pub fn number(value: i64) -> Self {
        Self::Number(value)
    }

    pub fn float(value: f64) -> Self {
        Self::Float(value)
    }

    pub fn string(value: impl Into<VimString>) -> Self {
        Self::String(value.into())
    }

    pub fn list(items: Vec<Value>) -> Self {
        Self::List(list_ref(items))
    }

    pub fn dict(dict: VimDict) -> Self {
        Self::Dict(dict_ref(dict))
    }

    pub fn dict_from_entries(entries: Vec<(VimString, Value)>) -> Self {
        let mut dict = VimDict::new();
        for (key, value) in entries {
            dict.set(key, value);
        }
        Self::dict(dict)
    }

    pub fn blob(bytes: Vec<u8>) -> Self {
        Self::Blob(blob_ref(bytes))
    }

    pub fn funcref(data: FuncrefData) -> Self {
        Self::Funcref(funcref_ref(data))
    }

    pub fn boolean(value: bool) -> Self {
        Self::Bool(value)
    }

    pub fn null() -> Self {
        Self::Special(SpecialValue::Null)
    }

    pub fn none() -> Self {
        Self::Special(SpecialValue::None)
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Number(_) => "number",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::List(_) => "list",
            Value::Dict(_) => "dict",
            Value::Blob(_) => "blob",
            Value::Funcref(_) => "func",
            Value::Bool(_) => "bool",
            Value::Special(_) => "none",
        }
    }

    pub fn type_number(&self) -> i64 {
        match self {
            Value::Number(_) => VIM_T_NUMBER,
            Value::Float(_) => VIM_T_FLOAT,
            Value::String(_) => VIM_T_STRING,
            Value::List(_) => VIM_T_LIST,
            Value::Dict(_) => VIM_T_DICT,
            Value::Blob(_) => VIM_T_BLOB,
            Value::Funcref(_) => VIM_T_FUNC,
            Value::Bool(_) => VIM_T_BOOL,
            Value::Special(_) => VIM_T_NONE,
        }
    }

    pub fn is_number(&self) -> bool {
        matches!(self, Value::Number(_))
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Value::Float(_))
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }

    pub fn is_list(&self) -> bool {
        matches!(self, Value::List(_))
    }

    pub fn is_dict(&self) -> bool {
        matches!(self, Value::Dict(_))
    }

    pub fn is_blob(&self) -> bool {
        matches!(self, Value::Blob(_))
    }

    pub fn is_funcref(&self) -> bool {
        matches!(self, Value::Funcref(_))
    }

    pub fn as_list(&self) -> Option<ListRef> {
        match self {
            Value::List(list) => Some(list.clone()),
            _ => None,
        }
    }

    pub fn as_dict(&self) -> Option<DictRef> {
        match self {
            Value::Dict(dict) => Some(dict.clone()),
            _ => None,
        }
    }

    pub fn as_blob(&self) -> Option<BlobRef> {
        match self {
            Value::Blob(blob) => Some(blob.clone()),
            _ => None,
        }
    }

    pub fn as_funcref(&self) -> Option<FuncrefRef> {
        match self {
            Value::Funcref(function) => Some(function.clone()),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&VimString> {
        match self {
            Value::String(string) => Some(string),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<i64> {
        match self {
            Value::Number(number) => Some(*number),
            Value::Bool(value) => Some(i64::from(*value)),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_special(&self) -> Option<SpecialValue> {
        match self {
            Value::Special(special) => Some(*special),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(number) => Some(*number as f64),
            Value::Float(float) => Some(*float),
            Value::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    /// Identity key used by the cycle collector and `is`/`isnot`.
    pub fn object_id(&self) -> Option<usize> {
        match self {
            Value::List(list) => Some(Rc::as_ptr(list) as *const () as usize),
            Value::Dict(dict) => Some(Rc::as_ptr(dict) as *const () as usize),
            Value::Funcref(function) => Some(Rc::as_ptr(function) as *const () as usize),
            _ => None,
        }
    }

    /// Vim's `string()` representation, safe in the presence of cycles.
    pub fn string_repr(&self) -> String {
        let mut seen = Vec::new();
        format_value(self, true, &mut seen, 0)
    }

    /// Vim's `echo` representation. A top-level String is not quoted.
    pub fn echo_string(&self) -> String {
        match self {
            Value::String(string) => String::from_utf8_lossy(string.as_bytes()).into_owned(),
            _ => self.string_repr(),
        }
    }

    /// Funcref identity used by `is` and equality.
    pub fn same_object(&self, other: &Value) -> bool {
        match (self.object_id(), other.object_id()) {
            (Some(left), Some(right)) => left == right,
            _ => self == other,
        }
    }
}

fn quoted_string(string: &VimString) -> String {
    let control = string
        .as_bytes()
        .iter()
        .any(|byte| *byte < 0x20 || *byte == 0x7f);
    if control {
        let mut out = String::from("\"");
        for &byte in string.as_bytes() {
            match byte {
                b'\n' => out.push_str("\\n"),
                b'\r' => out.push_str("\\r"),
                b'\t' => out.push_str("\\t"),
                0x1b => out.push_str("\\e"),
                b'\\' => out.push_str("\\\\"),
                b'"' => out.push_str("\\\""),
                0x20..=0x7e => out.push(byte as char),
                _ => out.push_str(&format!("\\x{byte:02x}")),
            }
        }
        out.push('"');
        out
    } else {
        let text = String::from_utf8_lossy(string.as_bytes());
        let mut out = String::from("'");
        for character in text.chars() {
            if character == '\'' {
                out.push_str("''");
            } else {
                out.push(character);
            }
        }
        out.push('\'');
        out
    }
}

fn format_value(value: &Value, quote_strings: bool, seen: &mut Vec<usize>, depth: usize) -> String {
    if depth > 64 {
        return "...".to_string();
    }
    match value {
        Value::Number(number) => number.to_string(),
        Value::Float(float) => format_float(*float),
        Value::String(string) => {
            if quote_strings {
                quoted_string(string)
            } else {
                String::from_utf8_lossy(string.as_bytes()).into_owned()
            }
        }
        Value::List(list) => {
            let id = Rc::as_ptr(list) as *const () as usize;
            if seen.contains(&id) {
                return "[...]".to_string();
            }
            seen.push(id);
            let items = match list.try_borrow() {
                Ok(list) => list.items.clone(),
                Err(_) => Vec::new(),
            };
            let rendered: Vec<String> = items
                .iter()
                .map(|item| format_value(item, true, seen, depth + 1))
                .collect();
            seen.pop();
            format!("[{}]", rendered.join(", "))
        }
        Value::Dict(dict) => {
            let id = Rc::as_ptr(dict) as *const () as usize;
            if seen.contains(&id) {
                return "{...}".to_string();
            }
            seen.push(id);
            let entries = match dict.try_borrow() {
                Ok(dict) => dict.entries.clone(),
                Err(_) => Vec::new(),
            };
            let rendered: Vec<String> = entries
                .iter()
                .map(|entry| {
                    format!(
                        "{}: {}",
                        quoted_string(&entry.key),
                        format_value(&entry.value, true, seen, depth + 1)
                    )
                })
                .collect();
            seen.pop();
            format!("{{{}}}", rendered.join(", "))
        }
        Value::Blob(blob) => {
            let bytes = match blob.try_borrow() {
                Ok(bytes) => bytes.clone(),
                Err(_) => Vec::new(),
            };
            let mut out = String::from("0z");
            for byte in bytes {
                out.push_str(&format!("{byte:02X}"));
            }
            out
        }
        Value::Funcref(function) => {
            let id = Rc::as_ptr(function) as *const () as usize;
            if seen.contains(&id) {
                return "function('...')".to_string();
            }
            seen.push(id);
            let data = match function.try_borrow() {
                Ok(data) => data.clone(),
                Err(_) => {
                    seen.pop();
                    return "function('?')".to_string();
                }
            };
            let name = String::from_utf8_lossy(data.name.as_bytes()).into_owned();
            let rendered = if data.args.is_empty() && data.dict.is_none() {
                format!("function({})", quoted_string(&VimString::from(name)))
            } else {
                let args: Vec<String> = data
                    .args
                    .iter()
                    .map(|arg| format_value(arg, true, seen, depth + 1))
                    .collect();
                if let Some(dict) = &data.dict {
                    format!(
                        "function({}, [{}], {})",
                        quoted_string(&VimString::from(name)),
                        args.join(", "),
                        format_value(dict, true, seen, depth + 1)
                    )
                } else {
                    format!(
                        "function({}, [{}])",
                        quoted_string(&VimString::from(name)),
                        args.join(", ")
                    )
                }
            };
            seen.pop();
            rendered
        }
        Value::Bool(value) => {
            if *value {
                "v:true".to_string()
            } else {
                "v:false".to_string()
            }
        }
        Value::Special(special) => special.to_string(),
    }
}

/// Format a float using Vim's `%g`-like six-significant-digit style, always
/// keeping a decimal point so the result reads as a Float.
pub fn format_float(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-inf".to_string()
        } else {
            "inf".to_string()
        };
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0".to_string()
        } else {
            "0.0".to_string()
        };
    }
    let scientific = format!("{value:.5e}");
    let Some((mantissa, exponent)) = scientific.split_once('e') else {
        return value.to_string();
    };
    let Ok(exponent) = exponent.parse::<i32>() else {
        return value.to_string();
    };
    if !(-4..6).contains(&exponent) {
        let mut mantissa = mantissa
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string();
        if !mantissa.contains('.') {
            mantissa.push_str(".0");
        }
        return format!("{mantissa}e{exponent}");
    }
    let decimals = (5 - exponent).max(0) as usize;
    let fixed = format!("{value:.decimals$}");
    let trimmed = fixed.trim_end_matches('0').trim_end_matches('.');
    if trimmed.contains('.') {
        trimmed.to_string()
    } else {
        format!("{trimmed}.0")
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.echo_string())
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.string_repr())
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        let mut seen = Vec::new();
        value_eq(self, other, &mut seen)
    }
}

fn value_eq(left: &Value, right: &Value, seen: &mut Vec<(usize, usize)>) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left == right,
        (Value::Float(left), Value::Float(right)) => left == right,
        (Value::Number(left), Value::Float(right)) | (Value::Float(right), Value::Number(left)) => {
            *left as f64 == *right
        }
        (Value::Bool(left), Value::Bool(right)) => left == right,
        (Value::Bool(left), Value::Number(right)) | (Value::Number(right), Value::Bool(left)) => {
            i64::from(*left) == *right
        }
        (Value::Special(left), Value::Special(right)) => {
            left == right || (*left == SpecialValue::Null && *right == SpecialValue::None)
        }
        (Value::String(left), Value::String(right)) => left == right,
        (Value::Blob(left), Value::Blob(right)) => {
            Rc::ptr_eq(left, right)
                || match (left.try_borrow(), right.try_borrow()) {
                    (Ok(left), Ok(right)) => *left == *right,
                    _ => false,
                }
        }
        (Value::List(left), Value::List(right)) => {
            if Rc::ptr_eq(left, right) {
                return true;
            }
            let pair = (Rc::as_ptr(left) as usize, Rc::as_ptr(right) as usize);
            if seen.contains(&pair) {
                return true;
            }
            seen.push(pair);
            let result = match (left.try_borrow(), right.try_borrow()) {
                (Ok(left), Ok(right)) => {
                    left.items.len() == right.items.len()
                        && left
                            .items
                            .iter()
                            .zip(right.items.iter())
                            .all(|(left, right)| value_eq(left, right, seen))
                }
                _ => false,
            };
            seen.pop();
            result
        }
        (Value::Dict(left), Value::Dict(right)) => {
            if Rc::ptr_eq(left, right) {
                return true;
            }
            let pair = (Rc::as_ptr(left) as usize, Rc::as_ptr(right) as usize);
            if seen.contains(&pair) {
                return true;
            }
            seen.push(pair);
            let result = match (left.try_borrow(), right.try_borrow()) {
                (Ok(left), Ok(right)) => {
                    left.entries.len() == right.entries.len()
                        && left.entries.iter().all(|entry| {
                            right
                                .get(&entry.key)
                                .is_some_and(|value| value_eq(&entry.value, value, seen))
                        })
                }
                _ => false,
            };
            seen.pop();
            result
        }
        (Value::Funcref(left), Value::Funcref(right)) => {
            if Rc::ptr_eq(left, right) {
                return true;
            }
            match (left.try_borrow(), right.try_borrow()) {
                (Ok(left), Ok(right)) => {
                    left.name == right.name
                        && left.kind == right.kind
                        && left.args == right.args
                        && left.dict == right.dict
                }
                _ => false,
            }
        }
        _ => false,
    }
}
