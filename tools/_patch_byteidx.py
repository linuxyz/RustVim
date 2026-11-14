import pathlib
p = pathlib.Path("crates/rvim-viml/src/builtins.rs")
s = p.read_text()
# add names after strcharpart
s = s.replace('''    "strcharpart",''', '''    "strcharpart",
    "byteidx",
    "byteidxcomp",
    "charidx",''', 1)
anchor = '''        "strcharpart" => {'''
arms = '''        "byteidx" | "byteidxcomp" => {
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
        "strcharpart" => {'''
assert anchor in s
s = s.replace(anchor, arms, 1)
s += '''

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
'''
p.write_text(s)
print("patched")
