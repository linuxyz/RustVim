//! Coercions, operators and comparisons with Vim's legacy semantics.

use std::cmp::Ordering;
use std::rc::Rc;

use rvim_types::VimString;

use crate::error::{Result, VimError};
use crate::lexer::{CaseSense, CmpOp, CompareKind};
use crate::parser::BinaryOp;
use crate::pattern;
use crate::value::{format_float, Value};

pub fn truthy(value: &Value) -> Result<bool> {
    Ok(match value {
        Value::Number(number) => *number != 0,
        Value::Float(float) => *float != 0.0,
        Value::String(string) => !string.is_empty(),
        Value::Bool(boolean) => *boolean,
        Value::Special(_) => false,
        Value::List(list) => !list
            .try_borrow()
            .map(|list| list.is_empty())
            .unwrap_or(true),
        Value::Dict(dict) => !dict
            .try_borrow()
            .map(|dict| dict.is_empty())
            .unwrap_or(true),
        Value::Blob(blob) => !blob
            .try_borrow()
            .map(|blob| blob.is_empty())
            .unwrap_or(true),
        Value::Funcref(_) => true,
    })
}

pub fn to_number(value: &Value) -> Result<i64> {
    match value {
        Value::Number(number) => Ok(*number),
        Value::Float(float) => Ok(*float as i64),
        Value::Bool(boolean) => Ok(i64::from(*boolean)),
        Value::String(string) => Ok(string_to_number(string.as_bytes())),
        Value::Special(_) => Ok(0),
        Value::List(_) => Err(VimError::new(745, "Using a List as a Number")),
        Value::Dict(_) => Err(VimError::new(745, "Using a Dictionary as a Number")),
        Value::Blob(_) => Err(VimError::new(745, "Using a Blob as a Number")),
        Value::Funcref(_) => Err(VimError::new(745, "Using a Funcref as a Number")),
    }
}

pub fn to_float(value: &Value) -> Result<f64> {
    match value {
        Value::Number(number) => Ok(*number as f64),
        Value::Float(float) => Ok(*float),
        Value::Bool(boolean) => Ok(if *boolean { 1.0 } else { 0.0 }),
        Value::String(string) => Ok(string_to_float(string.as_bytes())),
        Value::Special(_) => Ok(0.0),
        Value::List(_) => Err(VimError::new(891, "Using a List as a Float")),
        Value::Dict(_) => Err(VimError::new(891, "Using a Dictionary as a Float")),
        Value::Blob(_) => Err(VimError::new(891, "Using a Blob as a Float")),
        Value::Funcref(_) => Err(VimError::new(891, "Using a Funcref as a Float")),
    }
}

/// Conversion used by `.`, `..`, `join()` and similar string contexts.
pub fn to_string(value: &Value) -> Result<VimString> {
    match value {
        Value::String(string) => Ok(string.clone()),
        Value::Number(number) => Ok(VimString::from(number.to_string())),
        Value::Float(float) => Ok(VimString::from(format_float(*float))),
        Value::Bool(boolean) => Ok(VimString::from(if *boolean { "v:true" } else { "v:false" })),
        Value::Special(special) => Ok(VimString::from(special.to_string())),
        Value::List(_) => Err(VimError::new(730, "Using a List as a String")),
        Value::Dict(_) => Err(VimError::new(731, "Using a Dictionary as a String")),
        Value::Blob(_) => Err(VimError::new(976, "Using a Blob as a String")),
        Value::Funcref(_) => Err(VimError::new(729, "Using a Funcref as a String")),
    }
}

pub fn string_to_number(bytes: &[u8]) -> i64 {
    let mut index = 0;
    while bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        index += 1;
    }
    let mut negative = false;
    match bytes.get(index) {
        Some(b'-') => {
            negative = true;
            index += 1;
        }
        Some(b'+') => {
            index += 1;
        }
        _ => {}
    }
    let mut radix = 10u32;
    if bytes.get(index) == Some(&b'0') {
        match bytes.get(index + 1) {
            Some(b'x' | b'X') => {
                radix = 16;
                index += 2;
            }
            Some(b'b' | b'B') => {
                radix = 2;
                index += 2;
            }
            Some(b'o' | b'O') => {
                radix = 8;
                index += 2;
            }
            _ => {
                radix = 8;
                index += 1;
            }
        }
    }
    let mut magnitude: u64 = 0;
    let mut any = false;
    while let Some(&byte) = bytes.get(index) {
        let Some(digit) = (byte as char).to_digit(radix) else {
            break;
        };
        any = true;
        magnitude = magnitude
            .wrapping_mul(u64::from(radix))
            .wrapping_add(u64::from(digit));
        index += 1;
    }
    let _ = any;
    if negative {
        (0u64.wrapping_sub(magnitude)) as i64
    } else {
        magnitude as i64
    }
}

pub fn string_to_float(bytes: &[u8]) -> f64 {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    if let Ok(value) = trimmed.parse::<f64>() {
        return value;
    }
    let mut end = 0;
    for (index, character) in trimmed.char_indices() {
        if character.is_ascii_digit()
            || matches!(
                character,
                '+' | '-' | '.' | 'e' | 'E' | 'n' | 'N' | 'i' | 'I' | 'f' | 'F'
            )
        {
            end = index + character.len_utf8();
        } else {
            break;
        }
    }
    trimmed[..end].parse::<f64>().unwrap_or(0.0)
}

pub fn apply_binary(op: BinaryOp, left: &Value, right: &Value, ignorecase: bool) -> Result<Value> {
    match op {
        BinaryOp::Add => add(left, right),
        BinaryOp::Sub => numeric(
            left,
            right,
            |a, b| a.wrapping_sub(b),
            |a, b| a - b,
            OpName::Sub,
        ),
        BinaryOp::Mul => numeric(
            left,
            right,
            |a, b| a.wrapping_mul(b),
            |a, b| a * b,
            OpName::Mul,
        ),
        BinaryOp::Div => divide(left, right),
        BinaryOp::Rem => remainder(left, right),
        BinaryOp::Concat => {
            let mut text = to_string(left)?;
            text.extend_from_slice(to_string(right)?.as_bytes());
            Ok(Value::String(text))
        }
        BinaryOp::Compare(compare) => compare_values(left, right, compare, ignorecase),
        BinaryOp::Is => Ok(Value::Number(i64::from(is_identical(left, right)))),
        BinaryOp::IsNot => Ok(Value::Number(i64::from(!is_identical(left, right)))),
        BinaryOp::And => Ok(Value::Number(i64::from(truthy(left)? && truthy(right)?))),
        BinaryOp::Or => Ok(Value::Number(i64::from(truthy(left)? || truthy(right)?))),
    }
}

#[derive(Clone, Copy)]
enum OpName {
    Add,
    Sub,
    Mul,
}

fn add(left: &Value, right: &Value) -> Result<Value> {
    if let (Value::List(left), Value::List(right)) = (left, right) {
        let mut items = left
            .try_borrow()
            .map(|list| list.items.clone())
            .unwrap_or_default();
        items.extend(
            right
                .try_borrow()
                .map(|list| list.items.clone())
                .unwrap_or_default(),
        );
        return Ok(Value::list(items));
    }
    if let (Value::Blob(left), Value::Blob(right)) = (left, right) {
        let mut bytes = left
            .try_borrow()
            .map(|blob| blob.clone())
            .unwrap_or_default();
        bytes.extend(
            right
                .try_borrow()
                .map(|blob| blob.clone())
                .unwrap_or_default(),
        );
        return Ok(Value::blob(bytes));
    }
    numeric(
        left,
        right,
        |a, b| a.wrapping_add(b),
        |a, b| a + b,
        OpName::Add,
    )
}

fn numeric<Int, Float>(
    left: &Value,
    right: &Value,
    int_op: Int,
    float_op: Float,
    _name: OpName,
) -> Result<Value>
where
    Int: Fn(i64, i64) -> i64,
    Float: Fn(f64, f64) -> f64,
{
    if is_floatish(left) || is_floatish(right) {
        return Ok(Value::Float(float_op(to_float(left)?, to_float(right)?)));
    }
    Ok(Value::Number(int_op(to_number(left)?, to_number(right)?)))
}

fn is_floatish(value: &Value) -> bool {
    matches!(value, Value::Float(_))
}

fn divide(left: &Value, right: &Value) -> Result<Value> {
    if is_floatish(left) || is_floatish(right) {
        let divisor = to_float(right)?;
        if divisor == 0.0 {
            return Err(VimError::new(805, "Divide by zero"));
        }
        return Ok(Value::Float(to_float(left)? / divisor));
    }
    let divisor = to_number(right)?;
    if divisor == 0 {
        return Err(VimError::new(805, "Divide by zero"));
    }
    Ok(Value::Number(to_number(left)?.wrapping_div(divisor)))
}

fn remainder(left: &Value, right: &Value) -> Result<Value> {
    let divisor = to_number(right)?;
    if divisor == 0 {
        return Err(VimError::new(805, "Divide by zero"));
    }
    Ok(Value::Number(to_number(left)?.wrapping_rem(divisor)))
}

pub fn compare_values(
    left: &Value,
    right: &Value,
    compare: CmpOp,
    ignorecase: bool,
) -> Result<Value> {
    let insensitive = match compare.case {
        CaseSense::Sensitive => false,
        CaseSense::Insensitive => true,
        CaseSense::Default => ignorecase,
    };
    if matches!(compare.kind, CompareKind::Match | CompareKind::NotMatch) {
        let haystack = to_string(left)?;
        let pattern_text = to_string(right)?;
        let pattern = pattern::compile(pattern_text.as_bytes())?;
        let found = pattern::find(&pattern, haystack.as_bytes(), 0, insensitive).is_some();
        let result = if compare.kind == CompareKind::Match {
            found
        } else {
            !found
        };
        return Ok(Value::Number(i64::from(result)));
    }
    let ordering = compare_ordering(left, right, insensitive)?;
    let result = match compare.kind {
        CompareKind::Equal => ordering == Ordering::Equal,
        CompareKind::NotEqual => ordering != Ordering::Equal,
        CompareKind::Greater => ordering == Ordering::Greater,
        CompareKind::GreaterEqual => ordering != Ordering::Less,
        CompareKind::Less => ordering == Ordering::Less,
        CompareKind::LessEqual => ordering != Ordering::Greater,
        CompareKind::Match | CompareKind::NotMatch => false,
    };
    Ok(Value::Number(i64::from(result)))
}

pub fn compare_ordering(left: &Value, right: &Value, insensitive: bool) -> Result<Ordering> {
    if let (Value::String(left), Value::String(right)) = (left, right) {
        return Ok(compare_bytes(
            left.as_bytes(),
            right.as_bytes(),
            insensitive,
        ));
    }
    if let (Value::List(left), Value::List(right)) = (left, right) {
        let left = left
            .try_borrow()
            .map(|list| list.items.clone())
            .unwrap_or_default();
        let right = right
            .try_borrow()
            .map(|list| list.items.clone())
            .unwrap_or_default();
        for (left, right) in left.iter().zip(right.iter()) {
            let ordering = compare_ordering(left, right, insensitive)?;
            if ordering != Ordering::Equal {
                return Ok(ordering);
            }
        }
        return Ok(left.len().cmp(&right.len()));
    }
    if let (Value::Blob(left), Value::Blob(right)) = (left, right) {
        let left = left
            .try_borrow()
            .map(|blob| blob.clone())
            .unwrap_or_default();
        let right = right
            .try_borrow()
            .map(|blob| blob.clone())
            .unwrap_or_default();
        return Ok(left.cmp(&right));
    }
    if let (Value::Dict(_), Value::Dict(_)) = (left, right) {
        if values_equal(left, right, insensitive) {
            return Ok(Ordering::Equal);
        }
        return Err(VimError::new(
            1030,
            "Cannot compare dictionaries with < or >",
        ));
    }
    if let (Value::Funcref(_), Value::Funcref(_)) = (left, right) {
        return Ok(if is_identical(left, right) {
            Ordering::Equal
        } else {
            Ordering::Greater
        });
    }
    if matches!(left, Value::Special(_)) || matches!(right, Value::Special(_)) {
        if values_equal(left, right, insensitive) {
            return Ok(Ordering::Equal);
        }
        return Err(VimError::new(1030, "Cannot compare special values"));
    }
    if matches!(left, Value::Float(_)) || matches!(right, Value::Float(_)) {
        return Ok(to_float(left)?
            .partial_cmp(&to_float(right)?)
            .unwrap_or(Ordering::Equal));
    }
    Ok(to_number(left)?.cmp(&to_number(right)?))
}

pub fn values_equal(left: &Value, right: &Value, insensitive: bool) -> bool {
    let mut seen = Vec::new();
    values_equal_inner(left, right, insensitive, &mut seen)
}

fn values_equal_inner(
    left: &Value,
    right: &Value,
    insensitive: bool,
    seen: &mut Vec<(usize, usize)>,
) -> bool {
    match (left, right) {
        (Value::String(left), Value::String(right)) => {
            compare_bytes(left.as_bytes(), right.as_bytes(), insensitive) == Ordering::Equal
        }
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
            left == right
                || (*left == crate::value::SpecialValue::Null
                    && *right == crate::value::SpecialValue::None)
                || (*left == crate::value::SpecialValue::None
                    && *right == crate::value::SpecialValue::Null)
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
                            .all(|(left, right)| values_equal_inner(left, right, insensitive, seen))
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
                            right.get(&entry.key).is_some_and(|value| {
                                values_equal_inner(&entry.value, value, insensitive, seen)
                            })
                        })
                }
                _ => false,
            };
            seen.pop();
            result
        }
        (Value::Blob(left), Value::Blob(right)) => match (left.try_borrow(), right.try_borrow()) {
            (Ok(left), Ok(right)) => *left == *right,
            _ => false,
        },
        (Value::Funcref(left), Value::Funcref(right)) => {
            match (left.try_borrow(), right.try_borrow()) {
                (Ok(left), Ok(right)) => {
                    left.name == right.name
                        && left.kind == right.kind
                        && left.args.len() == right.args.len()
                        && left
                            .args
                            .iter()
                            .zip(right.args.iter())
                            .all(|(left, right)| values_equal_inner(left, right, insensitive, seen))
                }
                _ => false,
            }
        }
        _ => false,
    }
}

pub fn is_identical(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::List(_), Value::List(_))
        | (Value::Dict(_), Value::Dict(_))
        | (Value::Blob(_), Value::Blob(_))
        | (Value::Funcref(_), Value::Funcref(_)) => left.object_id() == right.object_id(),
        (Value::Special(left), Value::Special(right)) => left == right,
        _ => values_equal(left, right, false),
    }
}

fn compare_bytes(left: &[u8], right: &[u8], insensitive: bool) -> Ordering {
    if insensitive {
        let mut left = left.iter().map(|byte| byte.to_ascii_lowercase());
        let mut right = right.iter().map(|byte| byte.to_ascii_lowercase());
        loop {
            match (left.next(), right.next()) {
                (Some(left), Some(right)) => match left.cmp(&right) {
                    Ordering::Equal => continue,
                    other => return other,
                },
                (None, Some(_)) => return Ordering::Less,
                (Some(_), None) => return Ordering::Greater,
                (None, None) => return Ordering::Equal,
            }
        }
    } else {
        left.cmp(right)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coercions() {
        assert_eq!(to_number(&Value::string("42abc")).unwrap(), 42);
        assert_eq!(to_number(&Value::string("0x10")).unwrap(), 16);
        assert_eq!(to_number(&Value::string("010")).unwrap(), 8);
        assert_eq!(to_number(&Value::string("-3")).unwrap(), -3);
        assert_eq!(to_number(&Value::Float(3.9)).unwrap(), 3);
        assert!(to_string(&Value::list(vec![])).is_err());
        assert!(truthy(&Value::string("x")).unwrap());
    }

    #[test]
    fn arithmetic_and_comparison() {
        let add = apply_binary(BinaryOp::Add, &Value::Number(1), &Value::Number(2), false).unwrap();
        assert_eq!(add, Value::Number(3));
        let add =
            apply_binary(BinaryOp::Add, &Value::Number(1), &Value::Float(0.5), false).unwrap();
        assert_eq!(add, Value::Float(1.5));
        let equal = compare_values(
            &Value::String(VimString::from("AbC")),
            &Value::String(VimString::from("abc")),
            CmpOp {
                kind: CompareKind::Equal,
                case: CaseSense::Insensitive,
            },
            false,
        )
        .unwrap();
        assert_eq!(equal, Value::Number(1));
    }

    #[test]
    fn division_by_zero() {
        let error =
            apply_binary(BinaryOp::Div, &Value::Number(1), &Value::Number(0), false).unwrap_err();
        assert_eq!(error.code, Some(805));
    }

    #[test]
    fn string_to_number_prefixes() {
        assert_eq!(string_to_number(b"0b101"), 5);
        assert_eq!(string_to_number(b"0o17"), 15);
        assert_eq!(string_to_number(b"-0x10"), -16);
        assert_eq!(string_to_number(b"12.5"), 12);
    }
}
