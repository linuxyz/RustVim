import pathlib
p = pathlib.Path("crates/rvim-viml/src/builtins.rs")
s = p.read_text()
s = s.replace('''    "matchstr",''', '''    "matchstr",
    "matchstrpos",
    "matchlist",''', 1)
anchor = '''        "matchend" => {'''
arms = '''        "matchstrpos" => {
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
            let result = found.map_or_else(
                Vec::new,
                |found| {
                    let mut items = vec![Value::String(VimString::from(
                        &text.as_bytes()[found.start..found.end],
                    ))];
                    for capture in &found.captures {
                        items.push(match capture {
                            Some((start, end)) => Value::String(VimString::from(
                                &text.as_bytes()[*start..*end],
                            )),
                            None => Value::string(""),
                        });
                    }
                    items
                },
            );
            Ok(Value::list(result))
        }
        "matchend" => {'''
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
