import pathlib
p = pathlib.Path("crates/rvim-syntax/src/lib.rs")
s = p.read_text()
s = s.replace("""    matchgroup: Option<String>,
    oneline: bool,
}""", """    matchgroup: Option<String>,
    oneline: bool,
    nextgroup: Vec<String>,
}""", 1)
# constructors keyword/match
s = s.replace("""                    matchgroup: None,
                    oneline: false,
                });""", """                    matchgroup: None,
                    oneline: false,
                    nextgroup: Vec::new(),
                });""", 2)
# region parsing add nextgroup
s = s.replace("""                let mut matchgroup = None;
                let mut oneline = false;""", """                let mut matchgroup = None;
                let mut oneline = false;
                let mut nextgroup = Vec::new();""", 1)
s = s.replace("""                    } else if token == "oneline" {
                        oneline = true;
                    }""", """                    } else if token == "oneline" {
                        oneline = true;
                    } else if let Some(value) = token.strip_prefix("nextgroup=") {
                        nextgroup = value
                            .split(',')
                            .filter(|group| !group.is_empty())
                            .map(str::to_string)
                            .collect();
                    }""", 1)
s = s.replace("""                    matchgroup,
                    oneline,
                });""", """                    matchgroup,
                    oneline,
                    nextgroup,
                });""", 1)
# add nextgroup after region end
s = s.replace("""                            spans.push(Span {
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
                        }""", """                            spans.push(Span {
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
                        self.push_next_groups(&mut spans, line, &item.nextgroup, end.end, 0);""", 1)
# add helper method before highlight_line
anchor = """    pub fn highlight_line(&mut self, line: &[u8]) -> Vec<Span> {"""
helper = """    fn push_next_groups(
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

    pub fn highlight_line(&mut self, line: &[u8]) -> Vec<Span> {"""
assert anchor in s
s = s.replace(anchor, helper, 1)
p.write_text(s)
print("patched")
