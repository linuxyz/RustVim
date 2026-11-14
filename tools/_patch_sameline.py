import pathlib
p = pathlib.Path("crates/rvim-syntax/src/lib.rs")
s = p.read_text()
old = """        spans.sort_by_key(|span| (span.start, std::cmp::Reverse(span.end)));
        spans"""
new = """        let region_groups: std::collections::HashSet<String> = self
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
        spans"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("patched")
