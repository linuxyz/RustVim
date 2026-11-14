use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagEntry {
    pub name: String,
    pub file: PathBuf,
    pub command: String,
}

#[derive(Clone, Debug, Default)]
pub struct TagIndex {
    by_name: HashMap<String, Vec<TagEntry>>,
    pub source: Option<PathBuf>,
}

impl TagIndex {
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read(path) else {
            return Self::default();
        };
        let root = path.parent().unwrap_or(Path::new("."));
        let mut by_name: HashMap<String, Vec<TagEntry>> = HashMap::new();
        for line in text.split(|byte| *byte == b'\n') {
            if line.starts_with(b"!_TAG_") || line.is_empty() {
                continue;
            }
            let text = String::from_utf8_lossy(line);
            let mut fields = text.split('\t');
            let Some(name) = fields.next() else { continue };
            let Some(file) = fields.next() else { continue };
            let command = fields.next().unwrap_or("1").to_string();
            let full = if Path::new(file).is_absolute() {
                PathBuf::from(file)
            } else {
                root.join(file)
            };
            by_name.entry(name.to_string()).or_default().push(TagEntry {
                name: name.to_string(),
                file: full,
                command,
            });
        }
        Self {
            by_name,
            source: Some(path.to_path_buf()),
        }
    }

    pub fn find(&self, name: &str) -> Option<&TagEntry> {
        self.by_name.get(name).and_then(|entries| entries.first())
    }

    pub fn find_all(&self, name: &str) -> &[TagEntry] {
        self.by_name.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn len(&self) -> usize {
        self.by_name.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.by_name.keys().map(String::as_str)
    }

    pub fn all_entries(&self) -> impl Iterator<Item = &TagEntry> {
        self.by_name.values().flatten()
    }
}

pub fn help_tag_for(topic: &str) -> Vec<String> {
    let topic = topic.trim();
    if topic.is_empty() {
        return vec!["help.txt".to_string()];
    }
    let mut out = vec![topic.to_string()];
    out.push(format!("{topic}()"));
    out.push(format!("'{topic}'"));
    out
}

pub fn generate_helptags(directory: &Path) -> Result<usize, std::io::Error> {
    let mut entries: Vec<(String, String, String)> = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("txt") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_string();
        let text = std::fs::read_to_string(&path)?;
        for (line_number, line) in text.lines().enumerate() {
            let mut rest = line;
            while let Some(start) = rest.find('*') {
                let after = &rest[start + 1..];
                let Some(end) = after.find('*') else { break };
                let tag = &after[..end];
                if !tag.is_empty() && !tag.contains(' ') && !tag.contains('\t') {
                    entries.push((
                        tag.to_string(),
                        name.clone(),
                        format!("{}", line_number + 1),
                    ));
                }
                rest = &after[end + 1..];
            }
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let count = entries.len();
    let mut out = String::new();
    for (tag, file, command) in entries {
        out.push_str(&format!("{tag}\t{file}\t{command}\n"));
    }
    std::fs::write(directory.join("tags"), out)?;
    Ok(count)
}
