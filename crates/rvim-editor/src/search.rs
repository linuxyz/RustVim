use rvim_regex::{compile, Magic, Match, Regex};
use rvim_types::{BufPos, VimError};

use crate::editor::Editor;

pub fn compile_pattern(editor: &Editor, pattern: &str) -> Result<Regex, VimError> {
    if pattern.is_empty() {
        return Err(VimError::new(35, "No previous regular expression"));
    }
    let magic = if editor.options.get_bool("magic") {
        Magic::Magic
    } else {
        Magic::Nomagic
    };
    let ignorecase = editor.options.get_bool("ignorecase")
        && !(editor.options.get_bool("smartcase") && pattern.chars().any(char::is_uppercase));
    compile(pattern.as_bytes(), magic, ignorecase)
        .map_err(|error| VimError::new(486, error.message))
}

pub fn find_in_line(regex: &Regex, line: &[u8], start: usize) -> Option<Match> {
    regex.find_at(line, start.min(line.len()), true)
}

pub fn search(editor: &Editor, pattern: &str, forward: bool) -> Result<Option<BufPos>, VimError> {
    let regex = compile_pattern(editor, pattern)?;
    let current = editor.cursor();
    let last = editor.line_count().max(1);
    let wrapscan = editor.options.get_bool("wrapscan");
    let order: Vec<u32> = if forward {
        if wrapscan {
            (current.lnum..=last).chain(1..current.lnum).collect()
        } else {
            (current.lnum..=last).collect()
        }
    } else if wrapscan {
        (1..=current.lnum)
            .rev()
            .chain((current.lnum + 1..=last).rev())
            .collect()
    } else {
        (1..=current.lnum).rev().collect()
    };
    for lnum in order {
        let Some(line) = editor.line(lnum) else {
            continue;
        };
        if forward {
            let start = if lnum == current.lnum {
                current.col as usize + 1
            } else {
                0
            };
            if let Some(found) = find_in_line(&regex, line, start) {
                return Ok(Some(BufPos::new(lnum, found.start as u32)));
            }
            if lnum == current.lnum && current.col > 0 {
                if let Some(found) = find_in_line(&regex, line, 0) {
                    return Ok(Some(BufPos::new(lnum, found.start as u32)));
                }
            }
        } else {
            let matches = rvim_regex::find_all(&regex, line);
            if lnum == current.lnum {
                if let Some(found) = matches
                    .iter()
                    .rev()
                    .find(|found| found.start < current.col as usize)
                {
                    return Ok(Some(BufPos::new(lnum, found.start as u32)));
                }
            } else if let Some(found) = matches.last() {
                return Ok(Some(BufPos::new(lnum, found.start as u32)));
            }
        }
    }
    Ok(None)
}
