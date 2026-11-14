use std::path::PathBuf;

use rvim_edit::{RangeKind, RegisterContent, RegisterKind, RegisterName, TextRange};
use rvim_ex::{AddressKind, ExCommand, Modifier, RangeSpec, ResolvedRange};
use rvim_types::{BufPos, VimError, VimString};
use rvim_view::SplitDir;

use crate::editor::{CommandKind, Editor, Mode, Operator};
use crate::keymap::MapMode;
use crate::usercmd::Nargs;

pub fn execute(editor: &mut Editor, line: &str) -> Result<(), VimError> {
    let command = rvim_ex::parse(line).map_err(|error| VimError::plain(error.to_string()))?;
    execute_command(editor, command)
}

fn execute_command(editor: &mut Editor, mut command: ExCommand) -> Result<(), VimError> {
    let modifiers = std::mem::take(&mut command.modifiers);
    let previous_filter = editor.messages.filter.take();
    if let Some(filter) = command.filter.take() {
        editor.messages.filter = Some(filter);
    }
    let mut silent = false;
    let mut silent_bang = false;
    let mut noautocmd = false;
    let mut keepjumps = false;
    let mut tab = false;
    let mut vertical = false;
    let mut sandbox = false;
    for modifier in &modifiers {
        match modifier {
            Modifier::Silent => silent = true,
            Modifier::SilentBang => {
                silent = true;
                silent_bang = true;
            }
            Modifier::Unsilent => silent = false,
            Modifier::NoAutocmd => noautocmd = true,
            Modifier::KeepJumps => keepjumps = true,
            Modifier::Tab => tab = true,
            Modifier::Vertical => vertical = true,
            Modifier::Sandbox => sandbox = true,
            _ => {}
        }
    }
    if tab {
        editor.tab_new(None)?;
    }
    let previous_silent = editor.messages.silent;
    if silent {
        editor.messages.silent = true;
    }
    if noautocmd {
        editor.noautocmd_depth += 1;
    }
    if keepjumps {
        editor.keepjumps_depth += 1;
    }
    if sandbox {
        editor.sandbox_depth += 1;
    }
    let previous_vertical = editor.vertical_requested;
    editor.vertical_requested = vertical;
    let result = execute_command_inner(editor, command);
    editor.vertical_requested = previous_vertical;
    if keepjumps {
        editor.keepjumps_depth = editor.keepjumps_depth.saturating_sub(1);
    }
    if noautocmd {
        editor.noautocmd_depth = editor.noautocmd_depth.saturating_sub(1);
    }
    if sandbox {
        editor.sandbox_depth = editor.sandbox_depth.saturating_sub(1);
    }
    editor.messages.silent = previous_silent;
    editor.messages.filter = previous_filter;
    if silent_bang && result.is_err() {
        editor.messages.error = false;
        editor.messages.last = None;
        return Ok(());
    }
    result
}

/// True for Ex commands that operate on the current window's location list.
fn is_location_command(name: &str) -> bool {
    matches!(
        name,
        "lopen"
            | "lop"
            | "lwindow"
            | "lw"
            | "lclose"
            | "lcl"
            | "lclose!"
            | "lnext"
            | "lne"
            | "lprevious"
            | "lprev"
            | "lfirst"
            | "lfir"
            | "lrewind"
            | "lrew"
            | "llast"
            | "lla"
            | "ll"
            | "lolder"
            | "lol"
            | "lnewer"
            | "lnew"
            | "lfile"
            | "lf"
            | "lgetexpr"
            | "lgetfile"
            | "lbuffer"
            | "lgetbuffer"
            | "laddexpr"
            | "laddbuffer"
            | "laddfile"
            | "lvimgrep"
            | "lvim"
            | "lhistory"
            | "llist"
    )
}
/// Rejects :set of options flagged P_SECURE while sourcing a secure exrc or
/// inside :sandbox, matching Vim's E12/E48 checks.
fn secure_option_error(editor: &Editor, args: &str) -> Option<VimError> {
    let sandbox = editor.sandbox_depth > 0;
    if !sandbox && !editor.secure_sourcing {
        return None;
    }
    for token in args.split_whitespace() {
        if token.ends_with('?') {
            continue;
        }
        let name = token
            .trim_start_matches("no")
            .trim_start_matches("inv")
            .split(['=', '+', '-', '^', '&', '<'])
            .next()
            .unwrap_or("");
        if name.is_empty() || name == "all" {
            continue;
        }
        let spec = crate::options::spec(name).or_else(|| crate::options::spec_by_short(name));
        let Some(spec) = spec else { continue };
        if spec.flags.contains("P_SECURE") {
            return Some(if sandbox {
                VimError::new(48, "Not allowed in sandbox")
            } else {
                VimError::new(
                    12,
                    "Command not allowed from exrc/vimrc in current dir or tag search",
                )
            });
        }
    }
    None
}
/// Commands that Vim refuses while sourcing a "secure" local .vimrc/.exrc.
fn secure_disallowed(command: &ExCommand) -> bool {
    let name = command.name.as_str();
    let write = matches!(
        name,
        "w" | "wr"
            | "wri"
            | "writ"
            | "write"
            | "wa"
            | "wall"
            | "wq"
            | "wqa"
            | "wqall"
            | "x"
            | "xi"
            | "xit"
            | "exi"
            | "exit"
            | "saveas"
            | "sav"
            | "savea"
            | "update"
            | "up"
            | "mksession"
            | "mks"
            | "mkview"
            | "wviminfo"
            | "wv"
    );
    let shell = name == "!";
    let autocmd = matches!(name, "autocmd" | "au" | "augroup" | "aug");
    let read_filter = command.bang && matches!(name, "r" | "re" | "rea" | "read");
    write || shell || autocmd || read_filter
}
fn execute_command_inner(editor: &mut Editor, command: ExCommand) -> Result<(), VimError> {
    if editor.secure_sourcing && secure_disallowed(&command) {
        return Err(VimError::new(
            12,
            "Command not allowed from exrc/vimrc in current dir or tag search",
        ));
    }
    if editor.sandbox_depth > 0 && secure_disallowed(&command) {
        return Err(VimError::new(48, "Not allowed in sandbox"));
    }
    if is_location_command(&command.name)
        && editor.loclist_saved.is_none()
        && editor.begin_loclist_command().is_some()
    {
        let result = execute_command_inner(editor, command);
        editor.end_loclist_command();
        return result;
    }
    if let Some(user) = editor.user_commands.get(&command.name).cloned() {
        return execute_user_command(editor, &command, &user);
    }
    let name = command.name.as_str();
    if name.is_empty() {
        if command.range.addresses.is_empty() {
            return Ok(());
        }
        let range = resolve_range(editor, &command.range)?;
        editor.goto_line(range.end);
        return Ok(());
    }
    let full = command.name.to_ascii_lowercase();
    let command = ExCommand {
        name: full.clone(),
        ..command
    };
    match full.as_str() {
        "e" | "ed" | "edi" | "edit" => {
            let arg = command.args.trim();
            if arg.is_empty() {
                if command.bang {
                    reload_current(editor)?;
                } else {
                    let path = editor
                        .current_buffer()
                        .and_then(|buffer| buffer.text.meta.name.clone())
                        .ok_or_else(|| VimError::new(32, "No file name"))?;
                    check_modified(editor, false)?;
                    editor.open_path(&path)?;
                }
            } else {
                check_modified(editor, command.bang)?;
                editor.open_path(&resolve_file_arg(editor, arg)?)?;
            }
        }
        "w" | "wr" | "wri" | "writ" | "write" => {
            let mut arg = command.args.trim();
            let append = arg.starts_with(">>");
            if append {
                arg = arg[2..].trim();
            }
            if let Some(shell) = arg.strip_prefix('!') {
                let bytes = if command.range.addresses.is_empty() {
                    editor
                        .current_buffer()
                        .map(|buffer| buffer.text.to_file_bytes())
                        .unwrap_or_default()
                } else {
                    let range = resolve_range(editor, &command.range)?;
                    editor
                        .current_buffer()
                        .map(|buffer| buffer.text.to_file_bytes_range(range.start, range.end))
                        .unwrap_or_default()
                };
                let output = editor
                    .run_shell(shell, Some(&bytes))
                    .map_err(|error| VimError::new(13, error.to_string()))?;
                if !output.stdout.is_empty() {
                    editor
                        .messages
                        .info(String::from_utf8_lossy(&output.stdout).into_owned());
                } else if output.status != 0 {
                    return Err(VimError::new(
                        13,
                        String::from_utf8_lossy(&output.stderr).into_owned(),
                    ));
                }
                return Ok(());
            }
            check_readonly(editor, command.bang)?;
            if !command.range.addresses.is_empty() {
                let range = resolve_range(editor, &command.range)?;
                editor.write_range(
                    range.start,
                    range.end,
                    (!arg.is_empty()).then_some(arg),
                    append,
                )?;
            } else {
                editor.write_buffer((!arg.is_empty()).then_some(arg), append)?;
            }
        }
        "q" | "qu" | "qui" | "quit" => {
            if editor.cmdwin.is_some() {
                editor.close_cmdwin();
                return Ok(());
            }
            check_modified(editor, command.bang)?;
            editor.quit_window(command.bang);
        }
        "qa" | "qall" => {
            if !command.bang && editor.any_modified() {
                return Err(VimError::new(
                    37,
                    "No write since last change (add ! to override)",
                ));
            }
            editor.quit(command.bang);
        }
        "wa" | "wall" => {
            let count = editor.write_all_buffers()?;
            editor.messages.info(format!("{count} buffers written"));
        }
        "wq" | "wqa" | "wqall" | "x" | "xi" | "xit" | "exi" | "exit" => {
            let all = matches!(command.name.to_ascii_lowercase().as_str(), "wqa" | "wqall");
            if all {
                editor.write_all_buffers()?;
                editor.quit(command.bang);
            } else {
                check_readonly(editor, command.bang)?;
                editor.write_buffer(None, false)?;
                editor.quit_window(command.bang);
            }
        }
        "CompilerSet" | "compilerset" | "compilerse" => {
            let buffer_id = editor.current_buffer_id();
            let window_id = editor.current_window_id();
            crate::options::execute_set(
                &mut editor.options,
                command.args.trim(),
                true,
                buffer_id,
                window_id,
                false,
            )
            .map_err(VimError::plain)?;
            editor.sync_window_options();
        }
        "set" | "se" => {
            if let Some(error) = secure_option_error(editor, command.args.trim()) {
                return Err(error);
            }
            let buffer_id = editor.current_buffer_id();
            let window_id = editor.current_window_id();
            let result = crate::options::execute_set(
                &mut editor.options,
                command.args.trim(),
                false,
                buffer_id,
                window_id,
                false,
            );
            // Options before a bad argument were already applied; refresh the
            // window mirror even when the command reports an error.
            editor.sync_window_options_public();
            let result = result.map_err(VimError::plain)?;
            if command
                .args
                .split_whitespace()
                .any(|arg| arg.starts_with("buftype=") || arg.starts_with("bt="))
            {
                let id = editor.current_buffer;
                let value = editor.options.buffer_string(id.raw(), "buftype");
                if let Some(buffer) = editor.buffers.get_mut(id) {
                    buffer.buftype = value;
                }
            }

            for message in result.messages {
                editor.messages.info(message);
            }
            if command
                .args
                .split_whitespace()
                .any(|arg| arg.starts_with("filetype=") || arg.starts_with("ft="))
            {
                editor.refresh_filetype();
            }
            if command
                .args
                .split_whitespace()
                .any(|arg| arg.starts_with("runtimepath=") || arg.starts_with("rtp="))
            {
                let rtp = editor.options.get_string("runtimepath");
                editor.runtime = rvim_runtime::RuntimePaths::discover(
                    rvim_platform::env::vimruntime(),
                    Some(rtp),
                );
            }
            if command
                .args
                .split_whitespace()
                .any(|arg| arg.starts_with("undolevels=") || arg.starts_with("ul="))
            {
                editor.apply_undolevels();
            }
            if command
                .args
                .split_whitespace()
                .any(|arg| arg.starts_with("tags="))
            {
                let tags = editor.options.get_string("tags");
                let first = tags.split(',').next().unwrap_or("");
                if !first.is_empty() {
                    editor.tag_index = crate::tags::TagIndex::load(std::path::Path::new(first));
                }
            }
            editor.fire_event(crate::autocmd::Event::OptionSet);
        }
        "setlocal" | "setl" => {
            if let Some(error) = secure_option_error(editor, command.args.trim()) {
                return Err(error);
            }
            let buffer_id = editor.current_buffer_id();
            let window_id = editor.current_window_id();
            let result = crate::options::execute_set(
                &mut editor.options,
                command.args.trim(),
                true,
                buffer_id,
                window_id,
                false,
            )
            .map_err(VimError::plain)?;
            for message in result.messages {
                editor.messages.info(message);
            }
            editor.sync_window_options_public();
            editor.fire_event(crate::autocmd::Event::OptionSet);
        }
        "setglobal" | "setg" => {
            if let Some(error) = secure_option_error(editor, command.args.trim()) {
                return Err(error);
            }
            let buffer_id = editor.current_buffer_id();
            let window_id = editor.current_window_id();
            let result = crate::options::execute_set(
                &mut editor.options,
                command.args.trim(),
                false,
                buffer_id,
                window_id,
                true,
            )
            .map_err(VimError::plain)?;
            for message in result.messages {
                editor.messages.info(message);
            }
            editor.sync_window_options_public();
            editor.fire_event(crate::autocmd::Event::OptionSet);
        }
        "undo" | "u" => {
            let count = command.args.trim().parse::<u32>().unwrap_or(1).max(1);
            for _ in 0..count {
                editor.undo_public();
            }
        }
        "redo" => {
            let count = command.args.trim().parse::<u32>().unwrap_or(1).max(1);
            for _ in 0..count {
                editor.redo_public();
            }
        }
        "earlier" | "ea" => {
            let count = command.args.trim().parse::<u32>().unwrap_or(1).max(1);
            for _ in 0..count {
                editor.undo_public();
            }
        }
        "later" | "lat" => {
            let count = command.args.trim().parse::<u32>().unwrap_or(1).max(1);
            for _ in 0..count {
                editor.redo_public();
            }
        }
        "undolist" | "undol" => {
            editor.messages.info(editor.undo_summary());
        }
        "vimgrep" | "vim" | "vimgre" => {
            handle_vimgrep(editor, &command)?;
        }
        "lvimgrep" | "lvim" => {
            handle_vimgrep(editor, &command)?;
        }
        "copen" | "cope" | "copen!" | "cwindow" | "cw" | "lopen" | "lop" | "lwindow" | "lw" => {
            editor.open_quickfix()?
        }
        "cclose" | "ccl" | "cclose!" | "lclose" | "lcl" => editor.close_quickfix()?,
        "cnext" | "cn" | "cnf" | "lnext" | "lne" => editor.quickfix_jump(None)?,
        "colder" | "col" | "lolder" | "lol" => editor.quickfix_older()?,
        "cnewer" | "cnew" | "lnewer" | "lnew" => editor.quickfix_newer()?,
        "cprevious" | "cp" | "cprev" | "lprevious" | "lprev" => editor.quickfix_previous()?,
        "cfirst" | "cfir" | "lfirst" | "lfir" | "lrewind" | "lrew" => {
            editor.quickfix_jump(Some(1))?
        }
        "clast" | "cla" | "llast" | "lla" => {
            let index = editor.quickfix.entries.len();
            editor.quickfix_jump(Some(index))?;
        }
        "cc" | "ll" => {
            let index = command.args.trim().parse::<usize>().ok();
            editor.quickfix_jump(index)?;
        }
        "cfile" | "cf" | "lfile" | "lf" => {
            let path = resolve_file_arg(editor, command.args.trim())?;
            let text = rvim_platform::fs::read_file(&path)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            let entries = editor.parse_quickfix_text(&String::from_utf8_lossy(&text));
            editor.set_quickfix(path.display().to_string(), entries);
        }
        "cgetexpr" | "lgetexpr" => {
            let value = editor.eval_viml(command.args.trim())?;
            let text = match value {
                rvim_viml::Value::List(list) => list
                    .borrow()
                    .items
                    .iter()
                    .map(|item| item.echo_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
                other => other.echo_string(),
            };
            let entries = editor.parse_quickfix_text(&text);
            editor.set_quickfix(":cgetexpr".to_string(), entries);
        }
        "cbuffer" | "lbuffer" | "lgetbuffer" => {
            let mut text = String::new();
            for lnum in 1..=editor.line_count() {
                if let Some(line) = editor.line(lnum) {
                    text.push_str(&String::from_utf8_lossy(line));
                    text.push('\n');
                }
            }
            let entries = editor.parse_quickfix_text(&text);
            editor.set_quickfix(":cbuffer".to_string(), entries);
        }
        "cgetfile" | "lgetfile" => {
            let path = resolve_file_arg(editor, command.args.trim())?;
            let text = rvim_platform::fs::read_file(&path)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            let entries = editor.parse_quickfix_text(&String::from_utf8_lossy(&text));
            editor.set_quickfix(path.display().to_string(), entries);
        }
        "caddfile" | "laddfile" => {
            let path = resolve_file_arg(editor, command.args.trim())?;
            let text = rvim_platform::fs::read_file(&path)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            let entries = editor.parse_quickfix_text(&String::from_utf8_lossy(&text));
            let mut combined = editor.quickfix.entries.clone();
            combined.extend(entries);
            editor.set_quickfix(path.display().to_string(), combined);
        }
        "caddexpr" | "laddexpr" => {
            let value = editor.eval_viml(command.args.trim())?;
            let text = match value {
                rvim_viml::Value::List(list) => list
                    .borrow()
                    .items
                    .iter()
                    .map(|item| item.echo_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
                other => other.echo_string(),
            };
            let entries = editor.parse_quickfix_text(&text);
            let mut combined = editor.quickfix.entries.clone();
            combined.extend(entries);
            editor.set_quickfix(":caddexpr".to_string(), combined);
        }
        "caddbuffer" | "cad" | "laddbuffer" | "lad" => {
            let mut text = String::new();
            for lnum in 1..=editor.line_count() {
                if let Some(line) = editor.line(lnum) {
                    text.push_str(&String::from_utf8_lossy(line));
                    text.push('\n');
                }
            }
            let entries = editor.parse_quickfix_text(&text);
            let mut combined = editor.quickfix.entries.clone();
            combined.extend(entries);
            editor.set_quickfix(":caddbuffer".to_string(), combined);
        }
        "clist" | "lli" | "llist" => {
            let lines = editor.quickfix.lines();
            if lines.is_empty() {
                editor.messages.info("No errors");
            } else {
                for line in lines {
                    editor.messages.info(line);
                }
            }
        }
        "chistory" | "lhistory" => editor.messages.info(format!(
            "quickfix list {} of {}",
            editor.quickfix_stack_index + 1,
            editor.quickfix_stack.len()
        )),
        "compiler" | "comp" | "compi" => {
            let name = command.args.trim();
            if name.is_empty() {
                editor.messages.info("no compiler");
            } else {
                if let Some(path) = editor.runtime.find("compiler/compiler.vim") {
                    if let Ok(bytes) = std::fs::read(&path) {
                        editor.source_viml(&bytes, &path.display().to_string())?;
                    }
                }
                let relative = format!("compiler/{name}.vim");
                if let Some(path) = editor.runtime.find(&relative) {
                    let bytes = std::fs::read(&path)
                        .map_err(|error| VimError::new(13, error.to_string()))?;
                    editor.source_viml(&bytes, &path.display().to_string())?;
                    editor.viml_vars.insert(
                        (
                            rvim_viml::VarScope::Buffer,
                            format!("{}:current_compiler", editor.current_buffer_id()),
                        ),
                        rvim_viml::Value::string(name),
                    );
                } else {
                    return Err(VimError::new(666, format!("Cannot find compiler '{name}'")));
                }
            }
        }
        "make" | "mak" => editor.run_make(command.args.trim())?,
        "grep" | "gre" | "grep!" | "grepadd" | "grepadd!" | "grepa" => {
            let append_list = name.starts_with("grepadd");
            let pattern = command.args.trim();
            if pattern.is_empty() {
                return Err(VimError::new(471, "Argument required"));
            }
            let configured = editor
                .options
                .buffer_string(editor.current_buffer_id(), "grepprg");
            let program = if configured.is_empty() {
                "grep -n $* /dev/null".to_string()
            } else {
                configured
            };
            let quoted = shell_quote(pattern);
            let program = if program.contains("$*") {
                program.replace("$*", &quoted)
            } else {
                format!("{program} {quoted}")
            };
            let output = rvim_platform::run_shell(&program, None)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            let grepformat = editor
                .options
                .buffer_string(editor.current_buffer_id(), "grepformat");
            let entries = if grepformat.is_empty() {
                editor.parse_quickfix_text(&text)
            } else {
                crate::errorformat::parse(&text, &grepformat)
            };
            let title = if append_list {
                format!(":grepadd {pattern}")
            } else {
                format!(":grep {pattern}")
            };
            if append_list {
                let mut combined = editor.quickfix.entries.clone();
                combined.extend(entries);
                editor.set_quickfix(title, combined);
            } else {
                editor.set_quickfix(title, entries);
            }
        }
        "function" | "fu" | "fun" | "func" => {
            let arg = command.args.trim();
            if arg.is_empty() || arg.starts_with('/') {
                let pattern = arg.trim_start_matches('/');
                let mut names = editor.viml.function_names();
                names.retain(|name| pattern.is_empty() || name.contains(pattern));
                names.sort();
                editor.messages.info(names.join("\n"));
            } else {
                return Err(VimError::new(
                    474,
                    ":function definitions require a sourced script",
                ));
            }
        }
        "delfunction" | "delf" | "delfu" => {
            let name = command.args.trim();
            if editor.viml.remove_function(name).is_none() {
                return Err(VimError::new(117, format!("Unknown function: {name}")));
            }
        }
        "wviminfo" | "wv" => {
            let arg = command.args.trim();
            let path = if arg.is_empty() {
                crate::viminfo::default_path(editor)
                    .ok_or_else(|| VimError::new(484, "Can't find viminfo file"))?
            } else {
                expand_path_arg(arg)
            };
            crate::viminfo::write(editor, &path)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            editor.messages.info(format!("written {}", path.display()));
        }
        "rviminfo" | "rv" => {
            let arg = command.args.trim();
            let path = if arg.is_empty() {
                crate::viminfo::default_path(editor)
                    .ok_or_else(|| VimError::new(484, "Can't find viminfo file"))?
            } else {
                expand_path_arg(arg)
            };
            crate::viminfo::read(editor, &path)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            editor.messages.info(format!("read {}", path.display()));
        }
        "oldfiles" | "ol" => {
            if editor.oldfiles.is_empty() {
                editor.messages.info("No old files".to_string());
            } else {
                let text = editor
                    .oldfiles
                    .iter()
                    .enumerate()
                    .map(|(index, file)| format!("{:>3}: {file}", index + 1))
                    .collect::<Vec<_>>()
                    .join(
                        "
",
                    );
                editor.messages.info(text);
            }
        }
        "scriptnames" | "scriptn" | "scripts" => {
            let mut text = String::new();
            for (id, name) in &editor.sourced_scripts {
                text.push_str(&format!("{id}: {name}\n"));
            }
            if text.is_empty() {
                for id in editor.viml.script_ids() {
                    let name = editor.viml.script_name(id).unwrap_or_default();
                    if !name.is_empty() {
                        text.push_str(&format!("{}: {}\n", id.index(), name));
                    }
                }
            }
            editor.messages.info(text.trim_end().to_string());
        }
        "help" | "h" => {
            if editor.tag_index.is_empty() {
                if let Some(path) = editor.runtime.find("doc/tags") {
                    editor.tag_index = crate::tags::TagIndex::load(&path);
                }
            }
            let topic = command.args.trim();
            let already_help = editor
                .current_buffer()
                .is_some_and(|buffer| buffer.buftype == "help");
            if !already_help {
                editor.split_window(SplitDir::Horizontal, None)?;
            }
            let candidates = crate::tags::help_tag_for(topic);
            let mut found = None;
            for candidate in candidates {
                if editor.tag_index.find(&candidate).is_some() {
                    found = Some(candidate);
                    break;
                }
            }
            match found {
                Some(tag) => editor.jump_to_tag(&tag)?,
                None => {
                    if editor.tag_index.is_empty() {
                        editor
                            .messages
                            .info("E149: Sorry, no help for this topic (no runtime tags file)");
                    } else {
                        return Err(VimError::new(149, format!("Sorry, no help for {topic}")));
                    }
                }
            }
        }
        "tag" | "ta" => {
            let name = command.args.trim();
            if name.is_empty() {
                return Err(VimError::new(471, "Argument required"));
            }
            editor.jump_to_tag(name)?;
        }
        "tags" => editor.messages.info(editor.tag_stack_summary()),
        "tselect" | "ts" => {
            let name = command.args.trim();
            if name.is_empty() {
                return Err(VimError::new(471, "Argument required"));
            }
            editor.tag_select(name)?;
        }
        "tjump" | "tju" => {
            let name = command.args.trim();
            if name.is_empty() {
                return Err(VimError::new(471, "Argument required"));
            }
            editor.tag_jump(name)?;
        }
        "tnext" | "tn" => editor.tag_step(1)?,
        "tprevious" | "tp" | "tprev" => editor.tag_step(-1)?,
        "tfirst" | "tf" => editor.tag_first()?,
        "tlast" | "tl" => editor.tag_last()?,
        "trewind" | "tr" => editor.tag_first()?,
        "ptag" | "pta" => {
            let mut name = command.args.trim().to_string();
            if name.is_empty() {
                name = editor.keyword_under_cursor().unwrap_or_default();
            }
            if name.is_empty() {
                return Err(VimError::new(471, "Argument required"));
            }
            editor.open_preview_tag(&name)?;
        }
        "ptselect" | "pts" => {
            let mut name = command.args.trim().to_string();
            if name.is_empty() {
                name = editor.keyword_under_cursor().unwrap_or_default();
            }
            if name.is_empty() {
                return Err(VimError::new(471, "Argument required"));
            }
            // Show the candidate list like :tselect, then preview the first match.
            editor.tag_select(&name)?;
            editor.open_preview_tag(&name)?;
        }
        "pedit" | "ped" => {
            let path = resolve_file_arg(editor, command.args.trim())?;
            editor.open_preview_file(&path)?;
        }
        "pclose" | "pc" => editor.close_preview()?,
        "helpclose" | "helpc" => {
            let help = editor
                .current_buffer()
                .is_some_and(|buffer| buffer.buftype == "help");
            if !help {
                return Err(VimError::new(444, "Cannot close last help window"));
            }
            editor.close_window_or_error()?;
        }
        "helptags" => {
            let arg = command.args.trim();
            let directory = if arg.is_empty() {
                editor
                    .runtime
                    .find("doc")
                    .unwrap_or_else(|| std::path::PathBuf::from("doc"))
            } else {
                expand_path_arg(arg)
            };
            match crate::tags::generate_helptags(&directory) {
                Ok(count) => editor
                    .messages
                    .info(format!("{count} tags generated in {}", directory.display())),
                Err(error) => return Err(VimError::new(13, error.to_string())),
            }
        }
        "preserve" => editor.preserve_swaps(),
        "recover" | "rec" => {
            let arg = command.args.trim();
            let path = if arg.is_empty() {
                None
            } else {
                Some(expand_path_arg(arg))
            };
            editor.recover_current(path.as_deref())?;
        }
        "swapname" | "swa" => {
            let name = editor.swap_name();
            if name.is_empty() {
                editor.messages.info("no swap file");
            } else {
                editor.messages.info(name);
            }
        }
        "delmarks" | "delm" => {
            let arg = command.args.trim();
            if arg == "!" {
                if let Some(buffer) = editor.current_buffer_mut() {
                    buffer.marks.clear();
                }
            } else {
                let marks: Vec<char> = arg.chars().filter(|ch| !ch.is_whitespace()).collect();
                if let Some(buffer) = editor.current_buffer_mut() {
                    for mark in marks {
                        buffer.marks.remove(&mark);
                    }
                }
            }
        }
        "marks" => {
            editor.messages.info(editor.marks_summary());
        }
        "changes" => editor.messages.info(editor.changes_summary()),
        "jumps" => {
            editor.messages.info(editor.jumps_summary());
        }
        "colorscheme" | "colors" | "colo" => {
            let name = command.args.trim();
            if name.is_empty() {
                editor.messages.info("default");
            } else {
                let relative = format!("colors/{name}.vim");
                match editor.runtime.find(&relative) {
                    Some(path) => {
                        let text = rvim_platform::fs::read_file(&path)
                            .map_err(|error| VimError::new(13, error.to_string()))?;
                        editor.source_viml(&text, &path.display().to_string())?;
                        editor.fire_event(crate::autocmd::Event::ColorScheme);
                        editor.messages.info(format!("colorscheme {name}"));
                    }
                    None => {
                        return Err(VimError::new(
                            185,
                            format!("Cannot find color scheme '{name}'"),
                        ))
                    }
                }
            }
        }
        "highlight" | "hi" | "high" | "highl" | "highli" | "highlig" => {
            handle_highlight(editor, command.args.trim());
        }
        "setf" | "setfiletype" => {
            let name = command.args.trim();
            if name.is_empty() {
                return Err(VimError::new(471, "Argument required"));
            }
            if command.name == "setfiletype" {
                let current = editor
                    .options
                    .buffer_string(editor.current_buffer_id(), "filetype");
                if !current.is_empty() {
                    return Ok(());
                }
            }
            let buffer_id = editor.current_buffer_id();
            editor
                .options
                .set_buffer(
                    buffer_id,
                    "filetype",
                    crate::options::OptionValue::String(VimString::from(name)),
                )
                .map_err(VimError::plain)?;
            editor.refresh_filetype();
        }
        "filetype" | "filet" | "filety" => {
            let arg = command.args.trim();
            let mut parts = arg.split_whitespace();
            match parts.next() {
                None => editor.messages.info(format!(
                    "filetype detection:on  plugin:{}  indent:{}",
                    if editor.filetype_plugin { "on" } else { "off" },
                    if editor.filetype_indent { "on" } else { "off" }
                )),
                Some("on") => {}
                Some("off") => {
                    editor.filetype_plugin = false;
                    editor.filetype_indent = false;
                }
                Some("plugin") => editor.filetype_plugin = parts.next() != Some("off"),
                Some("indent") => editor.filetype_indent = parts.next() != Some("off"),
                Some("detect") => {
                    editor.load_filetype_plugins();
                }
                Some(other) => {
                    return Err(VimError::plain(format!("E475: Invalid argument: {other}")))
                }
            }
            editor.load_filetype_plugins();
        }
        "syntax" | "syn" => {
            let arg = command.args.trim();
            match arg {
                "" | "on" | "enable" => {
                    editor.syntax.set_enabled(true);
                    editor.load_runtime_syntax();
                    editor.messages.info("syntax on");
                }
                "off" | "disable" => {
                    editor.syntax.set_enabled(false);
                    editor.messages.info("syntax off");
                }
                "clear" => editor.syntax.clear(),
                _ => {
                    if let Err(error) = editor.syntax.define(arg) {
                        return Err(VimError::plain(error));
                    }
                    editor.syntax.set_enabled(true);
                }
            }
            editor.request_redraw();
        }
        "windo" | "wind" => editor
            .execute_list_command(crate::editor::ListDoCommand::Windows, command.args.trim())?,
        "tabdo" | "tabd" => {
            editor.execute_list_command(crate::editor::ListDoCommand::Tabs, command.args.trim())?
        }
        "bufdo" | "bufd" => editor
            .execute_list_command(crate::editor::ListDoCommand::Buffers, command.args.trim())?,
        "cdo" => editor
            .execute_list_command(crate::editor::ListDoCommand::Quickfix, command.args.trim())?,
        "cfdo" => editor.execute_list_command(
            crate::editor::ListDoCommand::QuickfixFiles,
            command.args.trim(),
        )?,
        "ldo" => editor
            .execute_list_command(crate::editor::ListDoCommand::Loclist, command.args.trim())?,
        "lfdo" => editor.execute_list_command(
            crate::editor::ListDoCommand::LoclistFiles,
            command.args.trim(),
        )?,
        "args" | "ar" => {
            editor.messages.info(editor.args_summary());
        }
        "argadd" | "arga" => {
            for arg in command.args.split_whitespace() {
                editor.arg_add(expand_path_arg(arg));
            }
            editor.messages.info(editor.args_summary());
        }
        "next" | "n" => editor.arg_next(true, command.args.trim().parse().unwrap_or(1))?,
        "previous" | "prev" | "pre" => {
            editor.arg_next(false, command.args.trim().parse().unwrap_or(1))?
        }
        "rewind" | "rew" => {
            editor.arg_index = 0;
            if let Some(path) = editor.arglist.first().cloned() {
                editor.open_file(&path)?;
            }
        }
        "argdo" | "argd" => editor
            .execute_list_command(crate::editor::ListDoCommand::Arguments, command.args.trim())?,
        "redir" | "redi" => {
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
        "history" | "his" => {
            let arg = command.args.trim();
            let kinds: Vec<(&str, &str)> = match arg {
                "" | ":" | "cmd" => vec![("cmd", ":")],
                "/" | "search" => vec![("search", "/")],
                "all" => vec![("cmd", ":"), ("search", "/")],
                _ => return Err(VimError::new(474, "E474: Invalid argument")),
            };
            let mut out = String::new();
            for (kind, _) in kinds {
                if let Some(history) = editor.history(kind) {
                    for (index, item) in history.iter().enumerate() {
                        out.push_str(&format!("{:>3}  {}\n", index + 1, item));
                    }
                }
            }
            editor.messages.info(out.trim_end().to_string());
        }
        "messages" | "mes" | "messa" => {
            let arg = command.args.trim();
            if arg == "clear" {
                editor.messages.history.clear();
                editor.messages.last = None;
                editor.messages.statusmsg.clear();
                editor.messages.info("Messages cleared");
            } else {
                let text = editor
                    .messages
                    .history
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n");
                editor.messages.info(text);
            }
        }
        "!" => {
            let command_text = expand_shell_command(editor, command.args.trim());
            if command_text.is_empty() {
                return Err(VimError::plain("E471: Argument required"));
            }
            editor.last_bang = command_text.clone();
            if !command.range.addresses.is_empty() {
                let range = resolve_range(editor, &command.range)?;
                let mut input = String::new();
                for lnum in range.start..=range.end {
                    if let Some(line) = editor.line(lnum) {
                        input.push_str(&String::from_utf8_lossy(line));
                    }
                    input.push('\n');
                }
                let output = editor
                    .run_shell(&command_text, Some(input.as_bytes()))
                    .map_err(|error| VimError::new(13, error.to_string()))?;
                let mut replacement = output.stdout;
                if replacement.is_empty() || !replacement.ends_with(b"\n") {
                    replacement.push(b'\n');
                }
                let start = range.start;
                let end = range.end;
                let line_count = editor.line_count();
                let delete_end = if end < line_count {
                    BufPos::new(end + 1, 0)
                } else {
                    let len = editor
                        .current_buffer()
                        .map_or(0, |buffer| buffer.text.line_len(end));
                    BufPos::new(end, len)
                };
                if let Some(buffer) = editor.current_buffer_mut() {
                    buffer.text.delete_range(BufPos::new(start, 0), delete_end);
                    buffer.text.insert_bytes(start, 0, &replacement);
                }
                editor.set_cursor(BufPos::new(start, 0));
                if !output.stderr.is_empty() {
                    editor.messages.info(
                        String::from_utf8_lossy(&output.stderr)
                            .trim_end()
                            .to_string(),
                    );
                }
            } else {
                let output = editor
                    .run_shell(&command_text, None)
                    .map_err(|error| VimError::new(13, error.to_string()))?;
                let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
                if !output.stderr.is_empty() {
                    text.push_str(&String::from_utf8_lossy(&output.stderr));
                }
                editor.messages.info(text.trim_end().to_string());
            }
        }
        "mksession" | "mks" => {
            let arg = command.args.trim();
            let path = if arg.is_empty() {
                std::path::PathBuf::from("Session.vim")
            } else {
                resolve_file_arg(editor, arg)?
            };
            let script = session_script(editor);
            rvim_platform::fs::write_file_atomic(&path, script.as_bytes())
                .map_err(|error| VimError::new(13, error.to_string()))?;
            editor
                .messages
                .info(format!("session written to {}", path.display()));
        }
        "mkview" => {
            let arg = command.args.trim();
            let path = view_path(editor, arg)?;
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let pos = editor.cursor();
            let script = format!("\" RVim view\ncall cursor({}, {})\n", pos.lnum, pos.col + 1);
            rvim_platform::fs::write_file_atomic(&path, script.as_bytes())
                .map_err(|error| VimError::new(13, error.to_string()))?;
        }
        "loadview" => {
            let arg = command.args.trim();
            let path = view_path(editor, arg)?;
            let text = rvim_platform::fs::read_file(&path)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            editor.source_viml(&text, &path.display().to_string())?;
        }
        "source" | "so" | "sourc" => {
            let arg = command.args.trim();
            if arg.is_empty() {
                return Err(VimError::new(471, "Argument required"));
            }
            let arg = expand_sfile(editor, arg);
            let path = expand_path_arg(&arg);
            let text = rvim_platform::fs::read_file(&path)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            editor.fire_event(crate::autocmd::Event::SourcePre);
            let result = editor.source_viml(&text, &path.display().to_string());
            editor.fire_event(crate::autocmd::Event::SourcePost);
            result?;
            editor.messages.info(format!("sourced {}", path.display()));
        }
        "let" | "let!" => {
            let source = format!(
                "let {}
",
                command.args.trim()
            );
            editor.execute_viml(&source, "[command]")?;
        }
        "unlet" | "unl" => {
            let source = format!(
                "unlet {}
",
                command.args.trim()
            );
            editor.execute_viml(&source, "[command]")?;
        }
        "call" | "cal" => {
            let source = format!(
                "call {}
",
                command.args.trim()
            );
            editor.execute_viml(&source, "[command]")?;
        }
        "throw" | "th" => {
            let source = format!(
                "throw {}
",
                command.args.trim()
            );
            editor.execute_viml(&source, "[command]")?;
        }
        "verbose" => {
            let rest = command.args.trim();
            if rest.is_empty() {
                return Err(VimError::new(471, "Argument required"));
            }
            editor.execute_ex(rest);
        }
        "execute" | "exe" | "exec" => {
            let source = format!("execute {}\n", command.args.trim());
            editor.execute_viml(&source, "[command]")?;
        }
        "packadd" | "pa" => {
            let name = command.args.trim();
            if name.is_empty() {
                return Err(VimError::new(471, "Argument required"));
            }
            editor.packadd(name, !command.bang)?;
        }
        "packloadall" | "packl" => {
            editor.packloadall()?;
        }
        "scriptencoding" | "scripte" => {
            let name = command.args.trim();
            if name.is_empty() {
                return Err(VimError::new(471, "Argument required"));
            }
        }
        "runtime" | "ru" => {
            let arg = command.args.trim();
            let all = command.bang || arg.ends_with('!');
            let names = arg.trim_end_matches('!');
            let matches = if all {
                editor.runtime.find_all(names)
            } else {
                editor.runtime.find(names).into_iter().collect()
            };
            if matches.is_empty() {
                return Err(VimError::new(
                    484,
                    format!("Can't find {names} in runtimepath"),
                ));
            }
            for path in matches {
                editor
                    .trace
                    .log("runtime", &format!("source {}", path.display()));
                let text = rvim_platform::fs::read_file(&path)
                    .map_err(|error| VimError::new(13, error.to_string()))?;
                editor.source_viml(&text, &path.display().to_string())?;
            }
            let _ = names;
        }
        "command" | "com" | "comm" | "comma" | "comman" => {
            define_user_command(editor, &command)?;
        }
        "delcommand" | "delc" => {
            let name = command.args.trim();
            editor.user_commands.remove(name);
        }
        "comclear" | "comc" => editor.user_commands.clear(),
        "autocmd" | "au" => {
            handle_autocmd(editor, &command)?;
        }
        "augroup" | "aug" => {
            let arg = command.args.trim();
            if arg.is_empty() {
                editor.messages.info(
                    editor
                        .autocmds
                        .groups
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            } else if arg.eq_ignore_ascii_case("END") {
                editor.autocmds.current_group = None;
            } else if arg.ends_with('!') {
                editor.autocmds.delete_group(arg.trim_end_matches('!'));
            } else {
                editor.autocmds.begin_group(arg);
            }
        }
        "doautocmd" | "doau" | "do" | "doautoall" | "doauto" => {
            let mut parts = command.args.split_whitespace();
            let first = parts.next().unwrap_or("");
            let event_name = if crate::autocmd::Event::parse(first).is_some() {
                first
            } else {
                parts.next().unwrap_or("")
            };
            let mut event = crate::autocmd::Event::parse(event_name)
                .ok_or_else(|| VimError::plain(format!("E216: No such event: {event_name}")))?;
            if matches!(event, crate::autocmd::Event::User(_)) {
                if let Some(name) = parts.next() {
                    event = crate::autocmd::Event::User(name.to_string());
                }
            }
            editor.fire_event(event);
        }
        "saveas" | "sav" | "savea" => {
            let arg = command.args.trim();
            if arg.is_empty() {
                return Err(VimError::new(471, "Argument required"));
            }
            let path = expand_path_arg(arg);
            editor.force_write = command.bang;
            editor.write_buffer(Some(&path.display().to_string()), false)?;
        }
        "file" | "f" => {
            let arg = command.args.trim();
            if !arg.is_empty() {
                if let Some(buffer) = editor.current_buffer_mut() {
                    buffer.set_name(expand_path_arg(arg));
                }
            } else if let Some(name) = editor
                .current_buffer()
                .and_then(|buffer| buffer.text.meta.name.clone())
            {
                editor.messages.info(name.display().to_string());
            }
        }
        "ls" | "buffers" | "files" => {
            editor.messages.info(editor.buffer_summary(false));
        }
        "ls!" | "buffers!" => {
            editor.messages.info(editor.buffer_summary(true));
        }
        "bnext" | "bn" | "bNext" => editor.next_buffer(true)?,
        "bprevious" | "bp" | "bprev" | "bPrevious" => editor.next_buffer(false)?,
        "bfirst" | "brewind" | "br" => editor.buffer_first()?,
        "blast" | "bl" => editor.buffer_last()?,
        "bmodified" | "bm" => editor.buffer_modified()?,
        "buffer" | "bu" | "buf" => {
            let arg = command.args.trim();
            if arg.is_empty() {
                editor.messages.info(editor.buffer_summary(false));
            } else if let Ok(number) = arg.parse::<u32>() {
                let id = editor
                    .buffer_by_number(number)
                    .ok_or_else(|| VimError::new(94, "No matching buffer"))?;
                editor.switch_to_buffer(id)?;
            } else {
                let path = expand_path_arg(arg);
                let found = editor.buffer_ids().into_iter().find(|id| {
                    editor
                        .buffers
                        .get(*id)
                        .and_then(|buffer| buffer.text.meta.name.as_ref())
                        == Some(&path)
                });
                match found {
                    Some(id) => editor.switch_to_buffer(id)?,
                    None => editor.open_file(&path)?,
                }
            }
        }
        "badd" | "ba" | "bufadd" => {
            let path = resolve_file_arg(editor, command.args.trim())?;
            editor.add_unlisted_buffer(Some(&path))?;
        }
        "bufload" => {
            let id = buffer_argument(editor, command.args.as_str())?;
            editor.load_buffer(id)?;
        }
        "bdelete" | "bd" => {
            let id = buffer_argument(editor, command.args.as_str())?;
            editor.unload_buffer(id, command.bang, true)?;
        }
        "bwipeout" | "bw" => {
            let id = buffer_argument(editor, command.args.as_str())?;
            editor.wipe_buffer(id, command.bang)?;
        }
        "bunload" | "bun" => {
            let id = buffer_argument(editor, command.args.as_str())?;
            editor.unload_buffer(id, command.bang, false)?;
        }
        "sbuffer" | "sb" => {
            editor.split_window(rvim_view::SplitDir::Horizontal, None)?;
            if let Ok(number) = command.args.trim().parse::<u32>() {
                if let Some(id) = editor.buffer_by_number(number) {
                    editor.switch_to_buffer(id)?;
                }
            }
        }
        "sbnext" | "sbn" => {
            editor.split_window(rvim_view::SplitDir::Horizontal, None)?;
            editor.next_buffer(true)?;
        }
        "sbprevious" | "sbp" | "sbprev" => {
            editor.split_window(rvim_view::SplitDir::Horizontal, None)?;
            editor.next_buffer(false)?;
        }
        "sbrewind" | "sbr" => {
            editor.split_window(rvim_view::SplitDir::Horizontal, None)?;
            editor.buffer_first()?;
        }
        "sblast" | "sbl" => {
            editor.split_window(rvim_view::SplitDir::Horizontal, None)?;
            editor.buffer_last()?;
        }
        "sball" | "sba" => {
            let current = editor.current_buffer;
            let ids = editor.buffer_ids();
            for id in ids {
                if id == current {
                    continue;
                }
                editor.split_window(rvim_view::SplitDir::Horizontal, None)?;
                editor.switch_to_buffer(id)?;
            }
        }
        "sp" | "spl" | "spli" | "split" => {
            let arg = command.args.trim();
            let path = if arg.is_empty() {
                None
            } else {
                Some(resolve_file_arg(editor, arg)?)
            };
            let direction = if editor.vertical_requested {
                rvim_view::SplitDir::Vertical
            } else {
                rvim_view::SplitDir::Horizontal
            };
            editor.split_window(direction, path.as_deref())?;
        }
        "vs" | "vsp" | "vspl" | "vspli" | "vsplit" => {
            let arg = command.args.trim();
            let path = if arg.is_empty() {
                None
            } else {
                Some(resolve_file_arg(editor, arg)?)
            };
            editor.split_window(rvim_view::SplitDir::Vertical, path.as_deref())?;
        }
        "foldopen" | "foldo" | "foldclose" | "foldc" | "folddoopen" | "folddoclosed" => {
            // Folding is P2; accept the commands so syntax files load.
        }
        "enew" => {
            check_modified(editor, command.bang)?;
            editor.new_empty_buffer()?;
        }
        "new" => {
            let direction = if editor.vertical_requested {
                rvim_view::SplitDir::Vertical
            } else {
                rvim_view::SplitDir::Horizontal
            };
            editor.split_window(direction, None)?;
            editor.new_empty_buffer()?;
        }
        "vnew" => {
            editor.split_window(rvim_view::SplitDir::Vertical, None)?;
            editor.new_empty_buffer()?;
        }
        "tabnew" | "tabe" | "tabedit" => {
            let arg = command.args.trim();
            let path = if arg.is_empty() {
                None
            } else {
                Some(resolve_file_arg(editor, arg)?)
            };
            editor.tab_new(path.as_deref())?;
        }
        "tabnext" | "tabn" => {
            let arg = command.args.trim();
            if let Ok(number) = arg.parse::<usize>() {
                if let Some(tab) = editor.view.tab_ids().get(number.saturating_sub(1)).copied() {
                    editor.view.set_tab(tab);
                    editor.sync_current_buffer_to_window();
                    editor.sync_window_options();
                    editor.request_redraw();
                }
            } else {
                editor.tab_next();
            }
        }
        "tabprevious" | "tabp" => editor.tab_previous(),
        "tabfirst" | "tabfir" => {
            if let Some(tab) = editor.view.tab_ids().first().copied() {
                editor.view.set_tab(tab);
                editor.sync_current_buffer_to_window();
                editor.sync_window_options();
                editor.request_redraw();
            }
        }
        "tablast" | "tabl" => {
            if let Some(tab) = editor.view.tab_ids().last().copied() {
                editor.view.set_tab(tab);
                editor.sync_current_buffer_to_window();
                editor.sync_window_options();
                editor.request_redraw();
            }
        }
        "tabonly" | "tabo" => editor.tab_only(command.bang)?,
        "tabclose" | "tabc" => editor.tab_close(command.bang)?,
        "tabmove" | "tabm" => {
            let arg = command.args.trim();
            let target = if arg.is_empty() {
                editor.view.tab_count().saturating_sub(1)
            } else if let Some(relative) = arg.strip_prefix('+') {
                let index = editor
                    .view
                    .tab_ids()
                    .iter()
                    .position(|id| *id == editor.view.current_tab)
                    .unwrap_or(0);
                index + relative.trim().parse::<usize>().unwrap_or(0)
            } else if let Some(relative) = arg.strip_prefix('-') {
                let index = editor
                    .view
                    .tab_ids()
                    .iter()
                    .position(|id| *id == editor.view.current_tab)
                    .unwrap_or(0);
                index.saturating_sub(relative.trim().parse::<usize>().unwrap_or(0))
            } else {
                arg.parse::<usize>().unwrap_or(0)
            };
            editor.view.move_current_tab(target);
            editor.request_redraw();
        }
        "close" | "clo" | "clos" => {
            if editor.cmdwin.is_some() {
                editor.close_cmdwin();
            } else {
                editor.close_window_or_error()?;
            }
        }
        "only" | "on" | "only!" => editor.only_window(),
        "wincmd" | "winc" => {
            let arg = command.args.trim();
            match arg.chars().next() {
                Some('h') | Some('k') | Some('p') => editor.cycle_window(false),
                Some('l') | Some('j') | Some('w') => editor.cycle_window(true),
                Some('o') => editor.only_window(),
                Some('c') | Some('q') => editor.close_window_or_error()?,
                Some('s') => editor.split_window(rvim_view::SplitDir::Horizontal, None)?,
                Some('v') => editor.split_window(rvim_view::SplitDir::Vertical, None)?,
                _ => return Err(VimError::plain("E474: Invalid argument")),
            }
        }
        "abbreviate" | "ab" | "iabbrev" | "ia" | "inoreabbrev" | "inorea" | "noreabbrev"
        | "norea" => {
            let args = command.args.trim();
            if args.is_empty() {
                let mut text = String::new();
                for (lhs, rhs) in &editor.abbreviations {
                    text.push_str(&format!("{lhs}  {rhs}\n"));
                }
                editor.messages.info(text.trim_end().to_string());
            } else {
                let (lhs, rhs) = args.split_once(char::is_whitespace).unwrap_or((args, ""));
                if !lhs.is_empty() {
                    editor
                        .abbreviations
                        .insert(lhs.to_string(), rhs.trim_start().to_string());
                }
            }
        }
        "unabbreviate" | "una" | "iunabbrev" | "iuna" => {
            let lhs = command.args.trim();
            editor.abbreviations.remove(lhs);
        }
        "abclear" | "abc" => editor.abbreviations.clear(),
        "map" | "nmap" | "nnoremap" | "noremap" | "imap" | "inoremap" | "vmap" | "vnoremap"
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
                    "expr" => options.expr = Some(String::new()),
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
                        text.push('\n');
                    }
                }
                editor.messages.info(text.trim_end().to_string());
            } else {
                let (lhs_text, rhs_text) =
                    args.split_once(char::is_whitespace).unwrap_or((args, ""));
                let lhs = crate::keymap::parse_keys(lhs_text);
                let rhs_text = rhs_text.trim_start();
                let rhs = if options.expr.is_some() {
                    options.expr = Some(rhs_text.to_string());
                    Vec::new()
                } else {
                    crate::keymap::parse_keys(rhs_text)
                };
                let noremap = name.contains("nore");
                for mode in modes {
                    if !editor.mappings.add(
                        mode,
                        lhs.clone(),
                        rhs.clone(),
                        noremap,
                        options.clone(),
                    ) {
                        return Err(VimError::new(227, "E227: Mapping already exists"));
                    }
                }
            }
        }
        "unmap" | "nunmap" | "iunmap" | "vunmap" | "cunmap" | "sunmap" | "xunmap" | "ounmap" => {
            let name = command.name.to_ascii_lowercase();
            let modes = match name.as_str() {
                "iunmap" => vec![MapMode::Insert],
                "cunmap" => vec![MapMode::Command],
                "vunmap" | "sunmap" | "xunmap" => vec![MapMode::Visual],
                "ounmap" => vec![MapMode::Operator],
                "nunmap" => vec![MapMode::Normal],
                _ => vec![MapMode::Normal, MapMode::Visual],
            };
            let lhs = crate::keymap::parse_keys(command.args.trim());
            for mode in modes {
                editor.mappings.remove(mode, &lhs);
            }
        }
        "mapclear" => {
            editor.mappings.clear(MapMode::Normal);
            editor.mappings.clear(MapMode::Insert);
            editor.mappings.clear(MapMode::Visual);
            editor.mappings.clear(MapMode::Command);
        }
        "d" | "de" | "del" | "dele" | "delet" | "delete" => {
            let range = resolve_range(editor, &command.range)?;
            let register = command
                .args
                .trim()
                .chars()
                .next()
                .and_then(RegisterName::parse)
                .unwrap_or(RegisterName::Unnamed);
            editor.pending_register = command.args.trim().chars().next();
            editor.delete_lines(range, register)?;
        }
        "y" | "ya" | "yan" | "yank" => {
            let range = resolve_range(editor, &command.range)?;
            editor.pending_register = command.args.trim().chars().next();
            editor.yank_lines(range)?;
        }
        "pu" | "put" => {
            let register = command
                .args
                .trim()
                .chars()
                .next()
                .and_then(RegisterName::parse)
                .unwrap_or(RegisterName::Unnamed);
            editor.put_register(register, command.bang)?;
        }
        "j" | "jo" | "joi" | "join" => {
            let range = resolve_range(editor, &command.range)?;
            editor.join_range(range, !command.bang);
        }
        "p" | "pr" | "pri" | "prin" | "print" => {
            let range = resolve_range(editor, &command.range)?;
            let mut text = String::new();
            for lnum in range.start..=range.end {
                if let Some(line) = editor.line(lnum) {
                    text.push_str(&String::from_utf8_lossy(line));
                    text.push('\n');
                }
            }
            editor.messages.info(text.trim_end().to_string());
        }
        "r" | "re" | "rea" | "read" => {
            let arg = command.args.trim();
            if arg.is_empty() {
                return Err(VimError::new(32, "No file name"));
            }
            let after = if command.range.addresses.is_empty() {
                editor.cursor().lnum
            } else {
                resolve_range(editor, &command.range)?.end
            };
            if let Some(shell) = arg.strip_prefix('!') {
                let shell = expand_shell_command(editor, shell);
                let output = editor
                    .run_shell(&shell, None)
                    .map_err(|error| VimError::new(13, error.to_string()))?;
                editor.read_after_line(
                    after,
                    &output.stdout,
                    std::path::Path::new("[shell output]"),
                )?;
            } else {
                let path = expand_path_arg(arg);
                let bytes = rvim_platform::fs::read_file(&path)
                    .map_err(|error| VimError::new(13, error.to_string()))?;
                editor.read_after_line(after, &bytes, &path)?;
            }
        }
        "noh" | "nohl" | "nohlsearch" => {
            editor.clear_search_highlight();
            editor.messages.info("Search highlight cleared");
        }
        "cd" | "chd" | "chdir" | "lcd" | "lc" | "tcd" => {
            let arg = command.args.trim();
            let current = rvim_platform::fs::cwd();
            let path = if arg == "-" {
                editor.previous_cwd.clone().unwrap_or(current.clone())
            } else if arg.is_empty() {
                rvim_platform::env::home()
            } else {
                expand_path_arg(arg)
            };
            rvim_platform::fs::set_cwd(&path)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            editor.previous_cwd = Some(current);
            editor
                .messages
                .info(format!("cwd: {}", rvim_platform::fs::cwd().display()));
        }
        "pwd" => {
            editor
                .messages
                .info(rvim_platform::fs::cwd().display().to_string());
        }
        "startinsert" | "star" | "starti" => {
            let arg = command.args.trim();
            match arg {
                "!" => {
                    let lnum = editor.cursor().lnum;
                    editor.set_cursor(BufPos::new(lnum, 0));
                }
                "end" => {
                    let lnum = editor.cursor().lnum;
                    let len = editor
                        .current_buffer()
                        .map_or(0, |buffer| buffer.text.line_len(lnum));
                    editor.set_cursor(BufPos::new(lnum, len));
                }
                _ => {}
            }
            editor.start_insert();
        }
        "stopinsert" | "stopi" => editor.stop_insert(),
        "resize" | "res" | "resiz" => {
            let vertical = editor.vertical_requested
                || command
                    .modifiers
                    .iter()
                    .any(|modifier| matches!(modifier, rvim_ex::Modifier::Vertical));
            let arg = command.args.trim();
            let delta = if let Some(value) = arg.strip_prefix('+') {
                value.parse::<i32>().unwrap_or(1)
            } else if arg.starts_with('-') {
                arg.parse::<i32>().unwrap_or(-1)
            } else {
                arg.parse::<i32>().unwrap_or(1)
            };
            editor.resize_current_window_public(vertical, delta);
        }
        "stop" | "suspend" | "sus" | "st" => {
            editor.suspend_requested = true;
        }
        "redraw" | "red" | "redr" | "redra" | "redrawstatus" | "redrawtabline" => {
            editor.request_redraw();
        }
        "version" | "ve" | "ver" => {
            let features = [
                "eval",
                "syntax",
                "quickfix",
                "linebreak",
                "statusline_click",
                "compl_func",
                "digraphs",
                "clipboard",
                "mouse",
                "timers",
                "job",
                "pty",
            ]
            .into_iter()
            .filter(|feature| rvim_types::has_feature(feature))
            .collect::<Vec<_>>()
            .join(" ");
            editor.messages.info(format!(
                "RVim {} (Vim {})\nFeatures: {}",
                rvim_types::RVIM_VERSION,
                rvim_types::VIM_VERSION,
                features
            ));
        }
        "normal" | "norm" | "norma" => {
            let trimmed = command.args.trim_start();
            let no_map = command.bang || trimmed.starts_with('!');
            let keys = trimmed.trim_start_matches('!').trim_start().to_string();
            if command.range.addresses.is_empty() {
                editor.run_normal_keys(&keys, !no_map);
            } else {
                let range = resolve_range(editor, &command.range)?;
                for lnum in range.start..=range.end {
                    editor.goto_line(lnum);
                    editor.run_normal_keys(&keys, !no_map);
                    editor.stop_insert();
                }
            }
        }
        "echo" | "ec" | "ech" | "echon" | "echomsg" | "echom" | "echoerr" | "echoe"
        | "echowindow" | "echow" => {
            let name = command.name.to_ascii_lowercase();
            editor.message_kind = match name.as_str() {
                "echon" => crate::editor::MessageKind::Echo,
                "echomsg" | "echom" => crate::editor::MessageKind::EchoMsg,
                "echoerr" | "echoe" => crate::editor::MessageKind::EchoErr,
                "echowindow" | "echow" => crate::editor::MessageKind::EchoWindow,
                _ => crate::editor::MessageKind::Echo,
            };
            editor.message_append = name == "echon";
            let source = format!("echo {}\n", command.args.trim());
            let result = editor.execute_viml(&source, "[command]");
            editor.message_kind = crate::editor::MessageKind::Info;
            editor.message_append = false;
            result?;
        }
        "registers" | "reg" | "regi" | "regis" | "regist" | "registe" | "register" => {
            editor.messages.info(editor.registers_summary());
        }
        "sort" | "sor" => {
            let range = if command.range.addresses.is_empty() {
                ResolvedRange::new(1, editor.line_count().max(1))
            } else {
                resolve_range(editor, &command.range)?
            };
            let mut lines: Vec<String> = (range.start..=range.end)
                .filter_map(|lnum| {
                    editor
                        .line(lnum)
                        .map(|line| String::from_utf8_lossy(line).into_owned())
                })
                .collect();
            let flags = command.args.trim();
            let numeric = flags.contains('n');
            let ignore_case = flags.contains('i');
            let reverse = flags.contains('r');
            let unique = flags.contains('u');
            lines.sort_by(|a, b| {
                let ordering = if numeric {
                    let left = a.trim().parse::<i64>().unwrap_or(0);
                    let right = b.trim().parse::<i64>().unwrap_or(0);
                    left.cmp(&right)
                } else if ignore_case {
                    a.to_lowercase().cmp(&b.to_lowercase())
                } else {
                    a.cmp(b)
                };
                if reverse {
                    ordering.reverse()
                } else {
                    ordering
                }
            });
            if unique {
                lines.dedup();
            }
            editor.replace_line_range(range, lines);
        }
        "copy" | "co" | "t" => {
            let source = resolve_range(editor, &command.range)?;
            let target = parse_address_target(editor, command.args.trim())?;
            editor.copy_lines(source, target)?;
        }
        "move" | "m" => {
            let source = resolve_range(editor, &command.range)?;
            let target = parse_address_target(editor, command.args.trim())?;
            editor.move_lines(source, target)?;
        }
        "global" | "g" | "vglobal" | "v" => {
            handle_global(editor, &command)?;
        }
        "s" | "su" | "sub" | "subs" | "subst" | "substi" | "substit" | "substu" | "substitut"
        | "substitute" => {
            handle_substitute(editor, &command)?;
        }
        "&" | "&&" => {
            let mut repeat = command.clone();
            repeat.name = "substitute".to_string();
            repeat.args = command.args.trim_start_matches('&').trim().to_string();
            handle_substitute(editor, &repeat)?;
        }
        _ => {
            if let Some(full) = crate::registry::resolve_command_name(&command.name) {
                if full != command.name {
                    let mut renamed = command;
                    renamed.name = full.to_string();
                    return execute_command_inner(editor, renamed);
                }
            }
            return Err(VimError::plain(format!(
                "E492: Not an editor command: {}",
                command.name
            )));
        }
    }
    Ok(())
}

/// Expand `<sfile>` and its `:p`, `:h`, `:t`, `:r`, `:e` modifiers.
///
/// Runtime syntax files use forms such as
/// `source <sfile>:p:h/java.vim` to load a file next to themselves.
fn expand_sfile(editor: &Editor, arg: &str) -> String {
    const MARKER: &str = "<sfile>";
    let Some(script) = editor.script_file.as_deref() else {
        return arg.to_string();
    };
    let mut out = String::new();
    let mut rest = arg;
    while let Some(position) = rest.find(MARKER) {
        out.push_str(&rest[..position]);
        let mut value = script.to_string();
        let mut tail = &rest[position + MARKER.len()..];
        while let Some(after) = tail.strip_prefix(':') {
            let Some(modifier) = after.chars().next() else {
                break;
            };
            let replacement = match modifier {
                'p' => Some(absolute_path(&value)),
                'h' => Some(head_path(&value)),
                't' => Some(tail_path(&value)),
                'r' => Some(root_path(&value)),
                'e' => Some(extension_path(&value)),
                _ => None,
            };
            let Some(replacement) = replacement else {
                break;
            };
            value = replacement;
            tail = &after[modifier.len_utf8()..];
        }
        out.push_str(&value);
        rest = tail;
    }
    out.push_str(rest);
    out
}

fn absolute_path(value: &str) -> String {
    let path = std::path::Path::new(value);
    if path.is_absolute() {
        return value.to_string();
    }
    std::env::current_dir()
        .map(|directory| directory.join(path))
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| value.to_string())
}

fn head_path(value: &str) -> String {
    std::path::Path::new(value)
        .parent()
        .map(|parent| parent.display().to_string())
        .filter(|parent| !parent.is_empty())
        .unwrap_or_else(|| ".".to_string())
}

fn tail_path(value: &str) -> String {
    std::path::Path::new(value)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| value.to_string())
}

fn root_path(value: &str) -> String {
    std::path::Path::new(value)
        .file_stem()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| value.to_string())
}

fn extension_path(value: &str) -> String {
    std::path::Path::new(value)
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn parse_address_target(editor: &Editor, text: &str) -> Result<u32, VimError> {
    if text.is_empty() {
        return Err(VimError::new(471, "Argument required"));
    }
    if let Ok(line) = text.parse::<u32>() {
        return Ok(line);
    }
    let command = rvim_ex::parse(text).map_err(|error| VimError::plain(error.to_string()))?;
    let range = resolve_range(editor, &command.range)?;
    Ok(range.end)
}

fn handle_vimgrep(editor: &mut Editor, command: &ExCommand) -> Result<(), VimError> {
    let args = command.args.trim();
    if args.is_empty() {
        return Err(VimError::new(471, "Argument required"));
    }
    let bytes = args.as_bytes();
    let delimiter = bytes[0] as char;
    if delimiter.is_ascii_alphanumeric() {
        return Err(VimError::new(474, "Invalid vimgrep arguments"));
    }
    let mut index = 1;
    let mut pattern = String::new();
    while index < bytes.len() && bytes[index] as char != delimiter {
        if bytes[index] == b'\\' && index + 1 < bytes.len() {
            pattern.push(bytes[index] as char);
            pattern.push(bytes[index + 1] as char);
            index += 2;
        } else {
            pattern.push(bytes[index] as char);
            index += 1;
        }
    }
    if index >= bytes.len() {
        return Err(VimError::new(474, "Missing pattern delimiter"));
    }
    index += 1;
    let mut files: Vec<String> = args[index..]
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if files.is_empty() {
        if let Some(path) = editor
            .current_buffer()
            .and_then(|buffer| buffer.text.meta.name.as_ref())
        {
            files.push(path.display().to_string());
        } else {
            return Err(VimError::new(471, "Argument required"));
        }
    }
    let mut paths = Vec::new();
    for file in &files {
        if file.contains(['*', '?', '[']) {
            paths.extend(crate::glob::expand(file));
        } else {
            paths.push(std::path::PathBuf::from(file));
        }
    }
    if paths.is_empty() {
        return Err(VimError::new(486, format!("Pattern not found: {pattern}")));
    }
    let regex = crate::search::compile_pattern(editor, &pattern)?;
    let mut entries = Vec::new();
    for path in paths {
        let Ok(bytes) = rvim_platform::fs::read_file(&path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        for (line_index, line) in text.lines().enumerate() {
            for found in rvim_regex::find_all(&regex, line.as_bytes()) {
                entries.push(crate::qf::QfEntry {
                    file: path.clone(),
                    lnum: line_index as u32 + 1,
                    col: found.start as u32 + 1,
                    text: line.trim().to_string(),
                    kind: None,
                    nr: None,
                });
            }
        }
    }
    let count = entries.len();
    editor.set_quickfix(format!(":vimgrep /{pattern}/"), entries);
    if count > 0 {
        editor.quickfix_jump(Some(1))?;
    } else {
        return Err(VimError::new(486, format!("Pattern not found: {pattern}")));
    }
    Ok(())
}

fn handle_global(editor: &mut Editor, command: &ExCommand) -> Result<(), VimError> {
    let inverse = command.name.starts_with('v');
    let Some((pattern, subcommand)) = parse_global_args(&command.args) else {
        return Err(VimError::new(474, "Invalid global arguments"));
    };
    if pattern.is_empty() {
        return Err(VimError::new(35, "No previous regular expression"));
    }
    let regex = crate::search::compile_pattern(editor, &pattern)?;
    let range = if command.range.addresses.is_empty() {
        ResolvedRange::new(1, editor.line_count().max(1))
    } else {
        resolve_range(editor, &command.range)?
    };
    let mut matching = Vec::new();
    for lnum in range.start..=range.end {
        let Some(line) = editor.line(lnum) else {
            continue;
        };
        let matched = regex.find(line).is_some();
        if matched != inverse {
            matching.push(lnum);
        }
    }
    if subcommand.trim().is_empty() {
        let mut text = String::new();
        for lnum in &matching {
            if let Some(line) = editor.line(*lnum) {
                text.push_str(&String::from_utf8_lossy(line));
                text.push('\n');
            }
        }
        editor.messages.info(text.trim_end().to_string());
        return Ok(());
    }
    let subcommand = subcommand.trim().to_string();
    let mut offset = 0i64;
    for lnum in matching {
        let target = (lnum as i64 + offset).max(1) as u32;
        if target > editor.line_count() {
            break;
        }
        editor.set_cursor(BufPos::new(target, 0));
        let before = editor.line_count();
        execute_ex_line_nested(editor, &subcommand)?;
        offset += editor.line_count() as i64 - before as i64;
    }
    Ok(())
}

fn execute_ex_line_nested(editor: &mut Editor, command: &str) -> Result<(), VimError> {
    execute(editor, command)
}

fn parse_global_args(args: &str) -> Option<(String, String)> {
    let trimmed = args.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let delimiter = bytes[0] as char;
    if delimiter.is_ascii_alphanumeric() {
        return None;
    }
    let mut pattern = String::new();
    let mut index = 1;
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if ch == '\\' && index + 1 < bytes.len() {
            pattern.push(ch);
            pattern.push(bytes[index + 1] as char);
            index += 2;
            continue;
        }
        if ch == delimiter {
            break;
        }
        pattern.push(ch);
        index += 1;
    }
    let command = if index + 1 < bytes.len() {
        trimmed[index + 1..].to_string()
    } else {
        String::new()
    };
    Some((unescape_pattern(&pattern), command))
}

fn handle_substitute(editor: &mut Editor, command: &ExCommand) -> Result<(), VimError> {
    if command.args.trim().is_empty() {
        return repeat_substitute(editor, &command.range);
    }
    let Some((pattern, replacement, flags)) = parse_substitute(&command.args) else {
        return Err(VimError::new(474, "Invalid substitute arguments"));
    };
    let pattern = if pattern.is_empty() {
        if flags.contains('r') {
            editor
                .last_search
                .as_ref()
                .map(|(pattern, _)| pattern.clone())
        } else {
            editor.last_substitute_pattern.clone()
        }
        .ok_or_else(|| VimError::new(35, "No previous regular expression"))?
    } else {
        pattern
    };
    let replacement = if replacement == "~" {
        editor
            .last_substitute_replacement
            .clone()
            .unwrap_or_default()
    } else {
        replacement
    };
    substitute_impl(editor, &command.range, &pattern, &replacement, &flags)
}

fn repeat_substitute(editor: &mut Editor, range: &rvim_ex::RangeSpec) -> Result<(), VimError> {
    let pattern = editor
        .last_substitute_pattern
        .clone()
        .ok_or_else(|| VimError::new(35, "No previous regular expression"))?;
    let replacement = editor
        .last_substitute_replacement
        .clone()
        .unwrap_or_default();
    let flags = editor.last_substitute_flags.clone();
    substitute_impl(editor, range, &pattern, &replacement, &flags)
}

#[derive(Clone, Copy, Debug, Default)]
struct SubstituteFlags {
    global: bool,
    ignorecase: bool,
    no_error: bool,
    count_only: bool,
    print: bool,
    number: bool,
    list: bool,
    confirm: bool,
    count: Option<usize>,
}

fn parse_substitute_flags(raw: &str) -> SubstituteFlags {
    let mut flags = SubstituteFlags::default();
    let mut digits = String::new();
    for ch in raw.chars() {
        match ch {
            'g' => flags.global = true,
            'i' => flags.ignorecase = true,
            'I' => {}
            'e' => flags.no_error = true,
            'n' => flags.count_only = true,
            'p' => flags.print = true,
            '#' => flags.number = true,
            'l' => flags.list = true,
            'c' => flags.confirm = true,
            'r' => {}
            '&' => {}
            '0'..='9' => digits.push(ch),
            _ => {}
        }
    }
    flags.count = digits.parse::<usize>().ok().filter(|count| *count > 0);
    flags
}

fn substitute_impl(
    editor: &mut Editor,
    range: &rvim_ex::RangeSpec,
    pattern: &str,
    replacement: &str,
    raw_flags: &str,
) -> Result<(), VimError> {
    if !editor
        .options
        .buffer_bool(editor.current_buffer_id(), "modifiable")
    {
        return Err(VimError::new(
            21,
            "Cannot make changes, 'modifiable' is off",
        ));
    }
    let mut effective_flags = raw_flags.to_string();
    if effective_flags.contains('&') {
        effective_flags = format!(
            "{}{}",
            editor.last_substitute_flags,
            effective_flags.replace('&', "")
        );
    }
    let flags = parse_substitute_flags(&effective_flags);
    let range = if range.addresses.is_empty() {
        ResolvedRange::new(editor.cursor().lnum, editor.cursor().lnum)
    } else {
        resolve_range(editor, range)?
    };
    let ignorecase = flags.ignorecase
        || (editor.options.get_bool("ignorecase")
            && !effective_flags.contains('I')
            && !(editor.options.get_bool("smartcase") && pattern.chars().any(char::is_uppercase)));
    let magic = if editor.options.get_bool("magic") {
        rvim_regex::Magic::Magic
    } else {
        rvim_regex::Magic::Nomagic
    };
    let regex = rvim_regex::compile(pattern.as_bytes(), magic, ignorecase)
        .map_err(|error| VimError::new(486, error.message))?;
    let mut substitutions = 0usize;
    let mut last_pos = None;
    let mut replacements: Vec<(u32, Vec<u8>)> = Vec::new();
    for lnum in range.start..=range.end {
        let Some(line) = editor.line(lnum).map(|line| line.to_vec()) else {
            continue;
        };
        let mut output = Vec::with_capacity(line.len());
        let mut cursor = 0usize;
        let mut local_count = 0usize;
        let mut match_index = 0usize;
        while cursor <= line.len() {
            let Some(found) = regex.find_at(&line, cursor, true) else {
                break;
            };
            if found.start < cursor {
                break;
            }
            match_index += 1;
            let selected = flags
                .count
                .map(|wanted| match_index >= wanted && (flags.global || match_index == wanted))
                .unwrap_or(true);
            if !selected {
                output.extend_from_slice(&line[cursor..found.end]);
                cursor = found.end;
                if found.end == found.start {
                    if cursor < line.len() {
                        output.push(line[cursor]);
                        cursor += 1;
                    } else {
                        break;
                    }
                }
                continue;
            }
            output.extend_from_slice(&line[cursor..found.start]);
            let replaced = expand_replacement(replacement, &line, &found);
            output.extend_from_slice(&replaced);
            substitutions += 1;
            local_count += 1;
            last_pos = Some(BufPos::new(lnum, found.start as u32));
            if found.end == found.start {
                if cursor < line.len() {
                    output.push(line[cursor]);
                    cursor += 1;
                } else {
                    break;
                }
            } else {
                cursor = found.end;
            }
            if !flags.global {
                break;
            }
        }
        if local_count == 0 {
            continue;
        }
        output.extend_from_slice(&line[cursor..]);
        replacements.push((lnum, output));
    }
    if substitutions == 0 {
        if !flags.no_error {
            return Err(VimError::new(486, format!("Pattern not found: {pattern}")));
        }
        return Ok(());
    }
    if !flags.count_only {
        editor.substitute_lines(replacements.clone(), pattern);
        editor.last_substitute_pattern = Some(pattern.to_string());
        editor.last_substitute_replacement = Some(replacement.to_string());
        editor.last_substitute_flags = effective_flags;
    }
    if let Some(pos) = last_pos {
        editor.set_cursor(pos);
    }
    if flags.print || flags.number || flags.list {
        if let Some((lnum, text)) = replacements.last() {
            let mut shown = String::from_utf8_lossy(text).into_owned();
            if flags.list {
                shown = shown.replace('\t', "^I").replace('\r', "^M");
                shown.push('$');
            }
            if flags.number {
                shown = format!("{lnum}\t{shown}");
            }
            editor.messages.info(shown);
        }
    }
    editor.messages.info(format!(
        "{substitutions} substitution{}",
        if substitutions == 1 { "" } else { "s" }
    ));
    Ok(())
}

fn parse_substitute(args: &str) -> Option<(String, String, String)> {
    if args.trim().is_empty() {
        return None;
    }
    let bytes = args.as_bytes();
    let delimiter = bytes[0] as char;
    if delimiter.is_ascii_alphanumeric() || delimiter == '\\' {
        return None;
    }
    let mut pattern = String::new();
    let mut index = 1;
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if ch == '\\' && index + 1 < bytes.len() {
            pattern.push(ch);
            pattern.push(bytes[index + 1] as char);
            index += 2;
            continue;
        }
        if ch == delimiter {
            break;
        }
        pattern.push(ch);
        index += 1;
    }
    if index >= bytes.len() {
        return Some((unescape_pattern(&pattern), String::new(), String::new()));
    }
    index += 1;
    let mut replacement = String::new();
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if ch == '\\' && index + 1 < bytes.len() {
            replacement.push(ch);
            replacement.push(bytes[index + 1] as char);
            index += 2;
            continue;
        }
        if ch == delimiter {
            break;
        }
        replacement.push(ch);
        index += 1;
    }
    let flags = if index + 1 < bytes.len() {
        args[index + 1..].trim().to_string()
    } else {
        String::new()
    };
    Some((unescape_pattern(&pattern), replacement, flags))
}

fn unescape_pattern(pattern: &str) -> String {
    let mut out = String::new();
    let mut chars = pattern.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                if next == '/' || next == '\\' {
                    out.push(next);
                } else {
                    out.push('\\');
                    out.push(next);
                }
            } else {
                out.push('\\');
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn apply_replacement_case(out: &mut Vec<u8>, text: &str, mode: &mut u8, one: &mut u8) {
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

fn handle_highlight(editor: &mut Editor, args: &str) {
    let mut args = args.trim();
    if let Some(rest) = args.strip_prefix("def ") {
        args = rest.trim_start();
    }
    if let Some(rest) = args.strip_prefix("link ") {
        let mut parts = rest.split_whitespace();
        if let (Some(group), Some(target)) = (parts.next(), parts.next()) {
            let attr = editor.highlights.get(target);
            editor.highlights.set(group, attr);
            editor.request_redraw();
        }
        return;
    }
    if args.is_empty() {
        let mut names: Vec<String> = editor
            .highlights
            .iter()
            .map(|(name, _)| name.to_string())
            .collect();
        names.sort();
        editor.messages.info(names.join(" "));
        return;
    }
    if args == "clear" {
        editor.highlights = crate::highlights::HighlightTable::new();
        return;
    }
    let mut parts = args.split_whitespace();
    let group = parts.next().unwrap_or("");
    if group.is_empty() {
        return;
    }
    let mut attr = editor.highlights.get(group);
    let mut linked = None;
    for part in parts {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key {
            "ctermfg" | "guifg" => {
                if let Some(color) = parse_color(value) {
                    attr.fg = color;
                }
            }
            "ctermbg" | "guibg" => {
                if let Some(color) = parse_color(value) {
                    attr.bg = color;
                }
            }
            "cterm" | "gui" => {
                for flag in value.split(',') {
                    if let Some(flag) = parse_attr_flag(flag) {
                        attr.flags.insert(flag);
                    }
                }
            }
            "link" | "linksto" => linked = Some(value.to_string()),
            _ => {}
        }
    }
    if let Some(link) = linked {
        let target = editor.highlights.get(&link);
        editor.highlights.set(group, target);
    } else {
        editor.highlights.set(group, attr);
    }
    editor.request_redraw();
}
fn parse_attr_flag(name: &str) -> Option<rvim_ui_api::AttrFlags> {
    Some(match name {
        "bold" => rvim_ui_api::AttrFlags::BOLD,
        "italic" => rvim_ui_api::AttrFlags::ITALIC,
        "underline" => rvim_ui_api::AttrFlags::UNDERLINE,
        "undercurl" => rvim_ui_api::AttrFlags::UNDERCURL,
        "underdouble" => rvim_ui_api::AttrFlags::UNDERDOUBLE,
        "underdotted" => rvim_ui_api::AttrFlags::UNDERDOTTED,
        "underdashed" => rvim_ui_api::AttrFlags::UNDERDASHED,
        "strikethrough" => rvim_ui_api::AttrFlags::STRIKETHROUGH,
        "reverse" | "inverse" => rvim_ui_api::AttrFlags::REVERSE,
        "blink" => rvim_ui_api::AttrFlags::BLINK,
        "conceal" => rvim_ui_api::AttrFlags::CONCEAL,
        "dim" => rvim_ui_api::AttrFlags::DIM,
        _ => return None,
    })
}

fn expand_shell_command(editor: &Editor, command: &str) -> String {
    if command.is_empty() || command == "!" {
        return editor.last_bang.clone();
    }
    let bytes = command.as_bytes();
    let mut out = String::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if ch == '\\' && index + 1 < bytes.len() {
            let next = bytes[index + 1] as char;
            if matches!(next, '%' | '#' | '!') {
                out.push(next);
                index += 2;
                continue;
            }
        }
        if ch == '!' && bytes.get(index + 1) == Some(&b'!') {
            out.push_str(&editor.last_bang);
            index += 2;
            continue;
        }
        if ch == '%' || ch == '#' {
            let path = if ch == '%' {
                editor
                    .current_buffer()
                    .and_then(|buffer| buffer.text.meta.name.clone())
            } else {
                editor
                    .alternate_buffer
                    .and_then(|id| editor.buffers.get(id))
                    .and_then(|buffer| buffer.text.meta.name.clone())
            };
            if let Some(path) = path {
                let mut end = index + 1;
                let mut modifiers = String::new();
                if bytes.get(end) == Some(&b':') {
                    end += 1;
                    while end < bytes.len() {
                        let modifier = bytes[end] as char;
                        if modifier.is_ascii_alphanumeric() || modifier == ':' {
                            modifiers.push(modifier);
                            end += 1;
                        } else {
                            break;
                        }
                    }
                }
                let path = apply_path_modifiers(path, &modifiers);
                out.push_str(&shell_quote(&path.display().to_string()));
                index = end;
                continue;
            }
        }
        out.push(ch);
        index += 1;
    }
    out
}

fn parse_color(value: &str) -> Option<rvim_ui_api::Color> {
    let lower = value.to_ascii_lowercase();
    if lower == "none" {
        return Some(rvim_ui_api::Color::Default);
    }
    if let Some(hex) = lower.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(rvim_ui_api::Color::Rgb(r, g, b));
        }
    }
    if let Ok(index) = lower.parse::<u8>() {
        return Some(rvim_ui_api::Color::Indexed(index));
    }
    let index = match lower.as_str() {
        "black" => 0,
        "darkblue" => 1,
        "darkgreen" => 2,
        "darkcyan" => 3,
        "darkred" => 4,
        "darkmagenta" => 5,
        "brown" | "darkyellow" => 6,
        "lightgray" | "gray" | "grey" => 7,
        "darkgray" | "darkgrey" => 8,
        "blue" | "lightblue" => 9,
        "green" | "lightgreen" => 10,
        "cyan" | "lightcyan" => 11,
        "red" | "lightred" => 12,
        "magenta" | "lightmagenta" => 13,
        "yellow" | "lightyellow" => 14,
        "white" => 15,
        _ => return None,
    };
    Some(rvim_ui_api::Color::Indexed(index))
}

/// True when the token is an event name or a comma separated event list.
fn is_event_list(token: &str) -> bool {
    !token.is_empty()
        && token
            .split(',')
            .all(|name| !name.is_empty() && crate::autocmd::Event::parse(name).is_some())
}

fn handle_autocmd(editor: &mut Editor, command: &ExCommand) -> Result<(), VimError> {
    let args = command.args.trim();
    if command.bang && args.is_empty() {
        editor.autocmds.clear(None, None, None);
        return Ok(());
    }
    if args.is_empty() {
        let mut text = String::new();
        for autocmd in editor.autocmds.list() {
            text.push_str(&format!(
                "{}  {}  {}  {}\n",
                autocmd.group,
                autocmd.event.name(),
                autocmd.pattern,
                autocmd.command
            ));
        }
        editor.messages.info(text.trim_end().to_string());
        return Ok(());
    }
    if let Some(rest) = args.strip_prefix('!') {
        let mut parts = rest.split_whitespace();
        let first = parts.next();
        let group =
            first.filter(|value| editor.autocmds.groups.contains_key(*value) || *value == "*");
        let after_group = if group.is_some() { parts.next() } else { first };
        let event = after_group.and_then(crate::autocmd::Event::parse);
        let pattern = if event.is_some() {
            parts.next()
        } else {
            after_group
        };
        editor
            .autocmds
            .clear(group.filter(|group| *group != "*"), event.as_ref(), pattern);
        return Ok(());
    }
    if command.bang {
        let mut parts = args.split_whitespace();
        let first = parts.next();
        let group = first
            .filter(|value| editor.autocmds.groups.contains_key(*value) || *value == "*")
            .map(str::to_string)
            .or_else(|| editor.autocmds.current_group.clone());
        let after_group = if group.is_some() && first.is_some() && first != Some("all") {
            parts.next()
        } else {
            first
        };
        let event = after_group.and_then(crate::autocmd::Event::parse);
        let pattern = if event.is_some() {
            parts.next()
        } else {
            after_group
        };
        editor.autocmds.clear(
            group.as_deref().filter(|group| *group != "*"),
            event.as_ref(),
            pattern,
        );
        return Ok(());
    }
    let tokens: Vec<&str> = args.split_whitespace().collect();
    let first = *tokens.first().unwrap_or(&"");
    let (group, event_index) = if is_event_list(first) {
        (None, 0)
    } else {
        (Some(first), 1)
    };
    let event_token = *tokens.get(event_index).unwrap_or(&"");
    if event_token.is_empty() {
        return Err(VimError::plain("E216: No such event"));
    }
    let pattern = *tokens.get(event_index + 1).unwrap_or(&"");
    let mut command_start = event_index + 2;
    let mut once = false;
    let mut nested = false;
    while let Some(flag) = tokens.get(command_start) {
        if *flag == "++once" {
            once = true;
            command_start += 1;
        } else if *flag == "++nested" {
            nested = true;
            command_start += 1;
        } else {
            break;
        }
    }
    let command_text = tokens[command_start.min(tokens.len())..].join(" ");
    if pattern.is_empty() || command_text.is_empty() {
        return Err(VimError::plain("E217: Missing command"));
    }
    let command_text = command_text.as_str();
    let _ = nested;
    let previous_group = editor.autocmds.current_group.clone();
    if let Some(group) = group {
        editor.autocmds.current_group = Some(group.to_string());
    }
    for event_name in event_token.split(',') {
        let event = crate::autocmd::Event::parse(event_name)
            .ok_or_else(|| VimError::plain(format!("E216: No such event: {event_name}")))?;
        let id = editor
            .autocmds
            .add(event, pattern.to_string(), command_text.to_string());
        if once {
            if let Some(entry) = editor
                .autocmds
                .entries
                .iter_mut()
                .find(|entry| entry.id == id)
            {
                entry.once = true;
            }
        }
    }
    editor.autocmds.current_group = previous_group;
    Ok(())
}

/// Expand a user command invocation without running it.
///
/// Used by the script engine so a replacement that refers to `s:` variables
/// of the defining script runs inside the sourcing runtime.
pub(crate) fn expand_user_command_line(
    editor: &mut Editor,
    line: &str,
) -> Result<Option<(String, Option<rvim_types::ScriptId>)>, VimError> {
    let command = match rvim_ex::parse(line) {
        Ok(command) => command,
        Err(_) => return Ok(None),
    };
    let Some(user) = editor.user_commands.get(&command.name).cloned() else {
        return Ok(None);
    };
    let range = if command.range.addresses.is_empty() {
        ResolvedRange::new(editor.cursor().lnum, editor.cursor().lnum)
    } else {
        resolve_range(editor, &command.range)?
    };
    let expanded = user
        .expand(
            &command.args,
            command.bang,
            range.start,
            range.end,
            user.range,
        )
        .map_err(VimError::plain)?;
    Ok(Some((expanded, user.script_id)))
}

fn execute_user_command(
    editor: &mut Editor,
    command: &ExCommand,
    user: &crate::usercmd::UserCommand,
) -> Result<(), VimError> {
    let range = if command.range.addresses.is_empty() {
        ResolvedRange::new(editor.cursor().lnum, editor.cursor().lnum)
    } else {
        resolve_range(editor, &command.range)?
    };
    let expanded = user
        .expand(
            &command.args,
            command.bang,
            range.start,
            range.end,
            user.range,
        )
        .map_err(VimError::plain)?;
    let previous_override = editor.viml_script_override;
    editor.viml_script_override = user.script_id;
    let result = execute(editor, &expanded);
    editor.viml_script_override = previous_override;
    result
}

fn define_user_command(editor: &mut Editor, command: &ExCommand) -> Result<(), VimError> {
    let args = command.args.trim();
    if args.is_empty() {
        let mut text = String::new();
        let mut names: Vec<&String> = editor.user_commands.keys().collect();
        names.sort();
        for name in names {
            let user = &editor.user_commands[name];
            text.push_str(&format!("{:<16} {}\n", name, user.replacement));
        }
        editor.messages.info(text.trim_end().to_string());
        return Ok(());
    }
    let mut rest = args;
    let mut bang = false;
    if let Some(stripped) = rest.strip_prefix('!') {
        bang = true;
        rest = stripped.trim_start();
    }
    let mut nargs = Nargs::Zero;
    let mut range = false;
    let mut count = false;
    while rest.starts_with('-') {
        let (attribute, remaining) = rest
            .split_once(char::is_whitespace)
            .map(|(attribute, remaining)| (attribute, remaining.trim_start()))
            .unwrap_or((rest, ""));
        if let Some(value) = attribute.strip_prefix("-nargs=") {
            nargs = match value {
                "0" => Nargs::Zero,
                "1" => Nargs::One,
                "*" => Nargs::Any,
                "?" => Nargs::Optional,
                "+" => Nargs::Plus,
                _ => return Err(VimError::new(474, "E474: Invalid argument")),
            };
        } else if attribute == "-range" || attribute.starts_with("-range=") {
            range = true;
        } else if attribute == "-count" || attribute.starts_with("-count=") {
            count = true;
        } else if attribute == "-bang"
            || attribute == "-bar"
            || attribute == "-buffer"
            || attribute.starts_with("-complete=")
        {
            // Accepted; completion and bars are handled by the caller later.
        } else {
            return Err(VimError::new(
                474,
                format!("E474: Invalid argument: {attribute}"),
            ));
        }
        rest = remaining;
    }
    let (name, rest) = rest
        .split_once(char::is_whitespace)
        .map(|(name, rest)| (name, rest.trim_start()))
        .unwrap_or((rest, ""));
    if name.is_empty() || !name.chars().next().is_some_and(char::is_uppercase) {
        return Err(VimError::new(
            183,
            "E183: User defined commands must start with an uppercase letter",
        ));
    }
    if rest.is_empty() {
        editor.user_commands.remove(name);
        return Ok(());
    }
    editor.user_commands.insert(
        name.to_string(),
        crate::usercmd::UserCommand {
            name: name.to_string(),
            script_id: editor.viml_script_context,
            bang,
            nargs,
            range,
            count,
            replacement: rest.to_string(),
        },
    );
    Ok(())
}

fn check_modified(editor: &Editor, force: bool) -> Result<(), VimError> {
    if !force {
        if let Some(buffer) = editor.current_buffer() {
            if buffer.modified() && !editor.options.get_bool("hidden") {
                return Err(VimError::new(
                    37,
                    "No write since last change (add ! to override)",
                ));
            }
        }
    }
    Ok(())
}

fn check_readonly(editor: &mut Editor, force: bool) -> Result<(), VimError> {
    editor.force_write = force;
    if !force {
        if let Some(buffer) = editor.current_buffer() {
            if buffer.readonly {
                return Err(VimError::new(
                    45,
                    "'readonly' option is set (add ! to override)",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn buffer_argument(editor: &Editor, arg: &str) -> Result<rvim_types::BufferId, VimError> {
    let arg = arg.trim();
    if arg.is_empty() {
        return Ok(editor.current_buffer);
    }
    if let Ok(number) = arg.parse::<u32>() {
        return editor
            .buffer_by_number(number)
            .ok_or_else(|| VimError::new(94, "No matching buffer"));
    }
    let path = expand_path_arg(arg);
    editor
        .buffer_ids()
        .into_iter()
        .find(|id| {
            editor
                .buffers
                .get(*id)
                .and_then(|buffer| buffer.text.meta.name.as_ref())
                .is_some_and(|name| name == &path || name.file_name() == path.file_name())
        })
        .ok_or_else(|| VimError::new(94, "No matching buffer"))
}

fn resolve_file_arg(editor: &Editor, arg: &str) -> Result<std::path::PathBuf, VimError> {
    let arg = arg.trim();
    if let Some(modifiers) = arg.strip_prefix("%:") {
        let path = editor
            .current_buffer()
            .and_then(|buffer| buffer.text.meta.name.clone())
            .ok_or_else(|| VimError::new(32, "No file name"))?;
        return Ok(apply_path_modifiers_with_suffix(path, modifiers));
    }
    if arg == "%" {
        return editor
            .current_buffer()
            .and_then(|buffer| buffer.text.meta.name.clone())
            .ok_or_else(|| VimError::new(32, "No file name"));
    }
    if let Some(modifiers) = arg.strip_prefix("#:") {
        let alternate = editor
            .alternate_buffer
            .ok_or_else(|| VimError::new(94, "No alternate file"))?;
        let path = editor
            .buffers
            .get(alternate)
            .and_then(|buffer| buffer.text.meta.name.clone())
            .ok_or_else(|| VimError::new(32, "No file name"))?;
        return Ok(apply_path_modifiers_with_suffix(path, modifiers));
    }
    if arg == "#" {
        let alternate = editor
            .alternate_buffer
            .ok_or_else(|| VimError::new(94, "No alternate file"))?;
        return editor
            .buffers
            .get(alternate)
            .and_then(|buffer| buffer.text.meta.name.clone())
            .ok_or_else(|| VimError::new(32, "No file name"));
    }
    Ok(expand_path_arg(arg))
}

fn apply_path_modifiers_with_suffix(
    path: std::path::PathBuf,
    modifiers: &str,
) -> std::path::PathBuf {
    let (modifiers, suffix) = modifiers
        .split_once('/')
        .map(|(modifiers, suffix)| (modifiers, Some(suffix)))
        .unwrap_or((modifiers, None));
    let mut path = apply_path_modifiers(path, modifiers);
    if let Some(suffix) = suffix {
        if !suffix.is_empty() {
            path.push(suffix);
        }
    }
    path
}

fn apply_path_modifiers(mut path: std::path::PathBuf, modifiers: &str) -> std::path::PathBuf {
    for modifier in modifiers.split(':') {
        match modifier {
            "" | "p" => {
                if path.is_relative() {
                    path = rvim_platform::fs::cwd().join(path);
                }
                if modifier == "p" {
                    path = path.canonicalize().unwrap_or(path);
                }
            }
            "h" => {
                path = path
                    .parent()
                    .map(|parent| parent.to_path_buf())
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
            }
            "t" => {
                path = path
                    .file_name()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_default();
            }
            "r" => {
                if let Some(stem) = path.file_stem() {
                    let mut next = path
                        .parent()
                        .map(|parent| parent.to_path_buf())
                        .unwrap_or_default();
                    next.push(stem);
                    path = next;
                }
            }
            "e" => {
                path = path
                    .extension()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_default();
            }
            "~" => {
                if let Some(home) = rvim_platform::env::home().to_str() {
                    if let Ok(relative) = path.strip_prefix(home) {
                        path = relative.to_path_buf();
                    }
                }
            }
            _ => {}
        }
    }
    path
}

fn expand_path_arg(arg: &str) -> PathBuf {
    let unescaped = arg.replace("\\ ", " ").trim().to_string();
    rvim_platform::fs::expand_env(&unescaped)
}

fn reload_current(editor: &mut Editor) -> Result<(), VimError> {
    let path = editor
        .current_buffer()
        .and_then(|buffer| buffer.text.meta.name.clone())
        .ok_or_else(|| VimError::new(32, "No file name"))?;
    let bytes = rvim_platform::fs::read_file(&path)
        .map_err(|error| VimError::new(13, error.to_string()))?;
    if let Some(buffer) = editor.current_buffer_mut() {
        buffer.load_bytes(&bytes);
        buffer.set_modified(false);
        buffer.undo.clear();
    }
    editor.set_cursor(BufPos::new(1, 0));
    editor
        .messages
        .info(format!("\"{}\" {}L", path.display(), editor.line_count()));
    Ok(())
}

pub fn resolve_range(editor: &Editor, range: &RangeSpec) -> Result<ResolvedRange, VimError> {
    let current = editor.cursor().lnum;
    let last = editor.line_count().max(1);
    if range.addresses.is_empty() {
        return Ok(ResolvedRange::new(current, current));
    }
    if range.is_whole_file() {
        return Ok(ResolvedRange::new(1, last));
    }
    let mut lines = Vec::new();
    for address in &range.addresses {
        let mut line = match &address.kind {
            AddressKind::Current | AddressKind::None => current,
            AddressKind::Last => last,
            AddressKind::Line(line) => (*line).max(1).min(last),
            AddressKind::Mark(mark) => editor
                .current_buffer()
                .and_then(|buffer| buffer.marks.get(mark))
                .map(|pos| pos.lnum)
                .ok_or_else(|| VimError::new(20, format!("Mark not set: {mark}")))?,
            AddressKind::Search { pattern, direction } => {
                simple_search_between(editor, pattern, *direction > 0, current, current, true)
                    .map(|pos| pos.lnum)
                    .ok_or_else(|| VimError::new(486, format!("Pattern not found: {pattern}")))?
            }
            AddressKind::WholeFile => 1,
        };
        line = (line as i64 + address.offset as i64).clamp(1, last as i64) as u32;
        if let Some((pattern, direction)) = &address.offset_pattern {
            line = simple_search_between(editor, pattern, *direction > 0, line, line, true)
                .map(|pos| pos.lnum)
                .unwrap_or(line);
        }
        lines.push(line);
    }
    match lines.as_slice() {
        [only] => Ok(ResolvedRange::new(*only, *only)),
        [start, end] => Ok(ResolvedRange::new(*start, *end)),
        _ => Err(VimError::plain("E492: Too many addresses")),
    }
}

pub fn line_range(range: ResolvedRange) -> TextRange {
    TextRange::linewise(range.start, range.end)
}

pub fn simple_search(editor: &Editor, pattern: &str, forward: bool) -> Option<BufPos> {
    crate::search::search(editor, pattern, forward)
        .ok()
        .flatten()
}

pub fn simple_search_between(
    editor: &Editor,
    pattern: &str,
    forward: bool,
    from_line: u32,
    _to_line: u32,
    _include_current: bool,
) -> Option<BufPos> {
    let regex = crate::search::compile_pattern(editor, pattern).ok()?;
    let last = editor.line_count().max(1);
    let start = from_line.max(1).min(last);
    let order: Vec<u32> = if forward {
        (start..=last).chain(1..start).collect()
    } else {
        (1..=start).rev().chain((start + 1..=last).rev()).collect()
    };
    for lnum in order {
        let Some(line) = editor.line(lnum) else {
            continue;
        };
        let found = if forward {
            crate::search::find_in_line(&regex, line, 0)
        } else {
            rvim_regex::find_all(&regex, line).into_iter().last()
        };
        if let Some(found) = found {
            return Some(BufPos::new(lnum, found.start as u32));
        }
    }
    None
}

pub fn execute_visual(editor: &mut Editor, linewise: bool) -> Result<(), VimError> {
    let start = editor.cursor();
    let range = if linewise {
        TextRange::linewise(start.lnum, start.lnum)
    } else {
        TextRange::characterwise(start, start, true)
    };
    let _ = range;
    editor.mode = Mode::Normal;
    Ok(())
}

pub fn make_range(start: u32, end: u32) -> TextRange {
    TextRange::linewise(start, end)
}

pub fn range_kind_linewise() -> RangeKind {
    RangeKind::Linewise
}

pub fn vim_string(text: &str) -> VimString {
    VimString::from(text)
}

pub fn command_kind_ex() -> CommandKind {
    CommandKind::Ex
}

pub fn operator_delete() -> Operator {
    Operator::Delete
}

fn ex_escape_path(path: &str) -> String {
    path.replace('\\', "\\\\").replace(' ', "\\ ")
}

fn session_script(editor: &Editor) -> String {
    let mut out = String::from("\" RVim session\nset shortmess=aoO\nsilent only\n");
    out.push_str(&format!(
        "cd {}\n",
        ex_escape_path(&rvim_platform::fs::cwd().display().to_string())
    ));
    let tabs = editor.view.tab_ids();
    let current_tab = tabs
        .iter()
        .position(|tab| *tab == editor.view.current_tab)
        .unwrap_or(0);
    for (tab_index, tab) in tabs.iter().enumerate() {
        if tab_index > 0 {
            out.push_str("tabnew\n");
        }
        let Some(page) = editor.view.tabs.get(tab) else {
            continue;
        };
        let windows = rvim_view::layout::all_windows(&page.root);
        for (window_index, window_id) in windows.iter().enumerate() {
            if window_index > 0 {
                out.push_str("split\n");
            }
            let Some(window) = editor.view.windows.get(window_id) else {
                continue;
            };
            let name = editor
                .buffers
                .get(window.buffer)
                .and_then(|buffer| buffer.text.meta.name.as_ref())
                .map(|path| path.display().to_string());
            match name {
                Some(name) => out.push_str(&format!("edit {}\n", ex_escape_path(&name))),
                None => out.push_str("enew\n"),
            }
            if let Some(window) = editor.view.windows.get(window_id) {
                out.push_str(&format!(
                    "call cursor({}, {})\n",
                    window.cursor.lnum,
                    window.cursor.col + 1
                ));
            }
        }
    }
    if tabs.len() > 1 {
        out.push_str(&format!("tabnext {}\n", current_tab + 1));
    }
    out
}

fn view_path(editor: &Editor, arg: &str) -> Result<std::path::PathBuf, VimError> {
    if !arg.is_empty() {
        return resolve_file_arg(editor, arg);
    }
    let viewdir = editor.options.get_string("viewdir");
    let dir = rvim_platform::fs::expand_env(if viewdir.is_empty() {
        "~/.vim/view"
    } else {
        &viewdir
    });
    let name = editor
        .current_buffer()
        .and_then(|buffer| buffer.text.meta.name.as_ref())
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "unnamed".to_string());
    let name = name.replace('/', "%");
    Ok(dir.join(format!("{name}.vim")))
}
