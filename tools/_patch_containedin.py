import pathlib
p = pathlib.Path("crates/rvim-syntax/src/lib.rs")
s = p.read_text()
s = s.replace("""    contains: Vec<String>,
    containedin: Vec<String>,
}""", """    contains: Vec<String>,
    containedin: Vec<String>,
    contained: bool,
}""", 1)
# keyword constructor and rewrite keyword branch words
old_keyword = """            "keyword" => {
                let mut words = rest.split_whitespace();
                let group = words
                    .next()
                    .ok_or_else(|| "E28: Missing group name".to_string())?;
                let words = words
                    .filter(|word| {
                        !word.contains('=')
                            && !matches!(
                                *word,
                                "contained"
                                    | "display"
                                    | "transparent"
                                    | "skipwhite"
                                    | "skipnl"
                                    | "skipempty"
                            )
                    })
                    .map(|word| word.as_bytes().to_vec())
                    .collect();
                self.items.push(Item {
                    group: group.to_string(),
                    matcher: Matcher::Keyword,
                    words,
                    region_end: None,
                    matchgroup: None,
                    oneline: false,
                    nextgroup: Vec::new(),
                    contains: Vec::new(),
                    containedin: Vec::new(),
                });
            }"""
new_keyword = """            "keyword" => {
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
            }"""
assert old_keyword in s
s = s.replace(old_keyword, new_keyword, 1)
# match constructor add contained
s = s.replace("""                    contains: Vec::new(),
                    containedin: Vec::new(),
                });""", """                    contains: Vec::new(),
                    containedin: Vec::new(),
                    contained: false,
                });""", 1)
# region parse contained
s = s.replace("""                let mut contains = Vec::new();
                let mut containedin = Vec::new();""", """                let mut contains = Vec::new();
                let mut containedin = Vec::new();
                let mut contained = false;""", 1)
s = s.replace("""                    } else if token == "oneline" {
                        oneline = true;""", """                    } else if token == "contained" {
                        contained = true;
                    } else if token == "oneline" {
                        oneline = true;""", 1)
s = s.replace("""                    contains,
                    containedin,
                });""", """                    contains,
                    containedin,
                    contained,
                });""", 1)
# helper allowed_toplevel and use
anchor = """    fn allowed_in_active("""
helper = """    fn allowed_toplevel(
        &self,
        item: &Item,
        limits: &[(usize, String, Vec<String>)],
    ) -> bool {
        !(limits.is_empty() && (item.contained || !item.containedin.is_empty()))
    }

    fn allowed_in_active("""
assert anchor in s
s = s.replace(anchor, helper, 1)
# keyword condition
s = s.replace("""                                && self.allowed_in_active(item, start, &limits)""", """                                && self.allowed_toplevel(item, &limits)
                                && self.allowed_in_active(item, start, &limits)""", 1)
# region start condition
s = s.replace("""                    if !self.allowed_in_active(item, start.start, &limits) {
                        continue;
                    }""", """                    if !self.allowed_toplevel(item, &limits)
                        || !self.allowed_in_active(item, start.start, &limits)
                    {
                        continue;
                    }""", 1)
# normal match condition
s = s.replace("""                    if found.end > found.start && self.allowed_in_active(item, found.start, &limits) {""", """                    if found.end > found.start
                        && self.allowed_toplevel(item, &limits)
                        && self.allowed_in_active(item, found.start, &limits)
                    {""", 1)
p.write_text(s)
print("patched")
