//! Pure and host-backed built-in functions.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;

use rvim_types::{ScriptId, VimString};

use crate::error::{codes, Result, VimError};
use crate::eval::{self, Ctx};
use crate::lexer::Scope;
use crate::ops;
use crate::parser::{Expr, Parser};
use crate::pattern;
use crate::script;
use crate::value::{FuncrefData, FuncrefKind, Value, VimDict};

/// Builtins implemented by this crate. Used by exists('*f') and function().
const BUILTINS: &[&str] = &[
    "abs",
    "add",
    "and",
    "call",
    "ceil",
    "char2nr",
    "copy",
    "count",
    "deepcopy",
    "empty",
    "eval",
    "execute",
    "executable",
    "search",
    "exists",
    "extend",
    "hlID",
    "hlexists",
    "items",
    "synID",
    "synIDattr",
    "synIDtrans",
    "system",
    "systemlist",
    "win_gettype",
    "filter",
    "float2nr",
    "floor",
    "fmod",
    "function",
    "funcref",
    "get",
    "getline",
    "getreg",
    "has",
    "has_key",
    "index",
    "insert",
    "invert",
    "join",
    "tr",
    "keys",
    "len",
    "line",
    "map",
    "match",
    "matchend",
    "matchstr",
    "matchstrpos",
    "matchlist",
    "max",
    "min",
    "nr2char",
    "nr2str",
    "or",
    "pow",
    "printf",
    "range",
    "remove",
    "repeat",
    "reverse",
    "round",
    "setline",
    "setreg",
    "sort",
    "split",
    "sqrt",
    "str2float",
    "str2nr",
    "strcharpart",
    "byteidx",
    "byteidxcomp",
    "charidx",
    "strchars",
    "strwidth",
    "escape",
    "shellescape",
    "fnameescape",
    "strdisplaywidth",
    "stridx",
    "string",
    "strlen",
    "strpart",
    "strridx",
    "substitute",
    "tolower",
    "toupper",
    "type",
    "typename",
    "values",
    "xor",
];

pub fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

pub fn builtin_names() -> &'static [&'static str] {
    BUILTINS
}

fn arity(name: &str, args: &[Value], minimum: usize, maximum: Option<usize>) -> Result<()> {
    if args.len() < minimum {
        return Err(codes::e119(name));
    }
    if let Some(maximum) = maximum {
        if args.len() > maximum {
            return Err(codes::e118(name));
        }
    }
    Ok(())
}

fn arg(args: &[Value], index: usize) -> &Value {
    &args[index]
}

fn string_arg(value: &Value) -> Result<VimString> {
    ops::to_string(value)
}

fn number_arg(value: &Value) -> Result<i64> {
    ops::to_number(value)
}

fn list_arg(value: &Value) -> Result<Rc<std::cell::RefCell<crate::value::VimList>>> {
    value
        .as_list()
        .ok_or_else(|| VimError::new(714, "List required"))
}

fn dict_arg(value: &Value) -> Result<Rc<std::cell::RefCell<VimDict>>> {
    value
        .as_dict()
        .ok_or_else(|| VimError::new(715, "Dictionary required"))
}

pub(crate) fn call_builtin(ctx: &mut Ctx, name: &str, args: &[Value]) -> Result<Value> {
    match name {
        "type" => {
            arity(name, args, 1, Some(1))?;
            Ok(Value::Number(arg(args, 0).type_number()))
        }
        "typename" => {
            arity(name, args, 1, Some(1))?;
            Ok(Value::string(arg(args, 0).type_name()))
        }
        "string" => {
            arity(name, args, 1, Some(1))?;
            Ok(Value::String(VimString::from(arg(args, 0).string_repr())))
        }
        "str2nr" => {
            arity(name, args, 1, Some(3))?;
            let text = string_arg(arg(args, 0))?;
            let base = if args.len() >= 2 {
                number_arg(arg(args, 1))?
            } else {
                0
            };
            if !matches!(base, 0 | 2 | 8 | 10 | 16) {
                return Err(VimError::new(
                    474,
                    format!("Invalid base for str2nr(): {base}"),
                ));
            }
            let value = if base == 0 {
                ops::string_to_number(text.as_bytes())
            } else {
                parse_int_base(text.as_bytes(), base as u32)
            };
            Ok(Value::Number(value))
        }
        "nr2str" => {
            arity(name, args, 1, Some(2))?;
            let number = number_arg(arg(args, 0))?;
            let base = if args.len() >= 2 {
                number_arg(arg(args, 1))?
            } else {
                10
            };
            if !matches!(base, 2 | 8 | 10 | 16) {
                return Err(VimError::new(
                    474,
                    format!("Invalid base for nr2str(): {base}"),
                ));
            }
            Ok(Value::String(VimString::from(format_radix(
                number,
                base as u32,
            ))))
        }
        "str2float" => {
            arity(name, args, 1, Some(1))?;
            Ok(Value::Float(ops::string_to_float(
                string_arg(arg(args, 0))?.as_bytes(),
            )))
        }
        "float2nr" => {
            arity(name, args, 1, Some(1))?;
            let number = ops::to_float(arg(args, 0))?;
            if number.is_nan() {
                return Ok(Value::Number(0));
            }
            Ok(Value::Number(number as i64))
        }
        "len" => {
            arity(name, args, 1, Some(1))?;
            match arg(args, 0) {
                Value::String(string) => Ok(Value::Number(string.len() as i64)),
                Value::List(list) => Ok(Value::Number(
                    list.try_borrow().map(|list| list.len()).unwrap_or(0) as i64,
                )),
                Value::Dict(dict) => Ok(Value::Number(
                    dict.try_borrow().map(|dict| dict.len()).unwrap_or(0) as i64,
                )),
                Value::Blob(blob) => Ok(Value::Number(
                    blob.try_borrow().map(|blob| blob.len()).unwrap_or(0) as i64,
                )),
                other => Err(VimError::new(
                    701,
                    format!("Invalid type for len(): {}", other.type_name()),
                )),
            }
        }
        "empty" => {
            arity(name, args, 1, Some(1))?;
            let empty = match arg(args, 0) {
                Value::Number(number) => *number == 0,
                Value::Float(float) => *float == 0.0,
                Value::String(string) => string.is_empty(),
                Value::List(list) => list
                    .try_borrow()
                    .map(|list| list.is_empty())
                    .unwrap_or(true),
                Value::Dict(dict) => dict
                    .try_borrow()
                    .map(|dict| dict.is_empty())
                    .unwrap_or(true),
                Value::Blob(blob) => blob
                    .try_borrow()
                    .map(|blob| blob.is_empty())
                    .unwrap_or(true),
                Value::Bool(boolean) => !*boolean,
                Value::Special(_) => true,
                Value::Funcref(_) => false,
            };
            Ok(Value::Number(i64::from(empty)))
        }
        "copy" => {
            arity(name, args, 1, Some(1))?;
            Ok(shallow_copy(arg(args, 0)))
        }
        "deepcopy" => {
            arity(name, args, 1, Some(2))?;
            let mut seen = HashMap::new();
            Ok(deepcopy(arg(args, 0), &mut seen))
        }
        "add" => {
            arity(name, args, 2, Some(2))?;
            match arg(args, 0) {
                Value::List(list) => {
                    let mut list = list
                        .try_borrow_mut()
                        .map_err(|_| VimError::plain("List is already borrowed"))?;
                    if list.locked {
                        return Err(codes::e741("list"));
                    }
                    if matches!(arg(args, 1), Value::List(_)) {
                        // Vim allows adding a List item without flattening.
                    }
                    list.items.push(arg(args, 1).clone());
                    Ok(arg(args, 0).clone())
                }
                Value::Blob(blob) => {
                    let mut blob = blob
                        .try_borrow_mut()
                        .map_err(|_| VimError::plain("Blob is already borrowed"))?;
                    blob.push((number_arg(arg(args, 1))? & 0xff) as u8);
                    Ok(arg(args, 0).clone())
                }
                other => Err(VimError::new(
                    712,
                    format!(
                        "Argument of add() must be a List or Blob, not {}",
                        other.type_name()
                    ),
                )),
            }
        }
        "extend" => {
            arity(name, args, 2, Some(3))?;
            extend(args)
        }
        "remove" => {
            arity(name, args, 2, Some(2))?;
            remove(args)
        }
        "insert" => {
            arity(name, args, 2, Some(3))?;
            insert(args)
        }
        "sort" => {
            arity(name, args, 1, Some(3))?;
            sort(ctx, args)
        }
        "reverse" => {
            arity(name, args, 1, Some(1))?;
            reverse(arg(args, 0))
        }
        "tr" => {
            arity(name, args, 3, Some(3))?;
            let text = string_arg(arg(args, 0))?;
            let from: Vec<char> = string_arg(arg(args, 1))?
                .to_string_lossy()
                .chars()
                .collect();
            let to: Vec<char> = string_arg(arg(args, 2))?
                .to_string_lossy()
                .chars()
                .collect();
            let mut out = VimString::new();
            for ch in text.to_string_lossy().chars() {
                if let Some(index) = from.iter().position(|candidate| *candidate == ch) {
                    if let Some(character) = to.get(index) {
                        out.push_str(&character.to_string());
                    }
                } else {
                    out.push_str(&ch.to_string());
                }
            }
            Ok(Value::String(out))
        }
        "join" => {
            arity(name, args, 1, Some(2))?;
            join(args)
        }
        "split" => {
            arity(name, args, 1, Some(3))?;
            split(args)
        }
        "keys" => {
            arity(name, args, 1, Some(1))?;
            let dict = dict_arg(arg(args, 0))?;
            let keys = dict
                .try_borrow()
                .map(|dict| dict.keys())
                .unwrap_or_default();
            Ok(Value::list(keys.into_iter().map(Value::String).collect()))
        }
        "values" => {
            arity(name, args, 1, Some(1))?;
            let dict = dict_arg(arg(args, 0))?;
            let values = dict
                .try_borrow()
                .map(|dict| dict.values())
                .unwrap_or_default();
            Ok(Value::list(values))
        }
        "has_key" => {
            arity(name, args, 2, Some(2))?;
            let dict = dict_arg(arg(args, 0))?;
            let key = string_arg(arg(args, 1))?;
            let found = dict
                .try_borrow()
                .map(|dict| dict.contains_key(&key))
                .unwrap_or(false);
            Ok(Value::Number(i64::from(found)))
        }
        "get" => {
            arity(name, args, 2, Some(3))?;
            builtin_get(args)
        }
        "index" => {
            arity(name, args, 2, Some(4))?;
            builtin_index(args)
        }
        "count" => {
            arity(name, args, 2, Some(4))?;
            builtin_count(args)
        }
        "printf" => {
            arity(name, args, 1, None)?;
            let format = string_arg(arg(args, 0))?;
            printf(format.as_bytes(), &args[1..])
        }
        "matchstr" => {
            arity(name, args, 2, Some(4))?;
            match_builtin(ctx, args, MatchResult::Text)
        }
        "match" => {
            arity(name, args, 2, Some(4))?;
            match_builtin(ctx, args, MatchResult::Start)
        }
        "matchstrpos" => {
            arity(name, args, 2, Some(4))?;
            let text = string_arg(arg(args, 0))?;
            let pattern_text = string_arg(arg(args, 1))?;
            let compiled = pattern::compile(pattern_text.as_bytes())?;
            let start = if args.len() >= 3 {
                number_arg(arg(args, 2))?.max(0) as usize
            } else {
                0
            };
            let count = if args.len() >= 4 {
                number_arg(arg(args, 3))?.max(1) as usize
            } else {
                1
            };
            let ignorecase = ctx.ignorecase();
            let mut search = start;
            let mut found = None;
            for _ in 0..count {
                match pattern::find(&compiled, text.as_bytes(), search, ignorecase) {
                    Some(found_match) => {
                        search = if found_match.end == found_match.start {
                            found_match.end + 1
                        } else {
                            found_match.end
                        };
                        found = Some(found_match);
                    }
                    None => {
                        found = None;
                        break;
                    }
                }
            }
            let result = found.map_or_else(
                || vec![Value::string(""), Value::Number(-1), Value::Number(-1)],
                |found| {
                    vec![
                        Value::String(VimString::from(&text.as_bytes()[found.start..found.end])),
                        Value::Number(found.start as i64),
                        Value::Number(found.end as i64),
                    ]
                },
            );
            Ok(Value::list(result))
        }
        "matchlist" => {
            arity(name, args, 2, Some(4))?;
            let text = string_arg(arg(args, 0))?;
            let pattern_text = string_arg(arg(args, 1))?;
            let compiled = pattern::compile(pattern_text.as_bytes())?;
            let start = if args.len() >= 3 {
                number_arg(arg(args, 2))?.max(0) as usize
            } else {
                0
            };
            let count = if args.len() >= 4 {
                number_arg(arg(args, 3))?.max(1) as usize
            } else {
                1
            };
            let ignorecase = ctx.ignorecase();
            let mut search = start;
            let mut found = None;
            for _ in 0..count {
                match pattern::find(&compiled, text.as_bytes(), search, ignorecase) {
                    Some(found_match) => {
                        search = if found_match.end == found_match.start {
                            found_match.end + 1
                        } else {
                            found_match.end
                        };
                        found = Some(found_match);
                    }
                    None => {
                        found = None;
                        break;
                    }
                }
            }
            let result = found.map_or_else(Vec::new, |found| {
                let mut items = vec![Value::String(VimString::from(
                    &text.as_bytes()[found.start..found.end],
                ))];
                for capture in &found.captures {
                    items.push(match capture {
                        Some((start, end)) => {
                            Value::String(VimString::from(&text.as_bytes()[*start..*end]))
                        }
                        None => Value::string(""),
                    });
                }
                items
            });
            Ok(Value::list(result))
        }
        "matchend" => {
            arity(name, args, 2, Some(4))?;
            match_builtin(ctx, args, MatchResult::End)
        }
        "substitute" => {
            arity(name, args, 4, Some(4))?;
            substitute(ctx, args)
        }
        "stridx" => {
            arity(name, args, 2, Some(3))?;
            let haystack = string_arg(arg(args, 0))?;
            let needle = string_arg(arg(args, 1))?;
            let start = if args.len() >= 3 {
                number_arg(arg(args, 2))?.max(0) as usize
            } else {
                0
            };
            let found = find_bytes(haystack.as_bytes(), needle.as_bytes(), start);
            Ok(Value::Number(found.map_or(-1, |index| index as i64)))
        }
        "strridx" => {
            arity(name, args, 2, Some(3))?;
            let haystack = string_arg(arg(args, 0))?;
            let needle = string_arg(arg(args, 1))?;
            let end = if args.len() >= 3 {
                number_arg(arg(args, 2))?.max(0) as usize
            } else {
                haystack.len()
            };
            let found = rfind_bytes(haystack.as_bytes(), needle.as_bytes(), end);
            Ok(Value::Number(found.map_or(-1, |index| index as i64)))
        }
        "strpart" => {
            arity(name, args, 2, Some(4))?;
            let characters = if args.len() >= 4 {
                ops::truthy(arg(args, 3))?
            } else {
                false
            };
            strpart(args, characters)
        }
        "byteidx" | "byteidxcomp" => {
            arity(name, args, 2, Some(3))?;
            let text = string_arg(arg(args, 0))?;
            let target = number_arg(arg(args, 1))?;
            let bytes = text.as_bytes();
            if target < 0 {
                return Ok(Value::Number(-1));
            }
            let mut index = 0usize;
            let mut count = 0i64;
            while index < bytes.len() {
                if count == target {
                    return Ok(Value::Number(index as i64));
                }
                index += utf8_char_len(bytes, index);
                count += 1;
            }
            if count == target {
                Ok(Value::Number(index as i64))
            } else {
                Ok(Value::Number(-1))
            }
        }
        "charidx" => {
            arity(name, args, 2, Some(3))?;
            let text = string_arg(arg(args, 0))?;
            let index = number_arg(arg(args, 1))?;
            let bytes = text.as_bytes();
            if index < 0 || index as usize > bytes.len() {
                return Ok(Value::Number(-1));
            }
            let end = index as usize;
            let mut position = 0usize;
            let mut count = 0i64;
            while position < end {
                position += utf8_char_len(bytes, position);
                if position > end {
                    return Ok(Value::Number(-1));
                }
                count += 1;
            }
            Ok(Value::Number(count))
        }
        "strcharpart" => {
            arity(name, args, 2, Some(3))?;
            strpart(args, true)
        }
        "strlen" => {
            arity(name, args, 1, Some(1))?;
            Ok(Value::Number(string_arg(arg(args, 0))?.len() as i64))
        }
        "escape" => {
            arity(name, args, 2, Some(2))?;
            let text = string_arg(arg(args, 0))?;
            let chars = string_arg(arg(args, 1))?;
            let mut out = VimString::new();
            for ch in text.to_string_lossy().chars() {
                if chars.to_string_lossy().contains(ch) {
                    out.push(b'\\');
                }
                out.push_str(&ch.to_string());
            }
            Ok(Value::String(out))
        }
        "shellescape" => {
            arity(name, args, 1, Some(2))?;
            let text = string_arg(arg(args, 0))?;
            let mut out = VimString::from("'");
            for ch in text.to_string_lossy().chars() {
                if ch == '\'' {
                    out.push_str("'\\''");
                } else {
                    out.push_str(&ch.to_string());
                }
            }
            out.push(b'\'');
            Ok(Value::String(out))
        }
        "fnameescape" => {
            arity(name, args, 1, Some(1))?;
            let text = string_arg(arg(args, 0))?;
            let special = " \t\n*?[{$\\%#'\"|!<";
            let mut out = VimString::new();
            for ch in text.to_string_lossy().chars() {
                if special.contains(ch) {
                    out.push(b'\\');
                }
                out.push_str(&ch.to_string());
            }
            Ok(Value::String(out))
        }
        "strwidth" => {
            arity(name, args, 1, Some(1))?;
            let text = string_arg(arg(args, 0))?;
            Ok(Value::Number(
                rvim_types::unicode::display_width(text.as_bytes(), 0, 1) as i64,
            ))
        }
        "strdisplaywidth" => {
            arity(name, args, 1, Some(2))?;
            let text = string_arg(arg(args, 0))?;
            let start = if args.len() > 1 {
                number_arg(arg(args, 1))?.max(0) as usize
            } else {
                0
            };
            Ok(Value::Number(
                rvim_types::unicode::display_width(text.as_bytes(), start, 8) as i64,
            ))
        }
        "strchars" => {
            arity(name, args, 1, Some(2))?;
            let text = string_arg(arg(args, 0))?;
            let count = String::from_utf8_lossy(text.as_bytes()).chars().count();
            Ok(Value::Number(count as i64))
        }
        "tolower" => {
            arity(name, args, 1, Some(1))?;
            Ok(Value::String(case_map(&string_arg(arg(args, 0))?, true)))
        }
        "toupper" => {
            arity(name, args, 1, Some(1))?;
            Ok(Value::String(case_map(&string_arg(arg(args, 0))?, false)))
        }
        "repeat" => {
            arity(name, args, 2, Some(2))?;
            repeat(args)
        }
        "range" => {
            arity(name, args, 1, Some(3))?;
            range(args)
        }
        "map" => {
            arity(name, args, 2, Some(2))?;
            map_or_filter(ctx, args, true)
        }
        "filter" => {
            arity(name, args, 2, Some(2))?;
            map_or_filter(ctx, args, false)
        }
        "call" => {
            arity(name, args, 2, Some(3))?;
            builtin_call(ctx, args)
        }
        "function" | "funcref" => {
            arity(name, args, 1, Some(3))?;
            builtin_function(ctx, args)
        }
        "exists" => {
            arity(name, args, 1, Some(1))?;
            builtin_exists(ctx, &string_arg(arg(args, 0))?)
        }
        "has" => {
            arity(name, args, 1, Some(1))?;
            let feature = string_arg(arg(args, 0))?
                .to_string_lossy()
                .to_ascii_lowercase();
            let available =
                feature == "rvim" || patch_available(&feature) || rvim_types::has_feature(&feature);
            Ok(Value::Number(i64::from(available)))
        }
        "eval" => {
            arity(name, args, 1, Some(1))?;
            let text = string_arg(arg(args, 0))?;
            let expression = crate::parser::Parser::parse_complete(text.as_bytes())?;
            eval::eval(ctx, &expression)
        }
        "execute" => {
            arity(name, args, 1, Some(1))?;
            let text = string_arg(arg(args, 0))?.to_string_lossy().into_owned();
            ctx.runtime.begin_capture();
            let outcome = script::execute_commands_text(ctx, &text);
            let output = ctx.runtime.end_capture().unwrap_or_default();
            outcome?;
            Ok(Value::String(VimString::from(output)))
        }
        "getline" => {
            arity(name, args, 0, Some(2))?;
            getline(ctx, args)
        }
        "hlexists" => {
            arity(name, args, 1, Some(1))?;
            let group = string_arg(arg(args, 0))?.to_string_lossy().into_owned();
            Ok(Value::Number(i64::from(ctx.host.highlight_id(&group)? > 0)))
        }
        "items" => {
            arity(name, args, 1, Some(1))?;
            let dict = dict_arg(arg(args, 0))?;
            let pairs = dict
                .try_borrow()
                .map(|dict| {
                    dict.keys()
                        .into_iter()
                        .map(|key| {
                            let value = dict
                                .get(&key)
                                .cloned()
                                .unwrap_or(Value::Special(crate::value::SpecialValue::Null));
                            Value::list(vec![Value::String(key), value])
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(Value::list(pairs))
        }
        "hlID" => {
            arity(name, args, 1, Some(1))?;
            let group = string_arg(arg(args, 0))?.to_string_lossy().into_owned();
            Ok(Value::Number(ctx.host.highlight_id(&group)?))
        }
        "synID" => {
            arity(name, args, 3, Some(3))?;
            // Position based syntax ids are not modelled; 0 means "no group".
            Ok(Value::Number(0))
        }
        "synIDtrans" => {
            arity(name, args, 1, Some(1))?;
            Ok(Value::Number(number_arg(arg(args, 0))?))
        }
        "synIDattr" => {
            arity(name, args, 2, Some(3))?;
            // Highlight attributes beyond the group ids are not exposed.
            Ok(Value::String(VimString::new()))
        }
        "system" => {
            arity(name, args, 1, Some(2))?;
            let command = string_arg(arg(args, 0))?.to_string_lossy().into_owned();
            let output = ctx.host.system_output(&command)?;
            let mut text = String::from_utf8_lossy(&output).into_owned();
            text = text.replace('\0', "\n");
            Ok(Value::String(VimString::from(text)))
        }
        "systemlist" => {
            arity(name, args, 1, Some(2))?;
            let command = string_arg(arg(args, 0))?.to_string_lossy().into_owned();
            let output = ctx.host.system_output(&command)?;
            let text = String::from_utf8_lossy(&output).into_owned();
            let mut lines: Vec<Value> = text
                .split('\n')
                .map(|line| Value::String(VimString::from(line)))
                .collect();
            if text.ends_with('\n') {
                lines.pop();
            }
            Ok(Value::list(lines))
        }
        "win_gettype" => {
            arity(name, args, 0, Some(1))?;
            // RVim does not model window types such as command/preview windows.
            Ok(Value::String(VimString::from("unknown")))
        }
        "executable" => {
            arity(name, args, 1, Some(1))?;
            let text = string_arg(arg(args, 0))?.to_string_lossy().into_owned();
            Ok(Value::Number(i64::from(executable_exists(&text))))
        }
        "search" => {
            arity(name, args, 1, Some(4))?;
            search(ctx, args)
        }
        "setline" => {
            arity(name, args, 2, Some(2))?;
            setline(ctx, args)
        }
        "line" => {
            arity(name, args, 0, Some(1))?;
            if args.is_empty() {
                return Ok(Value::Number(ctx.host.current_line()?));
            }
            let text = string_arg(arg(args, 0))?;
            match text.as_bytes() {
                b"." => Ok(Value::Number(ctx.host.current_line()?)),
                b"$" => Ok(Value::Number(ctx.host.line_count()?)),
                _ => Ok(Value::Number(ops::to_number(arg(args, 0))?)),
            }
        }
        "min" | "max" => {
            arity(name, args, 1, None)?;
            min_max(name, args)
        }
        "abs" => {
            arity(name, args, 1, Some(1))?;
            match arg(args, 0) {
                Value::Float(float) => Ok(Value::Float(float.abs())),
                other => Ok(Value::Number(number_arg(other)?.wrapping_abs())),
            }
        }
        "round" | "floor" | "ceil" | "sqrt" => {
            arity(name, args, 1, Some(1))?;
            let value = ops::to_float(arg(args, 0))?;
            let result = match name {
                "round" => {
                    if value >= 0.0 {
                        (value + 0.5).floor()
                    } else {
                        (value - 0.5).ceil()
                    }
                }
                "floor" => value.floor(),
                "ceil" => value.ceil(),
                _ => value.sqrt(),
            };
            Ok(Value::Float(result))
        }
        "pow" | "fmod" => {
            arity(name, args, 2, Some(2))?;
            let left = ops::to_float(arg(args, 0))?;
            let right = ops::to_float(arg(args, 1))?;
            if name == "pow" {
                Ok(Value::Float(left.powf(right)))
            } else {
                Ok(Value::Float(left % right))
            }
        }
        "and" | "or" | "xor" => {
            arity(name, args, 2, Some(2))?;
            let left = number_arg(arg(args, 0))?;
            let right = number_arg(arg(args, 1))?;
            Ok(Value::Number(match name {
                "and" => left & right,
                "or" => left | right,
                _ => left ^ right,
            }))
        }
        "invert" => {
            arity(name, args, 1, Some(1))?;
            Ok(Value::Number(!number_arg(arg(args, 0))?))
        }
        "char2nr" => {
            arity(name, args, 1, Some(2))?;
            let text = string_arg(arg(args, 0))?;
            let value = text.char_at(0).map_or(0, |character| character as u32);
            Ok(Value::Number(i64::from(value)))
        }
        "nr2char" => {
            arity(name, args, 1, Some(2))?;
            Ok(Value::String(char_string(number_arg(arg(args, 0))?)))
        }
        "getreg" => {
            arity(name, args, 1, Some(2))?;
            let register = register_name(&string_arg(arg(args, 0))?)?;
            let as_list = args.len() > 1 && args[1].echo_string() != "0";
            ctx.host.get_register_list(register, as_list)
        }
        "setreg" => {
            arity(name, args, 2, Some(3))?;
            let register = register_name(&string_arg(arg(args, 0))?)?;
            let value = arg(args, 1).clone();
            let options = if args.len() > 2 {
                string_arg(arg(args, 2))?
            } else {
                VimString::new()
            };
            ctx.host
                .set_register_ex(register, value, &options.to_string_lossy())?;
            Ok(Value::Number(0))
        }
        _ => Err(codes::e117(name)),
    }
}

fn shallow_copy(value: &Value) -> Value {
    match value {
        Value::List(list) => {
            let items = list
                .try_borrow()
                .map(|list| list.items.clone())
                .unwrap_or_default();
            Value::list(items)
        }
        Value::Dict(dict) => {
            let copy = dict
                .try_borrow()
                .map(|dict| dict.clone())
                .unwrap_or_default();
            Value::dict(copy)
        }
        Value::Blob(blob) => {
            let bytes = blob
                .try_borrow()
                .map(|blob| blob.clone())
                .unwrap_or_default();
            Value::blob(bytes)
        }
        other => other.clone(),
    }
}

fn deepcopy(value: &Value, seen: &mut HashMap<(u8, usize), Value>) -> Value {
    match value {
        Value::List(list) => {
            let id = (0u8, std::rc::Rc::as_ptr(list) as usize);
            if let Some(existing) = seen.get(&id) {
                return existing.clone();
            }
            let copy = Value::list(Vec::new());
            seen.insert(id, copy.clone());
            let items = list
                .try_borrow()
                .map(|list| list.items.clone())
                .unwrap_or_default();
            if let Value::List(new_list) = &copy {
                if let Ok(mut new_list) = new_list.try_borrow_mut() {
                    new_list.items = items.iter().map(|item| deepcopy(item, seen)).collect();
                }
            }
            copy
        }
        Value::Dict(dict) => {
            let id = (1u8, std::rc::Rc::as_ptr(dict) as usize);
            if let Some(existing) = seen.get(&id) {
                return existing.clone();
            }
            let copy = Value::dict(VimDict::new());
            seen.insert(id, copy.clone());
            let entries = dict
                .try_borrow()
                .map(|dict| dict.entries.clone())
                .unwrap_or_default();
            if let Value::Dict(new_dict) = &copy {
                if let Ok(mut new_dict) = new_dict.try_borrow_mut() {
                    for entry in entries {
                        new_dict.set(entry.key, deepcopy(&entry.value, seen));
                    }
                }
            }
            copy
        }
        Value::Blob(blob) => {
            let bytes = blob
                .try_borrow()
                .map(|blob| blob.clone())
                .unwrap_or_default();
            Value::blob(bytes)
        }
        Value::Funcref(reference) => {
            let id = (2u8, std::rc::Rc::as_ptr(reference) as usize);
            if let Some(existing) = seen.get(&id) {
                return existing.clone();
            }
            let data = match reference.try_borrow() {
                Ok(data) => data.clone(),
                Err(_) => return value.clone(),
            };
            let mut copy = data.clone();
            copy.args = data.args.iter().map(|item| deepcopy(item, seen)).collect();
            if let Some(dict) = &data.dict {
                copy.dict = Some(deepcopy(dict, seen));
            }
            let copied = Value::funcref(copy);
            seen.insert(id, copied.clone());
            copied
        }
        other => other.clone(),
    }
}

fn extend(args: &[Value]) -> Result<Value> {
    let third = args.get(2);
    match (arg(args, 0), arg(args, 1)) {
        (Value::List(target), Value::List(source)) => {
            let index = match third {
                Some(value) => Some(number_arg(value)?),
                None => None,
            };
            let source_items = source
                .try_borrow()
                .map(|list| list.items.clone())
                .unwrap_or_default();
            let mut target = target
                .try_borrow_mut()
                .map_err(|_| VimError::plain("List is already borrowed"))?;
            if target.locked {
                return Err(codes::e741("list"));
            }
            match index {
                Some(index) => {
                    let position = (index.max(0) as usize).min(target.items.len());
                    target.items.splice(position..position, source_items);
                }
                None => target.items.extend(source_items),
            }
            drop(target);
            Ok(arg(args, 0).clone())
        }
        (Value::Blob(target), Value::Blob(source)) => {
            let index = match third {
                Some(value) => Some(number_arg(value)?),
                None => None,
            };
            let source_bytes = source
                .try_borrow()
                .map(|blob| blob.clone())
                .unwrap_or_default();
            let mut target = target
                .try_borrow_mut()
                .map_err(|_| VimError::plain("Blob is already borrowed"))?;
            match index {
                Some(index) => {
                    let position = (index.max(0) as usize).min(target.len());
                    target.splice(position..position, source_bytes);
                }
                None => target.extend(source_bytes),
            }
            drop(target);
            Ok(arg(args, 0).clone())
        }
        (Value::Dict(target), Value::Dict(source)) => {
            let keep = match third {
                Some(value) => {
                    let action = string_arg(value)?.to_string_lossy().to_ascii_lowercase();
                    match action.as_str() {
                        "keep" => true,
                        "force" => false,
                        _ => {
                            return Err(VimError::new(
                                475,
                                format!("Invalid extend() action: {action}"),
                            ));
                        }
                    }
                }
                None => false,
            };
            let entries = source
                .try_borrow()
                .map(|dict| dict.entries.clone())
                .unwrap_or_default();
            let mut target = target
                .try_borrow_mut()
                .map_err(|_| VimError::plain("Dictionary is already borrowed"))?;
            if target.locked {
                return Err(codes::e741("dictionary"));
            }
            for entry in entries {
                if keep && target.contains_key(&entry.key) {
                    continue;
                }
                if target.entry_locked(&entry.key) {
                    return Err(codes::e741(&entry.key.to_string_lossy()));
                }
                target.set(entry.key, entry.value);
            }
            drop(target);
            Ok(arg(args, 0).clone())
        }
        _ => Err(VimError::new(
            712,
            "extend() requires two Lists, two Blobs or two Dictionaries",
        )),
    }
}

fn remove(args: &[Value]) -> Result<Value> {
    match arg(args, 0) {
        Value::List(list) => {
            let number = number_arg(arg(args, 1))?;
            let mut list = list
                .try_borrow_mut()
                .map_err(|_| VimError::plain("List is already borrowed"))?;
            if list.locked {
                return Err(codes::e741("list"));
            }
            let index =
                normalize_index(number, list.items.len()).ok_or_else(|| codes::e684(number))?;
            Ok(list.items.remove(index))
        }
        Value::Dict(dict) => {
            let key = string_arg(arg(args, 1))?;
            let mut dict = dict
                .try_borrow_mut()
                .map_err(|_| VimError::plain("Dictionary is already borrowed"))?;
            if dict.locked || dict.entry_locked(&key) {
                return Err(codes::e741(&key.to_string_lossy()));
            }
            dict.remove(&key)
                .ok_or_else(|| codes::e716(&key.to_string_lossy()))
        }
        Value::Blob(blob) => {
            let number = number_arg(arg(args, 1))?;
            let mut blob = blob
                .try_borrow_mut()
                .map_err(|_| VimError::plain("Blob is already borrowed"))?;
            let index = normalize_index(number, blob.len()).ok_or_else(|| codes::e684(number))?;
            Ok(Value::Number(i64::from(blob.remove(index))))
        }
        other => Err(VimError::new(
            712,
            format!("remove() does not accept {}", other.type_name()),
        )),
    }
}

fn insert(args: &[Value]) -> Result<Value> {
    let index = if args.len() >= 3 {
        number_arg(arg(args, 2))?
    } else {
        0
    };
    match arg(args, 0) {
        Value::List(list) => {
            let mut list = list
                .try_borrow_mut()
                .map_err(|_| VimError::plain("List is already borrowed"))?;
            if list.locked {
                return Err(codes::e741("list"));
            }
            let position = index.clamp(0, list.items.len() as i64) as usize;
            list.items.insert(position, arg(args, 1).clone());
            Ok(arg(args, 0).clone())
        }
        Value::Blob(blob) => {
            let mut blob = blob
                .try_borrow_mut()
                .map_err(|_| VimError::plain("Blob is already borrowed"))?;
            let position = index.clamp(0, blob.len() as i64) as usize;
            blob.insert(position, (number_arg(arg(args, 1))? & 0xff) as u8);
            Ok(arg(args, 0).clone())
        }
        other => Err(VimError::new(
            712,
            format!("insert() does not accept {}", other.type_name()),
        )),
    }
}

fn sort(ctx: &mut Ctx, args: &[Value]) -> Result<Value> {
    let list = list_arg(arg(args, 0))?;
    let how = args.get(1).cloned();
    let comparator_dict = match args.get(2) {
        Some(value) if value.is_dict() => Some(value.clone()),
        Some(_) => {
            return Err(VimError::new(
                715,
                "Dictionary required as sort() argument 3",
            ))
        }
        None => None,
    };
    let mut items = list
        .try_borrow()
        .map(|list| list.items.clone())
        .unwrap_or_default();
    if list.try_borrow().map(|list| list.locked).unwrap_or(false) {
        return Err(codes::e741("list"));
    }
    let mut sort_error: Option<VimError> = None;
    match how {
        None => items.sort_by(|left, right| compare_for_sort(left, right, false)),
        Some(Value::String(flag)) => {
            let flag = flag.to_string_lossy();
            let insensitive = flag.contains('i');
            if flag.contains('n') || flag.contains('N') {
                items.sort_by(|left, right| {
                    let left = ops::to_number(left).unwrap_or(0);
                    let right = ops::to_number(right).unwrap_or(0);
                    left.cmp(&right)
                });
            } else if flag.contains('f') {
                items.sort_by(|left, right| {
                    let left = ops::to_float(left).unwrap_or(0.0);
                    let right = ops::to_float(right).unwrap_or(0.0);
                    left.partial_cmp(&right).unwrap_or(Ordering::Equal)
                });
            } else {
                items.sort_by(|left, right| compare_for_sort(left, right, insensitive));
            }
        }
        Some(Value::Funcref(function)) => {
            let mut order: Vec<(Value, usize)> = items
                .into_iter()
                .enumerate()
                .map(|(index, value)| (value, index))
                .collect();
            order.sort_by(|left, right| {
                if sort_error.is_some() {
                    return Ordering::Equal;
                }
                match eval::call_funcref(
                    ctx,
                    &Value::Funcref(function.clone()),
                    vec![left.0.clone(), right.0.clone()],
                    comparator_dict.clone(),
                ) {
                    Ok(result) => match ops::to_number(&result) {
                        Ok(number) => number.cmp(&0),
                        Err(error) => {
                            sort_error = Some(error);
                            Ordering::Equal
                        }
                    },
                    Err(error) => {
                        sort_error = Some(error);
                        Ordering::Equal
                    }
                }
            });
            items = order.into_iter().map(|(value, _)| value).collect();
        }
        Some(other) => {
            return Err(VimError::new(
                475,
                format!("Invalid sort argument: {}", other.echo_string()),
            ));
        }
    }
    if let Some(error) = sort_error {
        return Err(error);
    }
    if let Ok(mut list) = list.try_borrow_mut() {
        list.items = items;
    }
    Ok(arg(args, 0).clone())
}

fn compare_for_sort(left: &Value, right: &Value, insensitive: bool) -> Ordering {
    // Vim's default sort compares the string form of the items.
    let left = Value::String(ops::to_string(left).unwrap_or_default());
    let right = Value::String(ops::to_string(right).unwrap_or_default());
    ops::compare_ordering(&left, &right, insensitive).unwrap_or(Ordering::Equal)
}

fn reverse(value: &Value) -> Result<Value> {
    match value {
        Value::List(list) => {
            let mut list = list
                .try_borrow_mut()
                .map_err(|_| VimError::plain("List is already borrowed"))?;
            if list.locked {
                return Err(codes::e741("list"));
            }
            list.items.reverse();
            drop(list);
            Ok(value.clone())
        }
        Value::Blob(blob) => {
            let mut blob = blob
                .try_borrow_mut()
                .map_err(|_| VimError::plain("Blob is already borrowed"))?;
            blob.reverse();
            drop(blob);
            Ok(value.clone())
        }
        other => Err(VimError::new(
            712,
            format!("reverse() does not accept {}", other.type_name()),
        )),
    }
}

fn join(args: &[Value]) -> Result<Value> {
    let list = list_arg(arg(args, 0))?;
    let separator = if args.len() >= 2 {
        string_arg(arg(args, 1))?
    } else {
        VimString::from(" ")
    };
    let items = list
        .try_borrow()
        .map(|list| list.items.clone())
        .unwrap_or_default();
    let mut output = VimString::new();
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            output.extend_from_slice(separator.as_bytes());
        }
        output.extend_from_slice(ops::to_string(item)?.as_bytes());
    }
    Ok(Value::String(output))
}

fn split(args: &[Value]) -> Result<Value> {
    let text = string_arg(arg(args, 0))?;
    let pattern_text = if args.len() >= 2 {
        string_arg(arg(args, 1))?
    } else {
        VimString::from("\\s\\+")
    };
    let keepempty = if args.len() >= 3 {
        ops::truthy(arg(args, 2))?
    } else {
        false
    };
    let compiled = pattern::compile(pattern_text.as_bytes())?;
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut search = 0;
    while let Some(found) = pattern::find(&compiled, text.as_bytes(), search, false) {
        let piece = &text.as_bytes()[start..found.start];
        if keepempty || !piece.is_empty() {
            pieces.push(Value::String(VimString::from(piece)));
        }
        if found.end == found.start {
            if found.end >= text.len() {
                start = text.len();
                break;
            }
            search = found.end + 1;
        } else {
            search = found.end;
        }
        start = found.end;
    }
    if start <= text.len() {
        let piece = &text.as_bytes()[start..];
        if keepempty || !piece.is_empty() {
            pieces.push(Value::String(VimString::from(piece)));
        }
    }
    Ok(Value::list(pieces))
}

fn builtin_get(args: &[Value]) -> Result<Value> {
    let default = args.get(2).cloned().unwrap_or(Value::Number(0));
    match arg(args, 0) {
        Value::List(list) => {
            let index = number_arg(arg(args, 1))?;
            let list = list
                .try_borrow()
                .map_err(|_| VimError::plain("List is already borrowed"))?;
            match normalize_index(index, list.items.len()) {
                Some(index) => Ok(list.items[index].clone()),
                None => Ok(default),
            }
        }
        Value::Dict(dict) => {
            let key = string_arg(arg(args, 1))?;
            let dict = dict
                .try_borrow()
                .map_err(|_| VimError::plain("Dictionary is already borrowed"))?;
            Ok(dict.get(&key).cloned().unwrap_or(default))
        }
        Value::Blob(blob) => {
            let index = number_arg(arg(args, 1))?;
            let blob = blob
                .try_borrow()
                .map_err(|_| VimError::plain("Blob is already borrowed"))?;
            match normalize_index(index, blob.len()) {
                Some(index) => Ok(Value::Number(i64::from(blob[index]))),
                None => Ok(default),
            }
        }
        other => Err(VimError::new(
            712,
            format!("get() does not accept {}", other.type_name()),
        )),
    }
}

fn builtin_index(args: &[Value]) -> Result<Value> {
    let items = match arg(args, 0) {
        Value::List(list) => list
            .try_borrow()
            .map(|list| list.items.clone())
            .unwrap_or_default(),
        Value::Blob(blob) => blob
            .try_borrow()
            .map(|blob| {
                blob.iter()
                    .map(|byte| Value::Number(i64::from(*byte)))
                    .collect()
            })
            .unwrap_or_default(),
        other => {
            return Err(VimError::new(
                714,
                format!("index() requires a List, found {}", other.type_name()),
            ))
        }
    };
    let start = if args.len() >= 3 {
        number_arg(arg(args, 2))?.max(0) as usize
    } else {
        0
    };
    let insensitive = if args.len() >= 4 {
        ops::truthy(arg(args, 3))?
    } else {
        false
    };
    for (index, item) in items.iter().enumerate().skip(start) {
        if ops::values_equal(item, arg(args, 1), insensitive) {
            return Ok(Value::Number(index as i64));
        }
    }
    Ok(Value::Number(-1))
}

fn builtin_count(args: &[Value]) -> Result<Value> {
    let items = match arg(args, 0) {
        Value::List(list) => list
            .try_borrow()
            .map(|list| list.items.clone())
            .unwrap_or_default(),
        Value::Blob(blob) => blob
            .try_borrow()
            .map(|blob| {
                blob.iter()
                    .map(|byte| Value::Number(i64::from(*byte)))
                    .collect()
            })
            .unwrap_or_default(),
        Value::Dict(dict) => dict
            .try_borrow()
            .map(|dict| dict.values())
            .unwrap_or_default(),
        other => {
            return Err(VimError::new(
                714,
                format!("count() requires a List, found {}", other.type_name()),
            ))
        }
    };
    let insensitive = if args.len() >= 3 {
        ops::truthy(arg(args, 2))?
    } else {
        false
    };
    let count = items
        .iter()
        .filter(|item| ops::values_equal(item, arg(args, 1), insensitive))
        .count();
    Ok(Value::Number(count as i64))
}

fn parse_int_base(bytes: &[u8], base: u32) -> i64 {
    let mut index = 0;
    while bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        index += 1;
    }
    let mut negative = false;
    if bytes.get(index) == Some(&b'-') {
        negative = true;
        index += 1;
    } else if bytes.get(index) == Some(&b'+') {
        index += 1;
    }
    if base == 16
        && bytes.get(index) == Some(&b'0')
        && matches!(bytes.get(index + 1), Some(b'x' | b'X'))
    {
        index += 2;
    }
    let mut value: u64 = 0;
    while let Some(&byte) = bytes.get(index) {
        let Some(digit) = (byte as char).to_digit(base) else {
            break;
        };
        value = value
            .wrapping_mul(u64::from(base))
            .wrapping_add(u64::from(digit));
        index += 1;
    }
    if negative {
        (0u64.wrapping_sub(value)) as i64
    } else {
        value as i64
    }
}

fn format_radix(number: i64, base: u32) -> String {
    if number < 0 {
        format!("-{}", format_u64(0u64.wrapping_sub(number as u64), base))
    } else {
        format_u64(number as u64, base)
    }
}

fn format_u64(mut value: u64, base: u32) -> String {
    if value == 0 {
        return "0".to_string();
    }
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut output = Vec::new();
    while value > 0 {
        output.push(digits[(value % u64::from(base)) as usize]);
        value /= u64::from(base);
    }
    output.reverse();
    String::from_utf8_lossy(&output).into_owned()
}

fn printf(format: &[u8], args: &[Value]) -> Result<Value> {
    let mut output = Vec::new();
    let mut index = 0;
    let mut argument = 0;
    while index < format.len() {
        if format[index] != b'%' {
            output.push(format[index]);
            index += 1;
            continue;
        }
        index += 1;
        if format.get(index) == Some(&b'%') {
            output.push(b'%');
            index += 1;
            continue;
        }
        let mut left = false;
        let mut plus = false;
        let mut space = false;
        let mut zero = false;
        while let Some(&flag) = format.get(index) {
            match flag {
                b'-' => left = true,
                b'+' => plus = true,
                b' ' => space = true,
                b'0' => zero = true,
                b'#' => {}
                _ => break,
            }
            index += 1;
        }
        let width_start = index;
        while format.get(index).is_some_and(|byte| byte.is_ascii_digit()) {
            index += 1;
        }
        let width: usize = String::from_utf8_lossy(&format[width_start..index])
            .parse()
            .unwrap_or(0);
        let mut precision: Option<usize> = None;
        if format.get(index) == Some(&b'.') {
            index += 1;
            let precision_start = index;
            while format.get(index).is_some_and(|byte| byte.is_ascii_digit()) {
                index += 1;
            }
            precision = Some(
                String::from_utf8_lossy(&format[precision_start..index])
                    .parse()
                    .unwrap_or(0),
            );
        }
        let Some(&conversion) = format.get(index) else {
            return Err(VimError::new(767, "Wrong sprintf format string"));
        };
        index += 1;
        if argument >= args.len() {
            return Err(VimError::new(
                767,
                "Not enough arguments for printf() format",
            ));
        }
        let value = &args[argument];
        argument += 1;
        let mut text = match conversion {
            b's' => {
                let text = ops::to_string(value)?;
                let text = if let Some(precision) = precision {
                    VimString::from(&text.as_bytes()[..text.len().min(precision)])
                } else {
                    text
                };
                text.to_string_lossy().into_owned()
            }
            b'd' | b'i' => {
                let number = ops::to_number(value)?;
                format_number_with_flags(number, plus, space)
            }
            b'u' => (ops::to_number(value)? as u64).to_string(),
            b'x' => format_u64(ops::to_number(value)? as u64, 16),
            b'X' => format_u64(ops::to_number(value)? as u64, 16).to_ascii_uppercase(),
            b'o' => format_u64(ops::to_number(value)? as u64, 8),
            b'b' => format_u64(ops::to_number(value)? as u64, 2),
            b'c' => {
                let text = char_string(ops::to_number(value)?);
                text.to_string_lossy().into_owned()
            }
            b'f' | b'F' => {
                let number = ops::to_float(value)?;
                let precision = precision.unwrap_or(6);
                format_number_float(number, precision, plus, space, false)
            }
            b'e' | b'E' => {
                let number = ops::to_float(value)?;
                let precision = precision.unwrap_or(6);
                let text = format!("{:.*e}", precision, number);
                if conversion == b'E' {
                    text.to_ascii_uppercase()
                } else {
                    text
                }
            }
            b'g' | b'G' => {
                let text = format_float_length(ops::to_float(value)?, precision.unwrap_or(6));
                if conversion == b'G' {
                    text.to_ascii_uppercase()
                } else {
                    text
                }
            }
            _ => {
                return Err(VimError::new(
                    767,
                    format!(
                        "Wrong sprintf format string: unknown conversion {}",
                        conversion as char
                    ),
                ));
            }
        };
        if text.len() < width {
            let padding = width - text.len();
            if left {
                text.push_str(&" ".repeat(padding));
            } else if zero
                && matches!(
                    conversion,
                    b'd' | b'i'
                        | b'u'
                        | b'x'
                        | b'X'
                        | b'o'
                        | b'b'
                        | b'f'
                        | b'F'
                        | b'e'
                        | b'E'
                        | b'g'
                        | b'G'
                )
            {
                let sign = text.starts_with('-') || text.starts_with('+');
                if sign {
                    let rest = text[1..].to_string();
                    text = format!("{}{}{}", &text[..1], "0".repeat(padding), rest);
                } else {
                    text = format!("{}{}", "0".repeat(padding), text);
                }
            } else {
                text = format!("{}{}", " ".repeat(padding), text);
            }
        }
        output.extend_from_slice(text.as_bytes());
    }
    Ok(Value::String(VimString::from(output)))
}

fn format_number_with_flags(number: i64, plus: bool, space: bool) -> String {
    if number >= 0 && plus {
        format!("+{number}")
    } else if number >= 0 && space {
        format!(" {number}")
    } else {
        number.to_string()
    }
}

fn format_number_float(
    number: f64,
    precision: usize,
    plus: bool,
    space: bool,
    upper: bool,
) -> String {
    let mut text = format!("{:.*}", precision, number);
    if upper {
        text = text.to_ascii_uppercase();
    }
    if number >= 0.0 && plus {
        text.insert(0, '+');
    } else if number >= 0.0 && space {
        text.insert(0, ' ');
    }
    text
}

fn format_float_length(value: f64, precision: usize) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let text = format!("{:.*e}", precision.saturating_sub(1).min(17), value);
    let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
    if (-4..precision as i32).contains(&exponent) {
        let decimals = (precision as i32 - exponent - 1).max(0) as usize;
        format!("{:.*}", decimals, value)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    } else {
        format!("{mantissa}e{exponent}")
    }
}

enum MatchResult {
    Text,
    Start,
    End,
}

fn match_builtin(ctx: &mut Ctx, args: &[Value], result: MatchResult) -> Result<Value> {
    let ignorecase = ctx.ignorecase();
    let text = string_arg(arg(args, 0))?;
    let pattern_text = string_arg(arg(args, 1))?;
    let compiled = pattern::compile(pattern_text.as_bytes())?;
    let start = if args.len() >= 3 {
        number_arg(arg(args, 2))?.max(0) as usize
    } else {
        0
    };
    let count = if args.len() >= 4 {
        number_arg(arg(args, 3))?.max(1) as usize
    } else {
        1
    };
    let mut search = start;
    let mut found = None;
    for _ in 0..count {
        match pattern::find(&compiled, text.as_bytes(), search, ignorecase) {
            Some(found_match) => {
                search = if found_match.end == found_match.start {
                    found_match.end + 1
                } else {
                    found_match.end
                };
                found = Some(found_match);
            }
            None => {
                found = None;
                break;
            }
        }
    }
    match found {
        Some(found) => Ok(match result {
            MatchResult::Text => {
                Value::String(VimString::from(&text.as_bytes()[found.start..found.end]))
            }
            MatchResult::Start => Value::Number(found.start as i64),
            MatchResult::End => Value::Number(found.end as i64),
        }),
        None => Ok(match result {
            MatchResult::Text => Value::String(VimString::new()),
            MatchResult::Start | MatchResult::End => Value::Number(-1),
        }),
    }
}

fn substitute(ctx: &mut Ctx, args: &[Value]) -> Result<Value> {
    let text = string_arg(arg(args, 0))?;
    let pattern_text = string_arg(arg(args, 1))?;
    let replacement = string_arg(arg(args, 2))?;
    let flags = string_arg(arg(args, 3))?.to_string_lossy().into_owned();
    if replacement.as_bytes().starts_with(b"\\=") {
        return Err(VimError::plain(
            "substitute() expression replacement (\\=) is not supported",
        ));
    }
    let mut ignorecase = ctx.ignorecase();
    let mut global = false;
    for flag in flags.chars() {
        match flag {
            'g' => global = true,
            'i' => ignorecase = true,
            'I' => ignorecase = false,
            _ => {
                return Err(VimError::new(
                    475,
                    format!("Invalid substitute() flag: {flag}"),
                ));
            }
        }
    }
    let compiled = pattern::compile(pattern_text.as_bytes())?;
    let bytes = text.as_bytes();
    let mut output = Vec::new();
    let mut cursor = 0;
    let mut search = 0;
    let mut replaced = false;
    while search <= bytes.len() {
        let Some(found) = pattern::find(&compiled, bytes, search, ignorecase) else {
            break;
        };
        output.extend_from_slice(&bytes[cursor..found.start]);
        output.extend_from_slice(&expand_replacement(replacement.as_bytes(), bytes, &found)?);
        replaced = true;
        cursor = found.end;
        search = if found.end == found.start {
            if found.end >= bytes.len() {
                break;
            }
            found.end + 1
        } else {
            found.end
        };
        if !global {
            break;
        }
    }
    if !replaced {
        return Ok(Value::String(text));
    }
    output.extend_from_slice(&bytes[cursor..]);
    Ok(Value::String(VimString::from(output)))
}

fn expand_replacement(pattern: &[u8], text: &[u8], found: &pattern::Match) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut index = 0;
    while index < pattern.len() {
        let byte = pattern[index];
        if byte == b'&' {
            output.extend_from_slice(&text[found.start..found.end]);
            index += 1;
            continue;
        }
        if byte == b'\\' && index + 1 < pattern.len() {
            let next = pattern[index + 1];
            match next {
                b'0'..=b'9' => {
                    let group = (next - b'0') as usize;
                    if let Some((start, end)) = found.capture(group) {
                        output.extend_from_slice(&text[start..end]);
                    }
                }
                b'n' | b'r' => output.push(b'\n'),
                b't' => output.push(b'\t'),
                b'\\' => output.push(b'\\'),
                b'&' => output.push(b'&'),
                b'u' | b'l' | b'U' | b'L' | b'e' | b'E' => {
                    return Err(VimError::plain(
                        "substitute() case-conversion escapes (\\u \\l \\U \\L \\e \\E) are not supported",
                    ));
                }
                other => output.push(other),
            }
            index += 2;
            continue;
        }
        output.push(byte);
        index += 1;
    }
    Ok(output)
}

fn strpart(args: &[Value], characters: bool) -> Result<Value> {
    let text = string_arg(arg(args, 0))?;
    let start = number_arg(arg(args, 1))?.max(0) as usize;
    let length = if args.len() >= 3 {
        Some(number_arg(arg(args, 2))?.max(0) as usize)
    } else {
        None
    };
    if characters {
        let chars: Vec<char> = String::from_utf8_lossy(text.as_bytes()).chars().collect();
        let end = length.map_or(chars.len(), |length| (start + length).min(chars.len()));
        let slice: String = chars[start.min(chars.len())..end].iter().collect();
        Ok(Value::String(VimString::from(slice)))
    } else {
        let end = length.map_or(text.len(), |length| (start + length).min(text.len()));
        if start >= text.len() {
            return Ok(Value::String(VimString::new()));
        }
        Ok(Value::String(VimString::from(&text.as_bytes()[start..end])))
    }
}

fn case_map(text: &VimString, lower: bool) -> VimString {
    if let Ok(decoded) = std::str::from_utf8(text.as_bytes()) {
        let mapped = if lower {
            decoded.to_lowercase()
        } else {
            decoded.to_uppercase()
        };
        return VimString::from(mapped);
    }
    let bytes: Vec<u8> = text
        .as_bytes()
        .iter()
        .map(|byte| {
            if lower {
                byte.to_ascii_lowercase()
            } else {
                byte.to_ascii_uppercase()
            }
        })
        .collect();
    VimString::from(bytes)
}

fn repeat(args: &[Value]) -> Result<Value> {
    let count = number_arg(arg(args, 1))?.max(0) as usize;
    match arg(args, 0) {
        Value::String(text) => {
            let mut output = Vec::with_capacity(text.len() * count);
            for _ in 0..count {
                output.extend_from_slice(text.as_bytes());
            }
            Ok(Value::String(VimString::from(output)))
        }
        Value::List(list) => {
            let items = list
                .try_borrow()
                .map(|list| list.items.clone())
                .unwrap_or_default();
            let mut output = Vec::with_capacity(items.len() * count);
            for _ in 0..count {
                output.extend(items.iter().cloned());
            }
            Ok(Value::list(output))
        }
        Value::Blob(blob) => {
            let bytes = blob
                .try_borrow()
                .map(|blob| blob.clone())
                .unwrap_or_default();
            let mut output = Vec::with_capacity(bytes.len() * count);
            for _ in 0..count {
                output.extend_from_slice(&bytes);
            }
            Ok(Value::blob(output))
        }
        other => Err(VimError::new(
            712,
            format!("repeat() does not accept {}", other.type_name()),
        )),
    }
}

fn range(args: &[Value]) -> Result<Value> {
    let (start, end, stride) = match args.len() {
        1 => (0, number_arg(arg(args, 0))?.wrapping_sub(1), 1),
        2 => {
            let start = number_arg(arg(args, 0))?;
            let end = number_arg(arg(args, 1))?;
            (start, end, if end >= start { 1 } else { -1 })
        }
        _ => (
            number_arg(arg(args, 0))?,
            number_arg(arg(args, 1))?,
            number_arg(arg(args, 2))?,
        ),
    };
    if stride == 0 {
        return Err(VimError::new(726, "Stride is zero"));
    }
    let mut values = Vec::new();
    let mut current = start;
    let mut guard = 0usize;
    while (stride > 0 && current <= end) || (stride < 0 && current >= end) {
        values.push(Value::Number(current));
        current = current.wrapping_add(stride);
        guard += 1;
        if guard > 10_000_000 {
            return Err(VimError::plain("range() produced too many values"));
        }
    }
    Ok(Value::list(values))
}

fn map_or_filter(ctx: &mut Ctx, args: &[Value], map: bool) -> Result<Value> {
    let expression = arg(args, 1).clone();
    let compiled = match &expression {
        Value::String(text) => Some(Parser::parse_complete(text.as_bytes())?),
        Value::Funcref(_) => None,
        other => {
            return Err(VimError::new(
                712,
                format!(
                    "map()/filter() expression must be a String or Funcref, not {}",
                    other.type_name()
                ),
            ));
        }
    };
    let old_val = ctx.runtime.get_vvar("val").unwrap_or(Value::Number(0));
    let old_key = ctx.runtime.get_vvar("key").unwrap_or(Value::Number(0));
    let outcome = (|| -> Result<Value> {
        match arg(args, 0) {
            Value::List(list) => {
                if list.try_borrow().map(|list| list.locked).unwrap_or(false) {
                    return Err(codes::e741("list"));
                }
                let items = list
                    .try_borrow()
                    .map(|list| list.items.clone())
                    .unwrap_or_default();
                let mut output = Vec::with_capacity(items.len());
                for (index, item) in items.into_iter().enumerate() {
                    ctx.runtime.set_vvar("key", Value::Number(index as i64));
                    ctx.runtime.set_vvar("val", item.clone());
                    let result = evaluate_map_item(ctx, &compiled, &expression, index, &item)?;
                    if map {
                        output.push(result);
                    } else if ops::truthy(&result)? {
                        output.push(item);
                    }
                }
                if let Ok(mut list) = list.try_borrow_mut() {
                    list.items = output;
                }
                Ok(arg(args, 0).clone())
            }
            Value::Dict(dict) => {
                if dict.try_borrow().map(|dict| dict.locked).unwrap_or(false) {
                    return Err(codes::e741("dictionary"));
                }
                let entries = dict
                    .try_borrow()
                    .map(|dict| dict.entries.clone())
                    .unwrap_or_default();
                let mut kept = Vec::new();
                for (index, entry) in entries.into_iter().enumerate() {
                    ctx.runtime
                        .set_vvar("key", Value::String(entry.key.clone()));
                    ctx.runtime.set_vvar("val", entry.value.clone());
                    let result =
                        evaluate_map_item(ctx, &compiled, &expression, index, &entry.value)?;
                    if map {
                        kept.push((entry.key, result));
                    } else if ops::truthy(&result)? {
                        kept.push((entry.key, entry.value));
                    }
                }
                if let Ok(mut dict) = dict.try_borrow_mut() {
                    dict.entries.clear();
                    for (key, value) in kept {
                        dict.set(key, value);
                    }
                }
                Ok(arg(args, 0).clone())
            }
            Value::String(text) => {
                let characters: Vec<char> = text.to_string_lossy().chars().collect();
                let mut output = String::new();
                for (index, character) in characters.iter().enumerate() {
                    ctx.runtime.set_vvar("key", Value::Number(index as i64));
                    let item = Value::String(VimString::from(character.to_string()));
                    ctx.runtime.set_vvar("val", item.clone());
                    let result = evaluate_map_item(ctx, &compiled, &expression, index, &item)?;
                    if map {
                        output.push_str(&ops::to_string(&result)?.to_string_lossy());
                    } else if ops::truthy(&result)? {
                        output.push(*character);
                    }
                }
                Ok(Value::String(VimString::from(output)))
            }
            other => Err(VimError::new(
                712,
                format!(
                    "map()/filter() requires a List, Dictionary or String, found {}",
                    other.type_name()
                ),
            )),
        }
    })();
    ctx.runtime.set_vvar("val", old_val);
    ctx.runtime.set_vvar("key", old_key);
    outcome
}

fn evaluate_map_item(
    ctx: &mut Ctx,
    compiled: &Option<Expr>,
    expression: &Value,
    index: usize,
    item: &Value,
) -> Result<Value> {
    if let Some(expression) = compiled {
        let _ = index;
        eval::eval(ctx, expression)
    } else {
        let key = ctx
            .runtime
            .get_vvar("key")
            .unwrap_or(Value::Number(index as i64));
        eval::call_funcref(ctx, expression, vec![key, item.clone()], None)
    }
}

fn builtin_call(ctx: &mut Ctx, args: &[Value]) -> Result<Value> {
    let call_args = list_arg(arg(args, 1))?;
    let call_args = call_args
        .try_borrow()
        .map(|list| list.items.clone())
        .unwrap_or_default();
    let dict = args.get(2).cloned();
    match arg(args, 0) {
        Value::Funcref(_) => eval::call_funcref(ctx, arg(args, 0), call_args, dict),
        Value::String(name) => {
            let name = name.to_string_lossy().into_owned();
            eval::call_named(ctx, &name, call_args, dict)
        }
        other => Err(VimError::new(
            1085,
            format!("Not a callable type: {}", other.type_name()),
        )),
    }
}

fn builtin_function(ctx: &mut Ctx, args: &[Value]) -> Result<Value> {
    if let Value::Funcref(reference) = arg(args, 0) {
        if args.len() == 1 {
            return Ok(Value::Funcref(reference.clone()));
        }
        let mut data = match reference.try_borrow() {
            Ok(data) => data.clone(),
            Err(_) => return Err(VimError::plain("Funcref is already borrowed")),
        };
        if let Some(value) = args.get(1) {
            let list = list_arg(value)?;
            data.args = list
                .try_borrow()
                .map(|list| list.items.clone())
                .unwrap_or_default();
        }
        if let Some(value) = args.get(2) {
            data.dict = Some(value.clone());
        }
        return Ok(Value::funcref(data));
    }
    let written = string_arg(arg(args, 0))?.to_string_lossy().into_owned();
    let canonical = eval::resolve_function_name(ctx, &written);
    let (kind, stored_name) = if ctx.runtime.has_function(&canonical) {
        (FuncrefKind::User, canonical)
    } else if is_builtin(&canonical) {
        (FuncrefKind::Builtin, canonical)
    } else if ctx.host.function_exists(&written) {
        (FuncrefKind::Builtin, written)
    } else {
        return Err(codes::e117(&written));
    };
    let mut data = FuncrefData::new(
        stored_name,
        kind,
        ctx.current_script_id()
            .unwrap_or_else(|| ScriptId::from_raw(0)),
    );
    if let Some(value) = args.get(1) {
        let list = list_arg(value)?;
        data.args = list
            .try_borrow()
            .map(|list| list.items.clone())
            .unwrap_or_default();
    }
    if let Some(value) = args.get(2) {
        if !value.is_dict() {
            return Err(VimError::new(
                715,
                "Dictionary required as function() argument 3",
            ));
        }
        data.dict = Some(value.clone());
    }
    Ok(Value::funcref(data))
}

fn builtin_exists(ctx: &mut Ctx, expression: &VimString) -> Result<Value> {
    let text = expression.to_string_lossy().into_owned();
    if let Some(name) = text.strip_prefix('*') {
        let canonical = eval::resolve_function_name(ctx, name);
        let found = ctx.runtime.has_function(&canonical)
            || is_builtin(&canonical)
            || ctx.host.function_exists(name);
        return Ok(Value::Number(i64::from(found)));
    }
    if let Some(name) = text.strip_prefix('&') {
        let (scope, name) = if let Some(name) = name.strip_prefix("g:") {
            (crate::host::OptionScope::Global, name)
        } else if let Some(name) = name.strip_prefix("l:") {
            (crate::host::OptionScope::Local, name)
        } else {
            (crate::host::OptionScope::Default, name)
        };
        return Ok(Value::Number(i64::from(
            ctx.host.get_option(scope, name).is_ok(),
        )));
    }
    if let Some(name) = text.strip_prefix('$') {
        return Ok(Value::Number(i64::from(ctx.host.env_exists(name))));
    }
    if let Some(name) = text.strip_prefix('@') {
        let register = name.chars().next().unwrap_or('"');
        let valid = register.is_ascii_alphanumeric()
            || matches!(
                register,
                '"' | '-' | ':' | '.' | '%' | '#' | '=' | '*' | '+' | '~' | '_' | '/'
            );
        return Ok(Value::Number(i64::from(valid)));
    }
    if let Some(name) = text.strip_prefix(':') {
        let found = script::is_internal_command(name) || ctx.host.has_command(name);
        return Ok(Value::Number(i64::from(found)));
    }
    if let Some(name) = text.strip_prefix('+') {
        return Ok(Value::Number(i64::from(
            ctx.host
                .get_option(crate::host::OptionScope::Default, name)
                .is_ok(),
        )));
    }
    if text == "g:" {
        return Ok(Value::Number(1));
    }
    if text.len() >= 2 && text.as_bytes()[1] == b':' {
        if let Some(scope) = Scope::from_prefix(text.as_bytes()[0]) {
            let name = &text[2..];
            if name.is_empty() {
                return Ok(Value::Number(1));
            }
            let found = eval::get_var(ctx, Some(scope), &VimString::from(name)).is_ok();
            return Ok(Value::Number(i64::from(found)));
        }
    }
    let name = VimString::from(text.as_str());
    Ok(Value::Number(i64::from(
        eval::get_var(ctx, None, &name).is_ok(),
    )))
}

/// True when a file named `name` exists and is executable.
fn executable_exists(name: &str) -> bool {
    use std::path::Path;
    #[cfg(unix)]
    fn is_executable(path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    fn is_executable(path: &Path) -> bool {
        path.is_file()
    }
    if name.contains('/') || name.contains('\\') {
        return is_executable(Path::new(name));
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|directory| {
        let candidate = directory.join(name);
        is_executable(&candidate)
    })
}

/// `search({pattern} [, {flags} [, {stopline} [, {timeout}]]])`.
///
/// The search is buffer based.  It returns the matching line number (1 based)
/// or zero.  Cursor movement is not modelled, so the 'n' flag is irrelevant
/// and 'c'/'e'/'s' only influence the starting line.
fn search(ctx: &mut Ctx, args: &[Value]) -> Result<Value> {
    let text = string_arg(arg(args, 0))?;
    let compiled = pattern::compile(text.as_bytes())?;
    let flags = if args.len() >= 2 {
        string_arg(arg(args, 1))?.to_string_lossy().into_owned()
    } else {
        String::new()
    };
    let stopline = if args.len() >= 3 {
        match arg(args, 2) {
            Value::String(value) if value.is_empty() => None,
            value => Some(ops::to_number(value)?),
        }
    } else {
        None
    };
    let ignorecase = matches!(
        ctx.host
            .get_option(crate::host::OptionScope::Global, "ignorecase"),
        Ok(Value::Number(value)) if value != 0
    );
    let total = ctx.host.line_count()?;
    if total <= 0 {
        return Ok(Value::Number(0));
    }
    let current = ctx.host.current_line()?.clamp(1, total);
    let backward = flags.contains('b');
    let wrap = !flags.contains('W');
    let mut candidates: Vec<i64> = Vec::new();
    if backward {
        let stop = stopline.map_or(1, |line| line.clamp(1, total));
        if stop <= current {
            candidates.extend((stop..=current).rev());
        }
        if wrap {
            candidates.extend((current + 1..=total).rev());
        }
    } else {
        let stop = stopline.map_or(total, |line| line.clamp(1, total));
        if stop >= current {
            candidates.extend(current..=stop);
        }
        if wrap {
            candidates.extend(1..current);
        }
    }
    for line in candidates {
        if line < 1 || line > total {
            continue;
        }
        let content = ctx.host.get_line(line)?;
        if pattern::find(&compiled, content.as_bytes(), 0, ignorecase).is_some() {
            return Ok(Value::Number(line));
        }
    }
    Ok(Value::Number(0))
}

fn getline(ctx: &mut Ctx, args: &[Value]) -> Result<Value> {
    let mut start = if args.is_empty() {
        ctx.host.current_line()?
    } else {
        line_number(ctx, arg(args, 0))?
    };
    if start == 0 {
        start = ctx.host.current_line()?;
    }
    let end = if args.len() >= 2 {
        line_number(ctx, arg(args, 1))?
    } else {
        start
    };
    if end < start {
        return Ok(Value::list(Vec::new()));
    }
    if end == start {
        return Ok(Value::String(ctx.host.get_line(start)?));
    }
    let mut lines = Vec::new();
    for line in start..=end {
        lines.push(Value::String(ctx.host.get_line(line)?));
    }
    Ok(Value::list(lines))
}

fn line_number(ctx: &mut Ctx, value: &Value) -> Result<i64> {
    if let Value::String(text) = value {
        match text.as_bytes() {
            b"." => return ctx.host.current_line(),
            b"$" => return ctx.host.line_count(),
            _ => {}
        }
    }
    ops::to_number(value)
}

fn setline(ctx: &mut Ctx, args: &[Value]) -> Result<Value> {
    let line = line_number(ctx, arg(args, 0))?;
    match arg(args, 1) {
        Value::List(list) => {
            let items = list
                .try_borrow()
                .map(|list| list.items.clone())
                .unwrap_or_default();
            for (offset, item) in items.iter().enumerate() {
                let text = string_arg(item)?;
                ctx.host.set_line(line + offset as i64, &text)?;
            }
        }
        other => {
            let text = string_arg(other)?;
            ctx.host.set_line(line, &text)?;
        }
    }
    Ok(Value::Number(0))
}

fn min_max(name: &str, args: &[Value]) -> Result<Value> {
    let values: Vec<Value> = if args.len() == 1 {
        match arg(args, 0) {
            Value::List(list) => list
                .try_borrow()
                .map(|list| list.items.clone())
                .unwrap_or_default(),
            other => vec![other.clone()],
        }
    } else {
        args.to_vec()
    };
    if values.is_empty() {
        return Ok(Value::Number(0));
    }
    let mut best = values[0].clone();
    for value in values.iter().skip(1) {
        let ordering = ops::compare_ordering(value, &best, false)?;
        let replace = if name == "min" {
            ordering == Ordering::Less
        } else {
            ordering == Ordering::Greater
        };
        if replace {
            best = value.clone();
        }
    }
    Ok(best)
}

fn find_bytes(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(start.min(haystack.len()));
    }
    if start > haystack.len() {
        return None;
    }
    haystack[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| position + start)
}

fn rfind_bytes(haystack: &[u8], needle: &[u8], end: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(end.min(haystack.len()));
    }
    let end = end.min(haystack.len());
    haystack[..end]
        .windows(needle.len())
        .rposition(|window| window == needle)
}

fn normalize_index(index: i64, len: usize) -> Option<usize> {
    let len = len as i64;
    let index = if index < 0 { len + index } else { index };
    if index < 0 || index >= len {
        None
    } else {
        Some(index as usize)
    }
}

fn patch_available(feature: &str) -> bool {
    let Some(version) = feature.strip_prefix("patch-") else {
        return false;
    };
    let mut parts = version
        .split('.')
        .map(|part| part.parse::<i64>().unwrap_or(0));
    let value = parts.next().unwrap_or(0) * 1_000_000
        + parts.next().unwrap_or(0) * 10_000
        + parts.next().unwrap_or(0);
    let current = 9021338;
    value <= current
}

fn char_string(number: i64) -> VimString {
    match char::from_u32((number & 0x1f_ffff) as u32) {
        Some(character) => {
            let mut buffer = [0u8; 4];
            VimString::from(character.encode_utf8(&mut buffer).as_bytes())
        }
        None => VimString::from(vec![(number & 0xff) as u8]),
    }
}

fn register_name(value: &VimString) -> Result<char> {
    value
        .to_string_lossy()
        .chars()
        .next()
        .ok_or_else(|| VimError::new(158, "Invalid register name"))
}

fn utf8_char_len(bytes: &[u8], index: usize) -> usize {
    let Some(byte) = bytes.get(index) else {
        return 1;
    };
    if byte & 0x80 == 0 {
        1
    } else if byte & 0xE0 == 0xC0 {
        2
    } else if byte & 0xF0 == 0xE0 {
        3
    } else if byte & 0xF8 == 0xF0 {
        4
    } else {
        1
    }
}
