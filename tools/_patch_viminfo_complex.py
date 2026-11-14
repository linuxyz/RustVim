import pathlib
p = pathlib.Path("crates/rvim-editor/src/viminfo.rs")
s = p.read_text()
s = s.replace("""    globals: Vec<(String, rvim_viml::Value)>,
    buffers: Vec<String>,""", """    globals: Vec<(String, rvim_viml::Value)>,
    global_exprs: Vec<(String, String)>,
    buffers: Vec<String>,""", 1)
# parse !V
old = """            let value = match kind {
                "N" => value.parse::<i64>().ok().map(rvim_viml::Value::Number),
                "B" => value
                    .parse::<i64>()
                    .ok()
                    .map(|value| rvim_viml::Value::Bool(value != 0)),
                "S" => Some(rvim_viml::Value::string(value)),
                _ => None,
            };
            if let Some(value) = value {
                parsed.globals.push((name.to_string(), value));
            }"""
new = """            if kind == "V" {
                parsed.global_exprs.push((name.to_string(), value.to_string()));
                continue;
            }
            let value = match kind {
                "N" => value.parse::<i64>().ok().map(rvim_viml::Value::Number),
                "B" => value
                    .parse::<i64>()
                    .ok()
                    .map(|value| rvim_viml::Value::Bool(value != 0)),
                "S" => Some(rvim_viml::Value::string(value)),
                _ => None,
            };
            if let Some(value) = value {
                parsed.globals.push((name.to_string(), value));
            }"""
assert old in s
s = s.replace(old, new, 1)
# writer fallback
old2 = """            _ => {}
        }
    }

    out.push_str("\\n# Buffer list:\\n");"""
new2 = """            _ => {
                if let Some(expr) = value_expr(&entry.value) {
                    out.push_str(&format!("!V:{name}\\t{expr}\\n"));
                }
            }
        }
    }

    out.push_str("\\n# Buffer list:\\n");"""
assert old2 in s
s = s.replace(old2, new2, 1)
# read apply exprs after simple globals
old3 = """    for (name, value) in parsed.globals {
        editor.viml.set_global(&name, value);
    }"""
new3 = """    for (name, value) in parsed.globals {
        editor.viml.set_global(&name, value);
    }
    for (name, expr) in parsed.global_exprs {
        let source = format!("let g:{name} = {expr}\\n");
        let _ = editor.source_viml(source.as_bytes(), "[viminfo]");
    }"""
assert old3 in s
s = s.replace(old3, new3, 1)
# helpers before unix_now
s = s.replace("""fn unix_now() -> u64 {""", """fn value_expr(value: &rvim_viml::Value) -> Option<String> {
    match value {
        rvim_viml::Value::Number(value) => Some(value.to_string()),
        rvim_viml::Value::Float(value) => Some(format!("{value}")),
        rvim_viml::Value::Bool(value) => Some(if *value {
            "v:true".to_string()
        } else {
            "v:false".to_string()
        }),
        rvim_viml::Value::String(value) => Some(quote_vim_string(value.as_bytes())),
        rvim_viml::Value::List(list) => {
            let list = list.try_borrow().ok()?;
            let items: Option<Vec<String>> = list.items.iter().map(value_expr).collect();
            Some(format!("[{}]", items?.join(", ")))
        }
        rvim_viml::Value::Dict(dict) => {
            let dict = dict.try_borrow().ok()?;
            let mut entries = Vec::new();
            for entry in &dict.entries {
                entries.push(format!(
                    "{}: {}",
                    quote_vim_string(entry.key.as_bytes()),
                    value_expr(&entry.value)?
                ));
            }
            Some(format!("{{{}}}", entries.join(", ")))
        }
        _ => None,
    }
}

fn quote_vim_string(bytes: &[u8]) -> String {
    let mut out = String::from("'");
    for byte in bytes {
        match byte {
            b'\\'' => out.push_str("''"),
            b'\\n' => out.push_str("\\\\n"),
            b'\\t' => out.push_str("\\\\t"),
            b'\\\\' => out.push_str("\\\\\\\\"),
            _ => out.push(*byte as char),
        }
    }
    out.push('\\'');
    out
}

fn unix_now() -> u64 {""", 1)
p.write_text(s)
print("patched")
