import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
old = """        let mut add_indent = (smartindent || cindent) && trimmed.last() == Some(&b'{');
        if cindent && !add_indent {
            let text = String::from_utf8_lossy(&trimmed);
            let cinwords = self.options.buffer_string(buffer_id, "cinwords");
            add_indent = cinwords.split(',').any(|word| {
                !word.is_empty()
                    && text.trim_start().strip_prefix(word).is_some_and(|rest| {
                        rest.is_empty()
                            || rest.starts_with(' ')
                            || rest.starts_with('(')
                            || rest.starts_with(':')
                    })
            }) && trimmed.last() != Some(&b'}');
        }
        if add_indent {
            if expandtab {
                indent.extend(std::iter::repeat_n(b' ', shiftwidth));
            } else {
                indent.push(b'\\t');
            }
        }
        if cindent && trimmed.first() == Some(&b'}') && indent.len() >= shiftwidth {
            indent.truncate(indent.len() - shiftwidth);
        }"""
new = """        let cino = self.options.buffer_string(buffer_id, "cinoptions");
        let mut add_amount = 0i32;
        let mut add_indent = (smartindent || cindent) && trimmed.last() == Some(&b'{');
        if cindent && !add_indent {
            let text = String::from_utf8_lossy(&trimmed);
            let cinwords = self.options.buffer_string(buffer_id, "cinwords");
            add_indent = cinwords.split(',').any(|word| {
                !word.is_empty()
                    && text.trim_start().strip_prefix(word).is_some_and(|rest| {
                        rest.is_empty()
                            || rest.starts_with(' ')
                            || rest.starts_with('(')
                            || rest.starts_with(':')
                    })
            }) && trimmed.last() != Some(&b'}');
        }
        if add_indent {
            if smartindent && !cindent {
                add_amount = shiftwidth as i32;
            } else {
                add_amount = cino_amount(&cino, '>', shiftwidth).unwrap_or(shiftwidth as i32);
                if trimmed.last() == Some(&b'{') {
                    add_amount += cino_amount(&cino, 'e', shiftwidth).unwrap_or(0);
                } else {
                    add_amount += cino_amount(&cino, 'n', shiftwidth).unwrap_or(0);
                }
            }
        }
        if add_amount > 0 {
            let amount = add_amount as usize;
            if expandtab {
                indent.extend(std::iter::repeat_n(b' ', amount));
            } else {
                let tabs = amount / shiftwidth.max(1);
                let spaces = amount % shiftwidth.max(1);
                indent.extend(std::iter::repeat_n(b'\\t', tabs));
                indent.extend(std::iter::repeat_n(b' ', spaces));
            }
        } else if add_amount < 0 {
            let amount = (-add_amount) as usize;
            indent.truncate(indent.len().saturating_sub(amount));
        }
        if cindent && trimmed.first() == Some(&b'}') {
            let close = cino_amount(&cino, '}', shiftwidth).unwrap_or(shiftwidth as i32).max(0) as usize;
            indent.truncate(indent.len().saturating_sub(close));
        }"""
assert old in s
s = s.replace(old, new, 1)
# helper function before newline_indent? add free fn near bottom before impl? Insert before fn newline_indent
anchor = """    fn newline_indent(&mut self) -> String {"""
helper = """    fn newline_indent(&mut self) -> String {"""
# add module-level helper before impl block? easier add as associated? cino_amount free fn at file end before tests
s += '''

fn cino_amount(options: &str, key: char, shiftwidth: usize) -> Option<i32> {
    let bytes = options.as_bytes();
    let index = bytes.iter().position(|byte| *byte as char == key)?;
    let mut value = String::new();
    let mut position = index + 1;
    while position < bytes.len() {
        let ch = bytes[position] as char;
        if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' {
            value.push(ch);
            position += 1;
        } else if ch == 's' {
            position += 1;
            break;
        } else {
            break;
        }
    }
    if value.is_empty() {
        return None;
    }
    let mut amount = value.parse::<f64>().ok()?;
    let scaled = position > index + 1 && bytes.get(position - 1) == Some(&b's');
    if scaled {
        amount *= shiftwidth as f64;
    }
    Some(amount.round() as i32)
}
'''
p.write_text(s)
print("patched")
