import pathlib
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
old_start = s.index('        "map" | "nmap" | "nnoremap" | "noremap" | "imap" | "inoremap" | "vmap" | "vnoremap"')
old_end = s.index('        "unmap" | "nunmap"', old_start)
new = '''        "map" | "nmap" | "nnoremap" | "noremap" | "imap" | "inoremap" | "vmap" | "vnoremap"
        | "cmap" | "cnoremap" | "xmap" | "xnoremap" | "smap" | "snoremap" | "omap" | "onoremap" => {
            let name = command.name.to_ascii_lowercase();
            let bang = command.bang;
            let modes = if bang {
                vec![MapMode::Insert, MapMode::Command]
            } else {
                match name.as_str() {
                    "imap" | "inoremap" => vec![MapMode::Insert],
                    "cmap" | "cnoremap" => vec![MapMode::Command],
                    "vmap" | "vnoremap" | "smap" | "snoremap" | "xmap" | "xnoremap" => {
                        vec![MapMode::Visual]
                    }
                    "omap" | "onoremap" => vec![MapMode::Operator],
                    "nmap" | "nnoremap" => vec![MapMode::Normal],
                    _ => vec![MapMode::Normal, MapMode::Visual],
                }
            };
            let mut args = command.args.trim();
            let mut options = crate::keymap::MappingOptions::default();
            loop {
                if !args.starts_with('<') {
                    break;
                }
                let Some(end) = args.find('>') else {
                    break;
                };
                let token = args[1..end].to_ascii_lowercase();
                match token.as_str() {
                    "buffer" => options.buffer = Some(editor.current_buffer),
                    "nowait" => options.nowait = true,
                    "silent" => options.silent = true,
                    "unique" => options.unique = true,
                    "script" | "special" => {}
                    "expr" => {
                        return Err(VimError::new(
                            474,
                            "E474: <expr> mappings are not implemented",
                        ))
                    }
                    _ => break,
                }
                args = args[end + 1..].trim_start();
            }
            if args.is_empty() {
                let mut text = String::new();
                for mode in &modes {
                    for mapping in editor.mappings.list(*mode) {
                        if options.buffer.is_some() && mapping.buffer != options.buffer {
                            continue;
                        }
                        text.push_str(&crate::keymap::format_keys(&mapping.lhs));
                        text.push_str("  ");
                        text.push_str(&crate::keymap::format_keys(&mapping.rhs));
                        text.push('\\n');
                    }
                }
                editor.messages.info(text.trim_end().to_string());
            } else {
                let (lhs_text, rhs_text) =
                    args.split_once(char::is_whitespace).unwrap_or((args, ""));
                let lhs = crate::keymap::parse_keys(lhs_text);
                let rhs = crate::keymap::parse_keys(rhs_text.trim_start());
                let noremap = name.contains("nore");
                for mode in modes {
                    if !editor
                        .mappings
                        .add(mode, lhs.clone(), rhs.clone(), noremap, options.clone())
                    {
                        return Err(VimError::new(227, "E227: Mapping already exists"));
                    }
                }
            }
        }
'''
s = s[:old_start] + new + s[old_end:]
p.write_text(s)
print("ex patched")
