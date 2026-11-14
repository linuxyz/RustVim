//! Small helpers shared by the Vim9 evaluator methods.
//!
//! Kept in a separate module so the large evaluator impl block stays readable;
//! they are free functions because they do not need interpreter state.

use crate::error::{Result, VimError};
use crate::value::Value;

use super::ast::{AssignOp, BinaryOp};
use super::value9::Value9;

/// Error for language features scheduled for later phases.
pub fn not_implemented(what: &str) -> VimError {
    VimError::new(
        319,
        format!("Vim9 {what} support is not implemented yet (planned Phase 3/4)"),
    )
}

/// Translate a compound assignment operator to its binary operator.
pub fn assign_to_binary(assign: AssignOp) -> BinaryOp {
    match assign {
        AssignOp::Assign => BinaryOp::Add,
        AssignOp::Add => BinaryOp::Add,
        AssignOp::Sub => BinaryOp::Sub,
        AssignOp::Mul => BinaryOp::Mul,
        AssignOp::Div => BinaryOp::Div,
        AssignOp::Rem => BinaryOp::Rem,
        AssignOp::Concat => BinaryOp::Concat,
    }
}

/// Identity comparison for "is"/"isnot".
pub fn same_value(left: &Value9, right: &Value9) -> bool {
    match (left, right) {
        (Value9::Val(left), Value9::Val(right)) => left.same_object(right),
        _ => false,
    }
}

/// Normalize a list/tuple index, supporting negative indexes.
pub fn normalize_index(index: &Value9, length: i64) -> Result<usize> {
    let number = index
        .as_number()
        .ok_or_else(|| VimError::new(1030, "E1030: Using a special value as a Number"))?;
    let position = if number < 0 { length + number } else { number };
    if position < 0 || position >= length {
        return Err(VimError::new(684, "E684: List index out of range"));
    }
    Ok(position as usize)
}

/// Compute clamped slice bounds, supporting negative indexes.
pub fn slice_bounds(start: i64, end: Option<i64>, length: usize) -> (usize, usize) {
    let length_i64 = length as i64;
    let normalize = |value: i64| -> i64 {
        if value < 0 {
            (length_i64 + value).max(0)
        } else {
            value.min(length_i64)
        }
    };
    let start = normalize(start).max(0) as usize;
    let end = normalize(end.unwrap_or(length_i64)) as usize;
    (start.min(length), end.max(start).min(length))
}

/// Vim's string() conversion for the values Vim9 has but legacy does not
/// format the same way.
pub fn special_string(value: &Value9) -> Result<Option<String>> {
    match value {
        Value9::Tuple(items) => {
            let rendered: Vec<String> = items.iter().map(Value9::string_repr).collect();
            if items.len() == 1 {
                Ok(Some(format!("({}, )", rendered[0])))
            } else {
                Ok(Some(format!("({})", rendered.join(", "))))
            }
        }
        Value9::Val(Value::Bool(boolean)) => Ok(Some(if *boolean {
            "true".to_string()
        } else {
            "false".to_string()
        })),
        Value9::Val(Value::Special(_)) => Ok(Some("null".to_string())),
        Value9::Val(Value::Blob(blob)) => Ok(Some(blob_string(&blob.borrow()))),
        _ => Ok(None),
    }
}

/// Vim's blob string form: 0zAABBCCDD.EEFF with a dot every four bytes.
pub fn blob_string(bytes: &[u8]) -> String {
    let mut out = String::from("0z");
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 && index % 4 == 0 {
            out.push('.');
        }
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

/// Vim's :strtrans rendering: control bytes become ^X and DEL becomes ^?.
pub fn strtrans_text(text: &str) -> String {
    let mut out = String::new();
    for character in text.chars() {
        let code = character as u32;
        if code < 0x20 {
            out.push('^');
            out.push(char::from_u32(code + 0x40).unwrap_or('?'));
        } else if code == 0x7f {
            out.push_str("^?");
        } else {
            out.push(character);
        }
    }
    out
}

/// Trim characters in "mask" from both ends of "text".
pub fn trim_text(text: &str, mask: &str) -> String {
    let should_trim = |character: char| mask.contains(character);
    text.trim_matches(should_trim).to_string()
}

/// Split a command line into the command name and its argument text.
pub fn split_command(text: &str) -> (&str, &str) {
    let trimmed = text.trim_start();
    match trimmed.find(|character: char| character.is_whitespace()) {
        Some(position) => (&trimmed[..position], trimmed[position..].trim_start()),
        None => (trimmed, ""),
    }
}
