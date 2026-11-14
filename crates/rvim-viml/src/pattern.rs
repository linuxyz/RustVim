//! Vim pattern matching for =~, matchstr(), substitute() and :catch.
//!
//! This is a thin adapter over the real Vim regex engine in rvim-regex, so
//! lookaround, \zs/\ze, backreferences and Vim character classes behave the
//! same everywhere in the editor.

use crate::error::{Result, VimError};
use rvim_regex::{compile as compile_regex, Magic, Regex};

#[derive(Clone, Debug)]
pub struct Pattern {
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Match {
    pub start: usize,
    pub end: usize,
    pub captures: Vec<Option<(usize, usize)>>,
}

impl Match {
    pub fn capture(&self, index: usize) -> Option<(usize, usize)> {
        if index == 0 {
            Some((self.start, self.end))
        } else {
            self.captures.get(index - 1).copied().flatten()
        }
    }
}

fn pattern_error(error: rvim_regex::RegexError) -> VimError {
    VimError::new(383, format!("Invalid argument: {}", error.message))
}

pub fn compile(pattern: &[u8]) -> Result<Pattern> {
    compile_regex(pattern, Magic::Magic, false).map_err(pattern_error)?;
    Ok(Pattern {
        bytes: pattern.to_vec(),
    })
}

fn regex_for(pattern: &Pattern, ignorecase: bool) -> Result<Regex> {
    compile_regex(&pattern.bytes, Magic::Magic, ignorecase).map_err(pattern_error)
}

pub fn find(pattern: &Pattern, text: &[u8], from: usize, ignorecase: bool) -> Option<Match> {
    let regex = regex_for(pattern, ignorecase).ok()?;
    let from = from.min(text.len());
    regex.find_at(text, from, true).map(|found| Match {
        start: found.start,
        end: found.end,
        captures: found.captures,
    })
}

#[cfg(test)]
pub fn is_match(pattern: &Pattern, text: &[u8], ignorecase: bool) -> bool {
    find(pattern, text, 0, ignorecase).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_and_dot() {
        let pattern = compile(b"a.c").unwrap();
        assert!(is_match(&pattern, b"abc", false));
        assert!(!is_match(&pattern, b"ac", false));
    }

    #[test]
    fn star_and_anchors() {
        let pattern = compile(b"^ab*$").unwrap();
        assert!(is_match(&pattern, b"abbb", false));
        assert!(!is_match(&pattern, b"xab", false));
    }

    #[test]
    fn classes_groups_and_alternation() {
        let pattern = compile(b"^\\(foo\\|bar\\)[0-9]\\+$").unwrap();
        assert!(is_match(&pattern, b"foo123", false));
        assert!(is_match(&pattern, b"bar7", false));
        assert!(!is_match(&pattern, b"baz7", false));
    }

    #[test]
    fn captures_and_zero_capture() {
        let pattern = compile(b"\\(a\\+\\)-\\(b\\+\\)").unwrap();
        let found = find(&pattern, b"xxaa-bbxx", 0, false).unwrap();
        assert_eq!(found.capture(0), Some((found.start, found.end)));
        assert_eq!(found.capture(1), Some((2, 4)));
        assert_eq!(found.capture(2), Some((5, 7)));
    }

    #[test]
    fn lookaround_and_zs() {
        let pattern = compile(b"foo\\zsbar").unwrap();
        let found = find(&pattern, b"foobar", 0, false).unwrap();
        assert_eq!((found.start, found.end), (3, 6));
        assert!(is_match(
            &compile(b"foo\\(bar\\)\\@=").unwrap(),
            b"foobar",
            false
        ));
    }

    #[test]
    fn ignorecase_and_override() {
        let pattern = compile(b"abc").unwrap();
        assert!(is_match(&pattern, b"ABC", true));
        assert!(!is_match(&pattern, b"ABC", false));
        let sensitive = compile(b"\\Cabc").unwrap();
        assert!(!is_match(&sensitive, b"ABC", true));
    }
}
