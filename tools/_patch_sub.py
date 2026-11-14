import pathlib
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
start = s.index("fn handle_substitute(editor: &mut Editor, command: &ExCommand) -> Result<(), VimError> {")
end = s.index("fn parse_substitute(args: &str)", start)
new = r'''fn handle_substitute(editor: &mut Editor, command: &ExCommand) -> Result<(), VimError> {
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
            && !(editor.options.get_bool("smartcase")
                && pattern.chars().any(char::is_uppercase)));
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

'''
s = s[:start] + new + s[end:]
p.write_text(s)
print("patched")
