use std::io;
use std::path::{Path, PathBuf};

use rvim_text::{Snapshot, UndoNode};
use rvim_types::{BufPos, VimString};

use crate::editor::Editor;

const MAGIC: &[u8] = b"RVIMUNDO2\n";
const NO_PARENT: u64 = u64::MAX;

fn encode_name(name: &str) -> String {
    let mut encoded = String::new();
    for ch in name.chars() {
        if ch == '/' || ch == '\\' {
            encoded.push('%');
        } else {
            encoded.push(ch);
        }
    }
    encoded.push_str(".un~");
    encoded
}

pub fn path_for(editor: &Editor, name: &str) -> Option<PathBuf> {
    if !editor.options.get_bool("undofile") || name.is_empty() {
        return None;
    }
    let configured = editor.options.get_string("undodir");
    let directory = configured
        .split(',')
        .map(str::trim)
        .find(|directory| !directory.is_empty())
        .map(PathBuf::from)
        .or_else(|| Path::new(name).parent().map(Path::to_path_buf))?;
    let directory = if directory.is_absolute() {
        directory
    } else {
        rvim_platform::fs::cwd().join(directory)
    };
    Some(directory.join(encode_name(name)))
}

pub fn write(path: &Path, nodes: &[UndoNode], current: usize) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut bytes = MAGIC.to_vec();
    bytes.extend_from_slice(&(nodes.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(current as u32).to_le_bytes());
    for node in nodes {
        let parent = node.parent.map_or(NO_PARENT, |parent| parent as u64);
        bytes.extend_from_slice(&parent.to_le_bytes());
        bytes.extend_from_slice(&(node.children.len() as u32).to_le_bytes());
        for child in &node.children {
            bytes.extend_from_slice(&(*child as u64).to_le_bytes());
        }
        bytes.extend_from_slice(&node.snapshot.cursor.lnum.to_le_bytes());
        bytes.extend_from_slice(&node.snapshot.cursor.col.to_le_bytes());
        bytes.extend_from_slice(&node.snapshot.seq.to_le_bytes());
        bytes.extend_from_slice(&node.time.to_le_bytes());
        bytes.extend_from_slice(&(node.snapshot.lines.len() as u32).to_le_bytes());
        for line in &node.snapshot.lines {
            bytes.extend_from_slice(&(line.len() as u32).to_le_bytes());
            bytes.extend_from_slice(line.as_bytes());
        }
    }
    rvim_platform::fs::write_file_atomic(path, &bytes)
        .map_err(|error| io::Error::other(error.to_string()))
}

pub fn read(path: &Path) -> io::Result<(Vec<UndoNode>, usize)> {
    let bytes = std::fs::read(path)?;
    if !bytes.starts_with(MAGIC) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad undo file"));
    }
    let mut position = MAGIC.len();
    let count = read_u32(&bytes, &mut position)? as usize;
    let current = read_u32(&bytes, &mut position)? as usize;
    let mut nodes = Vec::with_capacity(count);
    for _ in 0..count {
        let parent = read_u64(&bytes, &mut position)?;
        let child_count = read_u32(&bytes, &mut position)? as usize;
        let mut children = Vec::with_capacity(child_count);
        for _ in 0..child_count {
            children.push(read_u64(&bytes, &mut position)? as usize);
        }
        let lnum = read_u32(&bytes, &mut position)?;
        let col = read_u32(&bytes, &mut position)?;
        let seq = read_u64(&bytes, &mut position)?;
        let time = read_u64(&bytes, &mut position)?;
        let line_count = read_u32(&bytes, &mut position)? as usize;
        let mut lines = Vec::with_capacity(line_count);
        for _ in 0..line_count {
            let length = read_u32(&bytes, &mut position)? as usize;
            if position + length > bytes.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "truncated undo file",
                ));
            }
            lines.push(VimString::from(bytes[position..position + length].to_vec()));
            position += length;
        }
        nodes.push(UndoNode {
            id: nodes.len(),
            snapshot: Snapshot {
                lines,
                cursor: BufPos::new(lnum, col),
                seq,
            },
            parent: (parent != NO_PARENT).then_some(parent as usize),
            children,
            time,
        });
    }
    Ok((nodes, current))
}

fn read_u32(bytes: &[u8], position: &mut usize) -> io::Result<u32> {
    if *position + 4 > bytes.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "truncated undo file",
        ));
    }
    let value = u32::from_le_bytes(
        bytes[*position..*position + 4]
            .try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad undo file"))?,
    );
    *position += 4;
    Ok(value)
}

fn read_u64(bytes: &[u8], position: &mut usize) -> io::Result<u64> {
    if *position + 8 > bytes.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "truncated undo file",
        ));
    }
    let value = u64::from_le_bytes(
        bytes[*position..*position + 8]
            .try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad undo file"))?,
    );
    *position += 8;
    Ok(value)
}
