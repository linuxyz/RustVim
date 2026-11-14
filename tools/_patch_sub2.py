import pathlib
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
start = s.index("fn expand_replacement(replacement: &str, line: &[u8], found: &rvim_regex::Match) -> Vec<u8> {")
end = s.index("fn handle_highlight(editor: &mut Editor, args: &str)", start)
new = r'''fn apply_replacement_case(out: &mut Vec<u8>, text: &str, mode: &mut u8, one: &mut u8) {
    for ch in text.chars() {
        let cased = if *one == 1 {
            *one = 0;
            ch.to_uppercase().next().unwrap_or(ch)
        } else if *one == 2 {
            *one = 0;
            ch.to_lowercase().next().unwrap_or(ch)
        } else if *mode == 1 {
            ch.to_uppercase().next().unwrap_or(ch)
        } else if *mode == 2 {
            ch.to_lowercase().next().unwrap_or(ch)
        } else {
            ch
        };
        let mut buffer = [0u8; 4];
        out.extend_from_slice(cased.encode_utf8(&mut buffer).as_bytes());
    }
}

fn expand_replacement(replacement: &str, line: &[u8], found: &rvim_regex::Match) -> Vec<u8> {
    let matched = &line[found.start..found.end];
    let mut out = Vec::new();
    let mut chars = replacement.chars().peekable();
    let mut mode = 0u8;
    let mut one = 0u8;
    while let Some(ch) = chars.next() {
        match ch {
            '&' => apply_replacement_case(
                &mut out,
                &String::from_utf8_lossy(matched),
                &mut mode,
                &mut one,
            ),
            '\\' => match chars.next() {
                Some('&') => out.push(b'&'),
                Some('\\') => out.push(b'\\'),
                Some('n') => out.push(b'\n'),
                Some('t') => out.push(b'\t'),
                Some('u') => one = 1,
                Some('l') => one = 2,
                Some('U') => mode = 1,
                Some('L') => mode = 2,
                Some('e') | Some('E') => {
                    mode = 0;
                    one = 0;
                }
                Some(digit @ '0'..='9') => {
                    let index = digit as usize - '0' as usize;
                    if index == 0 {
                        apply_replacement_case(
                            &mut out,
                            &String::from_utf8_lossy(matched),
                            &mut mode,
                            &mut one,
                        );
                    } else if let Some(Some((start, end))) = found.captures.get(index - 1) {
                        apply_replacement_case(
                            &mut out,
                            &String::from_utf8_lossy(&line[*start..*end]),
                            &mut mode,
                            &mut one,
                        );
                    }
                }
                Some(other) => {
                    let mut buffer = [0u8; 4];
                    apply_replacement_case(
                        &mut out,
                        other.encode_utf8(&mut buffer),
                        &mut mode,
                        &mut one,
                    );
                }
                None => out.push(b'\\'),
            },
            other => {
                let mut buffer = [0u8; 4];
                apply_replacement_case(
                    &mut out,
                    other.encode_utf8(&mut buffer),
                    &mut mode,
                    &mut one,
                );
            }
        }
    }
    out
}

'''
s = s[:start] + new + s[end:]
p.write_text(s)
print("patched")
