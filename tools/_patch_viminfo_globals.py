import pathlib
p = pathlib.Path("crates/rvim-editor/src/viminfo.rs")
s = p.read_text()
# write globals before file marks
old = """    out.push_str("\\n# File marks:\\n");"""
new = """    out.push_str("\\n# Global variables:\\n");
    let globals = editor.viml.globals();
    for entry in &globals.entries {
        let name = String::from_utf8_lossy(entry.key.as_bytes()).into_owned();
        if name.contains('\\t') || name.contains('\\n') {
            continue;
        }
        match &entry.value {
            rvim_viml::Value::Number(value) => {
                out.push_str(&format!("!N:{name}\\t{value}\\n"));
            }
            rvim_viml::Value::Bool(value) => {
                out.push_str(&format!("!B:{name}\\t{}\\n", i32::from(*value)));
            }
            rvim_viml::Value::String(value) => {
                let text = String::from_utf8_lossy(value.as_bytes()).into_owned();
                if !text.contains('\\t') && !text.contains('\\n') {
                    out.push_str(&format!("!S:{name}\\t{text}\\n"));
                }
            }
            _ => {}
        }
    }

    out.push_str("\\n# File marks:\\n");"""
assert old in s
s = s.replace(old, new, 1)
# parse ! before mark parse branch
old2 = """        } else if let Some(rest) = line.strip_prefix('\\'') {"""
new2 = """        } else if let Some(rest) = line.strip_prefix('!') {
            let Some((kind, rest)) = rest.split_once(':') else {
                continue;
            };
            let Some((name, value)) = rest.split_once('\\t') else {
                continue;
            };
            if name.is_empty() {
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
            }
        } else if let Some(rest) = line.strip_prefix('\\'') {"""
assert old2 in s
s = s.replace(old2, new2, 1)
# Parsed field
s = s.replace("""    marks: Vec<(char, u32, u32, String, u64)>,
}""", """    marks: Vec<(char, u32, u32, String, u64)>,
    globals: Vec<(String, rvim_viml::Value)>,
}""", 1)
# apply globals in read after registers
old3 = """    let current_name = editor
        .current_buffer()
        .map(|buffer| buffer.name_string())
        .unwrap_or_default();"""
new3 = """    for (name, value) in parsed.globals {
        editor.viml.set_global(&name, value);
    }
    let current_name = editor
        .current_buffer()
        .map(|buffer| buffer.name_string())
        .unwrap_or_default();"""
assert old3 in s
s = s.replace(old3, new3, 1)
p.write_text(s)
print("patched")
