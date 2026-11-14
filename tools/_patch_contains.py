import pathlib
p = pathlib.Path("crates/rvim-syntax/src/lib.rs")
s = p.read_text()
s = s.replace("""    oneline: bool,
    nextgroup: Vec<String>,
}""", """    oneline: bool,
    nextgroup: Vec<String>,
    contains: Vec<String>,
    containedin: Vec<String>,
}""", 1)
s = s.replace("""    end: Option<Regex>,
    matchgroup: Option<String>,
}""", """    end: Option<Regex>,
    matchgroup: Option<String>,
    contains: Vec<String>,
}""", 1)
# constructors keyword/match
s = s.replace("""                    oneline: false,
                    nextgroup: Vec::new(),
                });""", """                    oneline: false,
                    nextgroup: Vec::new(),
                    contains: Vec::new(),
                    containedin: Vec::new(),
                });""", 2)
# region parsing vars
s = s.replace("""                let mut nextgroup = Vec::new();""", """                let mut nextgroup = Vec::new();
                let mut contains = Vec::new();
                let mut containedin = Vec::new();""", 1)
s = s.replace("""                    } else if let Some(value) = token.strip_prefix("nextgroup=") {
                        nextgroup = value
                            .split(',')
                            .filter(|group| !group.is_empty())
                            .map(str::to_string)
                            .collect();
                    }""", """                    } else if let Some(value) = token.strip_prefix("nextgroup=") {
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
                    }""", 1)
s = s.replace("""                    oneline,
                    nextgroup,
                });""", """                    oneline,
                    nextgroup,
                    contains,
                    containedin,
                });""", 1)
# active loop add limits
s = s.replace("""        let mut spans = Vec::new();
        let mut active = Vec::new();
        for state in self.regions.drain(..) {""", """        let mut spans = Vec::new();
        let mut active = Vec::new();
        let mut limits: Vec<(usize, String, Vec<String>)> = Vec::new();
        for state in self.regions.drain(..) {""", 1)
# push limit in active loop; insert at start of loop body after end computed? use after let end
s = s.replace("""            let end = state.end.as_ref().and_then(|end| end.find(line));
            if let Some(found) = end {""", """            let end = state.end.as_ref().and_then(|end| end.find(line));
            limits.push((
                end.as_ref().map_or(line.len(), |found| found.end),
                state.group.clone(),
                state.contains.clone(),
            ));
            if let Some(found) = end {""", 1)
# add contains to active state push
s = s.replace("""                                matchgroup: item.matchgroup.clone(),
                            });""", """                                matchgroup: item.matchgroup.clone(),
                                contains: item.contains.clone(),
                            });""", 1)
# helper method before push_next_groups
anchor = """    fn push_next_groups("""
helper = """    fn allowed_in_active(
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
                || item
                    .containedin
                    .iter()
                    .any(|candidate| candidate == group);
        }
        allowed
    }

    fn push_next_groups("""
assert anchor in s
s = s.replace(anchor, helper, 1)
# keyword loop check
s = s.replace("""                            if &line[start..start + word.len()] == word.as_slice()
                                && (start == 0 || !is_word(line[start - 1]))
                                && (start + word.len() == line.len()
                                    || !is_word(line[start + word.len()]))
                            {""", """                            if &line[start..start + word.len()] == word.as_slice()
                                && (start == 0 || !is_word(line[start - 1]))
                                && (start + word.len() == line.len()
                                    || !is_word(line[start + word.len()]))
                                && self.allowed_in_active(item, start, &limits)
                            {""", 1)
# region match start check (before if let Some(end)) for each start
s = s.replace("""                for start in rvim_regex::find_all(pattern, line) {
                    if let Some(end) = end_pattern.find_at(line, start.end, true) {""", """                for start in rvim_regex::find_all(pattern, line) {
                    if !self.allowed_in_active(item, start.start, &limits) {
                        continue;
                    }
                    if let Some(end) = end_pattern.find_at(line, start.end, true) {""", 1)
# normal match loop check
s = s.replace("""                for found in rvim_regex::find_all(pattern, line) {
                    if found.end > found.start {
                        spans.push(Span {""", """                for found in rvim_regex::find_all(pattern, line) {
                    if found.end > found.start && self.allowed_in_active(item, found.start, &limits) {
                        spans.push(Span {""", 1)
p.write_text(s)
print("patched")
