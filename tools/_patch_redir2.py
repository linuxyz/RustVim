import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""pub struct Editor {
    pub buffers: Arena<BufferEntry>,""", """enum RedirTarget {
    File(std::path::PathBuf, bool),
    Register(char),
    Variable(String),
}

pub struct Editor {
    pub buffers: Arena<BufferEntry>,""", 1)
s = s.replace("""    pub(crate) noautocmd_depth: u32,""", """    pub(crate) redir_target: Option<RedirTarget>,
    pub(crate) noautocmd_depth: u32,""", 1)
s = s.replace("""            noautocmd_depth: 0,""", """            redir_target: None,
            noautocmd_depth: 0,""", 1)
p.write_text(s)
print("editor patched")
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
anchor = '''        "messages" | "mes" | "messa" => {'''
arm = '''        "redir" | "redi" | "red" => {
            let arg = command.args.trim();
            if arg.eq_ignore_ascii_case("END") {
                let target = editor.redir_target.take();
                let capture = editor.messages.capture.take().unwrap_or_default();
                match target {
                    Some(crate::editor::RedirTarget::File(path, append)) => {
                        let mut bytes = if append {
                            rvim_platform::fs::read_file(&path).unwrap_or_default()
                        } else {
                            Vec::new()
                        };
                        bytes.extend_from_slice(capture.as_bytes());
                        rvim_platform::fs::write_file_atomic(&path, &bytes)
                            .map_err(|error| VimError::new(13, error.to_string()))?;
                    }
                    Some(crate::editor::RedirTarget::Register(ch)) => {
                        if let Some(name) = RegisterName::parse(ch) {
                            editor.registers.set(
                                name,
                                RegisterContent::new(
                                    vec![VimString::from(capture)],
                                    RegisterKind::Characterwise,
                                ),
                            );
                        }
                    }
                    Some(crate::editor::RedirTarget::Variable(name)) => {
                        editor.viml.set_global(
                            name.trim_start_matches("g:"),
                            rvim_viml::Value::string(capture),
                        );
                    }
                    None => return Err(VimError::new(474, "E474: Invalid argument")),
                }
                return Ok(());
            }
            if editor.messages.capture.is_some() {
                return Err(VimError::new(474, "E474: Invalid argument"));
            }
            let target = if let Some(rest) = arg.strip_prefix(">>") {
                crate::editor::RedirTarget::File(expand_path_arg(rest.trim()), true)
            } else if let Some(rest) = arg.strip_prefix('>') {
                crate::editor::RedirTarget::File(expand_path_arg(rest.trim()), false)
            } else if let Some(rest) = arg.strip_prefix('@') {
                let ch = rest.trim().chars().next().unwrap_or('"');
                crate::editor::RedirTarget::Register(ch)
            } else if let Some(rest) = arg.strip_prefix("=>") {
                crate::editor::RedirTarget::Variable(rest.trim().to_string())
            } else {
                return Err(VimError::new(474, "E474: Invalid argument"));
            };
            editor.redir_target = Some(target);
            editor.messages.capture = Some(String::new());
        }
        "messages" | "mes" | "messa" => {'''
assert anchor in s
s = s.replace(anchor, arm, 1)
p.write_text(s)
print("ex patched")
