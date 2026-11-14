//! Syntax highlighting engine for Vim-style syntax definitions.
//!
//! The engine supports keyword, match and region items, containers, basic
//! clusters and multi-line regions. Highlight groups are resolved by the
//! editor; this crate only returns group names.

use rvim_regex::{compile, Magic, Regex};

#[derive(Clone, Debug)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub group: String,
}

#[derive(Clone, Debug)]
enum Matcher {
    Keyword,
    Pattern(Regex),
}

#[derive(Clone, Debug)]
struct Item {
    group: String,
    matcher: Matcher,
    words: Vec<Vec<u8>>,
    region_end: Option<Regex>,
    matchgroup: Option<String>,
    oneline: bool,
    nextgroup: Vec<String>,
    contains: Vec<String>,
    containedin: Vec<String>,
    contained: bool,
}

#[derive(Clone, Debug, Default)]
struct RegionState {
    group: String,
    end: Option<Regex>,
    matchgroup: Option<String>,
    contains: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct SyntaxEngine {
    enabled: bool,
    items: Vec<Item>,
    regions: Vec<RegionState>,
    last_filetype: Option<String>,
}

impl SyntaxEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.regions.clear();
    }

    pub fn filetype(&self) -> Option<&str> {
        self.last_filetype.as_deref()
    }

    pub fn set_filetype(&mut self, filetype: &str) {
        self.last_filetype = Some(filetype.to_string());
    }

    pub fn define(&mut self, command: &str) -> Result<(), String> {
        let command = command.trim();
        let (kind, rest) = command
            .split_once(char::is_whitespace)
            .map(|(kind, rest)| (kind.to_ascii_lowercase(), rest.trim()))
            .unwrap_or((command.to_ascii_lowercase(), ""));
        if kind.is_empty() {
            return Ok(());
        }
        match kind.as_str() {
            "keyword" => {
                let mut tokens = rest.split_whitespace();
                let group = tokens
                    .next()
                    .ok_or_else(|| "E28: Missing group name".to_string())?;
                let mut words = Vec::new();
                let mut contains = Vec::new();
                let mut containedin = Vec::new();
                let mut contained = false;
                for word in tokens {
                    if let Some(value) = word.strip_prefix("contains=") {
                        contains = value
                            .split(',')
                            .filter(|group| !group.is_empty())
                            .map(str::to_string)
                            .collect();
                    } else if let Some(value) = word.strip_prefix("containedin=") {
                        containedin = value
                            .split(',')
                            .filter(|group| !group.is_empty())
                            .map(str::to_string)
                            .collect();
                    } else if word == "contained" {
                        contained = true;
                    } else if !word.contains('=')
                        && !matches!(
                            word,
                            "display" | "transparent" | "skipwhite" | "skipnl" | "skipempty"
                        )
                    {
                        words.push(word.as_bytes().to_vec());
                    }
                }
                self.items.push(Item {
                    group: group.to_string(),
                    matcher: Matcher::Keyword,
                    words,
                    region_end: None,
                    matchgroup: None,
                    oneline: false,
                    nextgroup: Vec::new(),
                    contains,
                    containedin,
                    contained,
                });
            }
            "match" => {
                let mut parts = rest.splitn(2, char::is_whitespace);
                let group = parts
                    .next()
                    .ok_or_else(|| "E28: Missing group name".to_string())?;
                let pattern = parts
                    .next()
                    .ok_or_else(|| "E475: Invalid argument".to_string())?;
                let pattern = take_pattern(pattern);
                let regex = compile(pattern.as_bytes(), Magic::Magic, false)
                    .map_err(|error| error.message)?;
                self.items.push(Item {
                    group: group.to_string(),
                    matcher: Matcher::Pattern(regex),
                    words: Vec::new(),
                    region_end: None,
                    matchgroup: None,
                    oneline: false,
                    nextgroup: Vec::new(),
                    contains: Vec::new(),
                    containedin: Vec::new(),
                    contained: false,
                });
            }
            "region" => {
                let mut parts = rest.splitn(2, char::is_whitespace);
                let group = parts
                    .next()
                    .ok_or_else(|| "E28: Missing group name".to_string())?;
                let options = parts.next().unwrap_or("");
                let mut start = None;
                let mut end = None;
                let mut matchgroup = None;
                let mut oneline = false;
                let mut nextgroup = Vec::new();
                let mut contains = Vec::new();
                let mut containedin = Vec::new();
                let mut contained = false;
                for token in options.split_whitespace() {
                    if let Some(value) = token.strip_prefix("start=") {
                        start = Some(
                            compile(take_pattern(value).as_bytes(), Magic::Magic, false)
                                .map_err(|e| e.message)?,
                        );
                    } else if let Some(value) = token.strip_prefix("end=") {
                        end = Some(
                            compile(take_pattern(value).as_bytes(), Magic::Magic, false)
                                .map_err(|e| e.message)?,
                        );
                    } else if let Some(value) = token.strip_prefix("matchgroup=") {
                        matchgroup = Some(value.to_string());
                    } else if token == "contained" {
                        contained = true;
                    } else if token == "oneline" {
                        oneline = true;
                    } else if let Some(value) = token.strip_prefix("nextgroup=") {
                        nextgroup = value
                            .split(',')
                            .filter(|group| !group.is_empty())
                            .map(str::to_string)
                            .collect();
                    } else if let Some(value) = token.strip_prefix("contains=") {
                        contains = value
                            .split(',')
                            .filter(|group| !group.is_empty())
                            .map(str::to_string)
                            .collect();
                    } else if let Some(value) = token.strip_prefix("containedin=") {
                        containedin = value
                            .split(',')
                            .filter(|group| !group.is_empty())
                            .map(str::to_string)
                            .collect();
                    }
                }
                let start = start.ok_or_else(|| "E475: Missing start".to_string())?;
                self.items.push(Item {
                    group: group.to_string(),
                    matcher: Matcher::Pattern(start),
                    words: Vec::new(),
                    region_end: end,
                    matchgroup,
                    oneline,
                    nextgroup,
                    contains,
                    containedin,
                    contained,
                });
            }
            "clear" => self.clear(),
            // Runtime syntax files use these container/directive commands. They
            // do not affect the flat highlighter yet, so ignore them rather than
            // aborting a runtime source.
            "cluster" | "include" | "sync" | "case" | "spell" | "iskeyword" | "conceal"
            | "foldlevel" | "enable" | "disable" => {}
            _ => {}
        }
        Ok(())
    }

    fn allowed_toplevel(&self, item: &Item, limits: &[(usize, String, Vec<String>)]) -> bool {
        !(limits.is_empty() && (item.contained || !item.containedin.is_empty()))
    }

    fn allowed_in_active(
        &self,
        item: &Item,
        start: usize,
        limits: &[(usize, String, Vec<String>)],
    ) -> bool {
        let mut allowed = true;
        for (end, group, contains) in limits {
            if start >= *end || item.group == *group {
                continue;
            }
            allowed = contains
                .iter()
                .any(|candidate| candidate == &item.group || candidate == "ALL")
                || item.containedin.iter().any(|candidate| candidate == group);
        }
        allowed
    }

    fn push_next_groups(
        &self,
        spans: &mut Vec<Span>,
        line: &[u8],
        groups: &[String],
        position: usize,
        depth: usize,
    ) {
        if depth > 4 || position >= line.len() {
            return;
        }
        for group in groups {
            let Some(item) = self.items.iter().find(|item| item.group == *group) else {
                continue;
            };
            let Matcher::Pattern(pattern) = &item.matcher else {
                continue;
            };
            let Some(found) = pattern.find_at(line, position, true) else {
                continue;
            };
            if found.start != position || found.end <= found.start {
                continue;
            }
            if let Some(end_pattern) = &item.region_end {
                if let Some(end) = end_pattern.find_at(line, found.end, true) {
                    spans.push(Span {
                        start: found.start,
                        end: end.end,
                        group: item.group.clone(),
                    });
                    self.push_next_groups(spans, line, &item.nextgroup, end.end, depth + 1);
                } else {
                    spans.push(Span {
                        start: found.start,
                        end: line.len(),
                        group: item.group.clone(),
                    });
                }
            } else {
                spans.push(Span {
                    start: found.start,
                    end: found.end,
                    group: item.group.clone(),
                });
                self.push_next_groups(spans, line, &item.nextgroup, found.end, depth + 1);
            }
            break;
        }
    }

    pub fn highlight_line(&mut self, line: &[u8]) -> Vec<Span> {
        if !self.enabled {
            return Vec::new();
        }
        let mut spans = Vec::new();
        let mut active = Vec::new();
        let mut limits: Vec<(usize, String, Vec<String>)> = Vec::new();
        for state in self.regions.drain(..) {
            let end = state.end.as_ref().and_then(|end| end.find(line));
            limits.push((
                end.as_ref().map_or(line.len(), |found| found.end),
                state.group.clone(),
                state.contains.clone(),
            ));
            if let Some(found) = end {
                if let Some(matchgroup) = &state.matchgroup {
                    if found.start > 0 {
                        spans.push(Span {
                            start: 0,
                            end: found.start,
                            group: state.group,
                        });
                    }
                    spans.push(Span {
                        start: found.start,
                        end: found.end,
                        group: matchgroup.clone(),
                    });
                } else {
                    spans.push(Span {
                        start: 0,
                        end: found.end,
                        group: state.group,
                    });
                }
            } else {
                spans.push(Span {
                    start: 0,
                    end: line.len(),
                    group: state.group.clone(),
                });
                active.push(state);
            }
        }
        for item in &self.items {
            let Matcher::Pattern(pattern) = &item.matcher else {
                if matches!(item.matcher, Matcher::Keyword) {
                    for word in &item.words {
                        let mut start = 0;
                        while start + word.len() <= line.len() {
                            if &line[start..start + word.len()] == word.as_slice()
                                && (start == 0 || !is_word(line[start - 1]))
                                && (start + word.len() == line.len()
                                    || !is_word(line[start + word.len()]))
                                && self.allowed_toplevel(item, &limits)
                                && self.allowed_in_active(item, start, &limits)
                            {
                                spans.push(Span {
                                    start,
                                    end: start + word.len(),
                                    group: item.group.clone(),
                                });
                            }
                            start += 1;
                        }
                    }
                }
                continue;
            };
            if let Some(end_pattern) = &item.region_end {
                for start in rvim_regex::find_all(pattern, line) {
                    if !self.allowed_toplevel(item, &limits)
                        || !self.allowed_in_active(item, start.start, &limits)
                    {
                        continue;
                    }
                    if let Some(end) = end_pattern.find_at(line, start.end, true) {
                        if let Some(matchgroup) = &item.matchgroup {
                            spans.push(Span {
                                start: start.start,
                                end: start.end,
                                group: matchgroup.clone(),
                            });
                            if end.start > start.end {
                                spans.push(Span {
                                    start: start.end,
                                    end: end.start,
                                    group: item.group.clone(),
                                });
                            }
                            spans.push(Span {
                                start: end.start,
                                end: end.end,
                                group: matchgroup.clone(),
                            });
                        } else {
                            spans.push(Span {
                                start: start.start,
                                end: end.end,
                                group: item.group.clone(),
                            });
                        }
                        self.push_next_groups(&mut spans, line, &item.nextgroup, end.end, 0);
                    } else {
                        spans.push(Span {
                            start: start.start,
                            end: line.len(),
                            group: item.group.clone(),
                        });
                        if !item.oneline {
                            active.push(RegionState {
                                group: item.group.clone(),
                                end: Some(end_pattern.clone()),
                                matchgroup: item.matchgroup.clone(),
                                contains: item.contains.clone(),
                            });
                        }
                    }
                }
            } else {
                for found in rvim_regex::find_all(pattern, line) {
                    if found.end > found.start && self.allowed_in_active(item, found.start, &limits)
                    {
                        spans.push(Span {
                            start: found.start,
                            end: found.end,
                            group: item.group.clone(),
                        });
                    }
                }
            }
        }
        self.regions = active;
        let region_groups: std::collections::HashSet<String> = self
            .items
            .iter()
            .filter(|item| item.region_end.is_some())
            .map(|item| item.group.clone())
            .collect();
        let matchgroups: std::collections::HashSet<String> = self
            .items
            .iter()
            .filter_map(|item| item.matchgroup.clone())
            .collect();
        let region_contains: std::collections::HashMap<String, Vec<String>> = self
            .items
            .iter()
            .filter(|item| item.region_end.is_some())
            .map(|item| (item.group.clone(), item.contains.clone()))
            .collect();
        let regions: Vec<(usize, usize, String)> = spans
            .iter()
            .filter(|span| region_groups.contains(&span.group))
            .map(|span| (span.start, span.end, span.group.clone()))
            .collect();
        spans.retain(|span| {
            if region_groups.contains(&span.group) || matchgroups.contains(&span.group) {
                return true;
            }
            let enclosing = regions
                .iter()
                .filter(|(start, end, _)| *start < span.start && span.start < *end)
                .min_by_key(|(start, end, _)| end.saturating_sub(*start));
            match enclosing {
                Some((_, _, group)) => region_contains.get(group).is_some_and(|contains| {
                    contains
                        .iter()
                        .any(|candidate| candidate == &span.group || candidate == "ALL")
                }),
                None => true,
            }
        });
        spans.sort_by_key(|span| (span.start, std::cmp::Reverse(span.end)));
        spans
    }
}

fn take_pattern(value: &str) -> &str {
    let value = value.trim_start();
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes[0].is_ascii_alphanumeric() || bytes[0] == b'\\' {
        return value.split_whitespace().next().unwrap_or("");
    }
    let delimiter = bytes[0];
    let mut index = 1;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index += 2;
            continue;
        }
        if bytes[index] == delimiter {
            return &value[1..index];
        }
        index += 1;
    }
    value
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_and_matches() {
        let mut engine = SyntaxEngine::new();
        engine.set_enabled(true);
        engine.define("keyword Keyword fn let").unwrap();
        engine.define(r"match Comment /\/\/.*/").unwrap();
        let spans = engine.highlight_line(b"let x = 1; // hi");
        assert!(spans
            .iter()
            .any(|span| span.group == "Keyword" && span.start == 0));
        assert!(spans
            .iter()
            .any(|span| span.group == "Comment" && span.start == 11));
    }

    #[test]
    fn multiline_region_terminates_on_later_line() {
        let mut engine = SyntaxEngine::new();
        engine.set_enabled(true);
        engine
            .define("region String start=/begin/ end=/end/")
            .unwrap();
        let first = engine.highlight_line(b"begin");
        assert_eq!(first.len(), 1);
        assert_eq!((first[0].start, first[0].end), (0, 5));
        let middle = engine.highlight_line(b"middle");
        assert_eq!(middle.len(), 1);
        assert_eq!((middle[0].start, middle[0].end), (0, 6));
        let last = engine.highlight_line(b"end tail");
        assert_eq!(last.len(), 1);
        assert_eq!((last[0].start, last[0].end), (0, 3));
        let after = engine.highlight_line(b"plain");
        assert!(after.is_empty());
    }

    #[test]
    fn matchgroup_and_oneline_regions() {
        let mut engine = SyntaxEngine::new();
        engine.set_enabled(true);
        engine
            .define("region String start=/\"/ end=/\"/ matchgroup=Delimiter")
            .unwrap();
        let spans = engine.highlight_line(b"say \"hi\"");
        assert!(spans
            .iter()
            .any(|span| span.group == "Delimiter" && span.start == 4 && span.end == 5));
        assert!(spans
            .iter()
            .any(|span| span.group == "String" && span.start == 5 && span.end == 7));

        let mut oneline = SyntaxEngine::new();
        oneline.set_enabled(true);
        oneline
            .define("region Comment start=/<!--/ end=/-->/ oneline")
            .unwrap();
        assert!(!oneline.highlight_line(b"<!-- unfinished").is_empty());
        assert!(oneline.highlight_line(b"next line").is_empty());
    }

    #[test]
    fn nextgroup_matches_after_region_end() {
        let mut engine = SyntaxEngine::new();
        engine.set_enabled(true);
        engine
            .define("region Tag start=/</ end=/>/ nextgroup=String")
            .unwrap();
        engine.define("match String /[a-z]\\+/").unwrap();
        let spans = engine.highlight_line(b"<x>abc");
        assert!(spans
            .iter()
            .any(|span| span.group == "Tag" && span.start == 0 && span.end == 3));
        assert!(spans
            .iter()
            .any(|span| span.group == "String" && span.start == 3 && span.end == 6));
    }

    #[test]
    fn contains_allows_nested_groups_inside_region() {
        let mut engine = SyntaxEngine::new();
        engine.set_enabled(true);
        engine
            .define("region Comment start=/\\/\\*/ end=/\\*\\// contains=Todo")
            .unwrap();
        engine.define("keyword Todo TODO").unwrap();
        assert!(engine
            .highlight_line(b"/*")
            .iter()
            .any(|span| span.group == "Comment"));
        let nested = engine.highlight_line(b"TODO");
        assert!(nested.iter().any(|span| span.group == "Todo"));

        let mut strict = SyntaxEngine::new();
        strict.set_enabled(true);
        strict
            .define("region Comment start=/\\/\\*/ end=/\\*\\//")
            .unwrap();
        strict.define("keyword Todo TODO").unwrap();
        assert!(strict
            .highlight_line(b"/*")
            .iter()
            .any(|span| span.group == "Comment"));
        let nested = strict.highlight_line(b"TODO");
        assert!(!nested.iter().any(|span| span.group == "Todo"));
    }

    #[test]
    fn containedin_can_enter_named_region() {
        let mut engine = SyntaxEngine::new();
        engine.set_enabled(true);
        engine
            .define("region Comment start=/\\/\\*/ end=/\\*\\//")
            .unwrap();
        engine
            .define("keyword Todo TODO containedin=Comment")
            .unwrap();
        assert!(engine
            .highlight_line(b"/*")
            .iter()
            .any(|span| span.group == "Comment"));
        assert!(engine
            .highlight_line(b"TODO")
            .iter()
            .any(|span| span.group == "Todo"));

        let mut top = SyntaxEngine::new();
        top.set_enabled(true);
        top.define("keyword Todo TODO containedin=Comment").unwrap();
        assert!(!top
            .highlight_line(b"TODO")
            .iter()
            .any(|span| span.group == "Todo"));
    }

    #[test]
    fn same_line_region_containers_filter_nested_groups() {
        let mut engine = SyntaxEngine::new();
        engine.set_enabled(true);
        engine
            .define("region Comment start=/\\/\\*/ end=/\\*\\// contains=Todo")
            .unwrap();
        engine.define("keyword Todo TODO").unwrap();
        let spans = engine.highlight_line(b"/* TODO */");
        assert!(spans.iter().any(|span| span.group == "Comment"));
        assert!(spans.iter().any(|span| span.group == "Todo"));
        assert!(spans
            .iter()
            .any(|span| span.group == "Todo" && span.start == 3));

        let mut strict = SyntaxEngine::new();
        strict.set_enabled(true);
        strict
            .define("region Comment start=/\\/\\*/ end=/\\*\\//")
            .unwrap();
        strict.define("keyword Todo TODO").unwrap();
        let spans = strict.highlight_line(b"/* TODO */");
        assert!(spans.iter().any(|span| span.group == "Comment"));
        assert!(!spans.iter().any(|span| span.group == "Todo"));
    }
}
