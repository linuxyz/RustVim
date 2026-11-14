import pathlib
p = pathlib.Path("crates/rvim-syntax/src/lib.rs")
s = p.read_text()
s = s.replace("""struct Item {
    group: String,
    matcher: Matcher,
    words: Vec<Vec<u8>>,
    region_end: Option<Regex>,
}""", """struct Item {
    group: String,
    matcher: Matcher,
    words: Vec<Vec<u8>>,
    region_end: Option<Regex>,
    matchgroup: Option<String>,
    oneline: bool,
}""", 1)
s = s.replace("""struct RegionState {
    group: String,
    end: Option<Regex>,
}""", """struct RegionState {
    group: String,
    end: Option<Regex>,
    matchgroup: Option<String>,
}""", 1)
# item constructors (keyword, match, region)
s = s.replace("""                    matcher: Matcher::Keyword,
                    words,
                    region_end: None,
                });""", """                    matcher: Matcher::Keyword,
                    words,
                    region_end: None,
                    matchgroup: None,
                    oneline: false,
                });""", 1)
s = s.replace("""                    matcher: Matcher::Pattern(regex),
                    words: Vec::new(),
                    region_end: None,
                });""", """                    matcher: Matcher::Pattern(regex),
                    words: Vec::new(),
                    region_end: None,
                    matchgroup: None,
                    oneline: false,
                });""", 1)
# region parsing add vars
s = s.replace("""                let mut start = None;
                let mut end = None;
                for token in options.split_whitespace() {""", """                let mut start = None;
                let mut end = None;
                let mut matchgroup = None;
                let mut oneline = false;
                for token in options.split_whitespace() {""", 1)
s = s.replace("""                    } else if let Some(value) = token.strip_prefix("end=") {
                        end = Some(
                            compile(take_pattern(value).as_bytes(), Magic::Magic, false)
                                .map_err(|e| e.message)?,
                        );
                    }""", """                    } else if let Some(value) = token.strip_prefix("end=") {
                        end = Some(
                            compile(take_pattern(value).as_bytes(), Magic::Magic, false)
                                .map_err(|e| e.message)?,
                        );
                    } else if let Some(value) = token.strip_prefix("matchgroup=") {
                        matchgroup = Some(value.to_string());
                    } else if token == "oneline" {
                        oneline = true;
                    }""", 1)
s = s.replace("""                    matcher: Matcher::Pattern(start),
                    words: Vec::new(),
                    region_end: end,
                });""", """                    matcher: Matcher::Pattern(start),
                    words: Vec::new(),
                    region_end: end,
                    matchgroup,
                    oneline,
                });""", 1)
# active region drain logic
old = """        for state in self.regions.drain(..) {
            let end = state.end.as_ref().and_then(|end| end.find(line));
            if let Some(found) = end {
                spans.push(Span {
                    start: 0,
                    end: found.end,
                    group: state.group,
                });
            } else {
                spans.push(Span {
                    start: 0,
                    end: line.len(),
                    group: state.group.clone(),
                });
                active.push(state);
            }
        }"""
new = """        for state in self.regions.drain(..) {
            let end = state.end.as_ref().and_then(|end| end.find(line));
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
        }"""
assert old in s
s = s.replace(old, new, 1)
# region item matching branch
old2 = """            if let Some(end_pattern) = &item.region_end {
                for start in rvim_regex::find_all(pattern, line) {
                    if let Some(end) = end_pattern.find_at(line, start.end, true) {
                        spans.push(Span {
                            start: start.start,
                            end: end.end,
                            group: item.group.clone(),
                        });
                    } else {
                        spans.push(Span {
                            start: start.start,
                            end: line.len(),
                            group: item.group.clone(),
                        });
                        active.push(RegionState {
                            group: item.group.clone(),
                            end: Some(end_pattern.clone()),
                        });
                    }
                }"""
new2 = """            if let Some(end_pattern) = &item.region_end {
                for start in rvim_regex::find_all(pattern, line) {
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
                            });
                        }
                    }
                }"""
assert old2 in s
s = s.replace(old2, new2, 1)
p.write_text(s)
print("patched")
