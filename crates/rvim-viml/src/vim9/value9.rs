//! Values used by the Vim9 evaluator.
//!
//! Most values reuse the legacy VimL [Value] representation.  Vim9 adds one
//! value kind: tuples.  They are kept in this wrapper so the legacy evaluator
//! and its many match sites do not need a new Value variant; converting a
//! tuple to a legacy value (for a builtin that does not understand tuples)
//! degrades it to a list.

use rvim_types::{ScriptId, VimString};

use crate::error::{Result, VimError};
use crate::value::{FuncrefData, FuncrefKind, SpecialValue, Value};

/// A Vim9 value: either a legacy value or a tuple.
#[derive(Clone, Debug)]
pub enum Value9 {
    Val(Value),
    Tuple(Vec<Value9>),
}

/// Prefix of the synthetic funcref names that point at Vim9 callables.
pub const FUNCREF_PREFIX: &[u8] = b"\x01vim9:";
pub const CLASS_PREFIX: &[u8] = b"\x01vim9c:";
pub const OBJECT_PREFIX: &[u8] = b"\x01vim9o:";
pub const ENUM_PREFIX: &[u8] = b"\x01vim9e:";
pub const ENUM_VALUE_PREFIX: &[u8] = b"\x01vim9v:";

impl Value9 {
    pub fn number(value: i64) -> Self {
        Value9::Val(Value::Number(value))
    }

    pub fn from_string(value: impl Into<VimString>) -> Self {
        Value9::Val(Value::String(value.into()))
    }

    pub fn null() -> Self {
        Value9::Val(Value::Special(SpecialValue::Null))
    }

    pub fn none() -> Self {
        Value9::Val(Value::Special(SpecialValue::None))
    }

    pub fn list(items: Vec<Value9>) -> Self {
        Value9::Val(Value::list(items.iter().map(Value9::legacy).collect()))
    }

    /// Convert to a legacy value.  Tuples degrade to lists.
    pub fn legacy(&self) -> Value {
        match self {
            Value9::Val(value) => value.clone(),
            Value9::Tuple(items) => Value::list(items.iter().map(Value9::legacy).collect()),
        }
    }

    pub fn as_legacy(&self) -> Option<&Value> {
        match self {
            Value9::Val(value) => Some(value),
            Value9::Tuple(_) => None,
        }
    }

    pub fn as_string(&self) -> Option<VimString> {
        match self {
            Value9::Val(Value::String(string)) => Some(string.clone()),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<i64> {
        match self {
            Value9::Val(value) => value.as_number(),
            Value9::Tuple(_) => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value9::Val(Value::Float(value)) => Some(*value),
            Value9::Val(Value::Number(value)) => Some(*value as f64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value9::Val(Value::Bool(value)) => Some(*value),
            Value9::Val(Value::Number(value)) => Some(*value != 0),
            _ => None,
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value9::Val(value) => match value {
                Value::Bool(boolean) => *boolean,
                Value::Number(number) => *number != 0,
                Value::Float(float) => *float != 0.0,
                Value::String(string) => !string.is_empty(),
                Value::List(list) => !list.borrow().items.is_empty(),
                Value::Dict(dict) => !dict.borrow().entries.is_empty(),
                Value::Blob(blob) => !blob.borrow().is_empty(),
                Value::Special(_) => false,
                Value::Funcref(data) => !data.borrow().name.is_empty(),
            },
            Value9::Tuple(items) => !items.is_empty(),
        }
    }

    pub fn type_number(&self) -> i64 {
        match self {
            Value9::Val(value) => value.type_number(),
            // v:t_tuple (Vim 9.1+).
            Value9::Tuple(_) => 17,
        }
    }

    /// The string form used by ":echo", ":echomsg" and ".." conversion.
    pub fn display(&self) -> String {
        match self {
            Value9::Val(value) => display_legacy(value),
            Value9::Tuple(items) => tuple_string(items, false),
        }
    }

    /// The quoted form used by string().
    pub fn string_repr(&self) -> String {
        match self {
            Value9::Val(value) => repr_legacy(value),
            Value9::Tuple(items) => tuple_string(items, true),
        }
    }

    /// Convert to a dictionary key string.
    pub fn to_key_string(&self) -> Result<VimString> {
        match self {
            Value9::Val(Value::String(string)) => Ok(string.clone()),
            Value9::Val(Value::Number(number)) => Ok(VimString::from(number.to_string())),
            Value9::Val(Value::Bool(boolean)) => {
                Ok(VimString::from(if *boolean { "true" } else { "false" }))
            }
            Value9::Val(Value::Float(float)) => {
                Ok(VimString::from(crate::value::format_float(*float)))
            }
            Value9::Val(Value::Special(_)) => Ok(VimString::from("null")),
            _ => Err(VimError::new(730, "E730: Using a List as a String")),
        }
    }
}

fn display_legacy(value: &Value) -> String {
    match value {
        Value::Bool(boolean) => {
            if *boolean {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Value::Special(SpecialValue::Null | SpecialValue::None) => "null".to_string(),
        Value::String(string) => String::from_utf8_lossy(string.as_bytes()).into_owned(),
        _ => value.echo_string(),
    }
}

fn repr_legacy(value: &Value) -> String {
    match value {
        Value::Bool(boolean) => {
            if *boolean {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Value::Special(SpecialValue::Null | SpecialValue::None) => "null".to_string(),
        _ => value.string_repr(),
    }
}

/// Format a tuple: "(1, 'two')" and, for a single item, "(9, )".
fn tuple_string(items: &[Value9], quoted: bool) -> String {
    let parts: Vec<String> = items
        .iter()
        .map(|item| {
            if quoted {
                item.string_repr()
            } else {
                item.display()
            }
        })
        .collect();
    if items.len() == 1 {
        format!("({}, )", parts[0])
    } else {
        format!("({})", parts.join(", "))
    }
}

/// The synthetic funcref name for an object with a given prefix and id.
pub fn prefixed_funcref_name(prefix: &[u8], id: usize) -> VimString {
    let mut bytes = prefix.to_vec();
    bytes.extend_from_slice(id.to_string().as_bytes());
    VimString::from_bytes(bytes)
}

/// The synthetic funcref name for callable number "id".
pub fn funcref_name(id: usize) -> VimString {
    prefixed_funcref_name(FUNCREF_PREFIX, id)
}

/// The id stored in a synthetic funcref value with "prefix".
pub fn prefixed_funcref_id(value: &Value, prefix: &[u8]) -> Option<usize> {
    let Value::Funcref(data) = value else {
        return None;
    };
    let data = data.borrow();
    let rest = data.name.as_bytes().strip_prefix(prefix)?;
    std::str::from_utf8(rest).ok()?.parse().ok()
}

/// The callable number stored in a funcref value, when it is a Vim9 callable.
pub fn funcref_id(value: &Value) -> Option<usize> {
    prefixed_funcref_id(value, FUNCREF_PREFIX)
}

/// Build a Vim9 value that stores a registry id under "prefix".
pub fn prefixed_marker(prefix: &[u8], id: usize) -> Value9 {
    let data = FuncrefData::new(
        prefixed_funcref_name(prefix, id),
        FuncrefKind::User,
        ScriptId::default(),
    );
    Value9::Val(Value::funcref(data))
}

/// Read a registry id stored under "prefix" from a Vim9 value.
pub fn marker_id(value: &Value9, prefix: &[u8]) -> Option<usize> {
    match value {
        Value9::Val(value) => prefixed_funcref_id(value, prefix),
        Value9::Tuple(_) => None,
    }
}
