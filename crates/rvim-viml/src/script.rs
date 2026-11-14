//! Legacy Vim script parsing and execution: Ex-style statements, blocks,
//! functions, exceptions and script sourcing.

use std::cell::RefCell;
use std::rc::Rc;

use rvim_types::{ScriptId, VimString};

use crate::builtins;
use crate::error::{codes, Result, VimError};
use crate::eval::{self, Ctx};
use crate::host::VimHost;
use crate::lexer::{AssignOp, Keyword, Punct, Scope, TokenKind};
use crate::ops;
use crate::parser::{Expr, Parser};
use crate::pattern;
use crate::runtime::{CallFrame, FuncLine, FuncParam, UserFunc, VimRuntime};
use crate::value::{dict_ref, lock_container, Value, VimDict};

#[derive(Clone, Debug)]
pub struct Stmt {
    pub line: u32,
    pub silent: bool,
    /// Set by \`silent!\`: errors from this statement are ignored.
    pub force: bool,
    pub kind: StmtKind,
}

/// One piece of a dynamic `:let {expr}name` assignment target.
#[derive(Clone, Debug)]
pub enum DynamicNamePart {
    /// Literal text, including a `:scope` prefix.
    Literal(VimString),
    /// A `{expr}` segment evaluated at assignment time.
    Expr(Expr),
}

#[derive(Clone, Debug)]
pub enum AssignTarget {
    Expr(Expr),
    Destructure {
        names: Vec<Expr>,
        rest: Option<Box<Expr>>,
    },
    /// `:let {expr}name = value` / `let b:{expr}_x = value`.
    DynamicName {
        scope: Option<Scope>,
        parts: Vec<DynamicNamePart>,
    },
}

#[derive(Clone, Debug)]
pub struct FunctionDef {
    pub written_name: VimString,
    pub params: Vec<FuncParam>,
    pub varargs: bool,
    pub dict: bool,
    pub abort: bool,
    pub range: bool,
    pub closure: bool,
    pub bang: bool,
    pub body: Vec<FuncLine>,
    pub line: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EchoKind {
    Echo,
    Echon,
    Echomsg,
    Echoerr,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    Let {
        target: AssignTarget,
        op: AssignOp,
        value: Expr,
        lock: bool,
    },
    Unlet {
        bang: bool,
        targets: Vec<Expr>,
    },
    Lock {
        lock: bool,
        depth: i32,
        targets: Vec<Expr>,
    },
    If {
        branches: Vec<(Expr, Vec<Stmt>)>,
        else_body: Option<Vec<Stmt>>,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    Try {
        body: Vec<Stmt>,
        catches: Vec<(Option<Vec<u8>>, Vec<Stmt>)>,
        finally_body: Option<Vec<Stmt>>,
    },
    For {
        target: AssignTarget,
        iterable: Expr,
        body: Vec<Stmt>,
    },
    Function(FunctionDef),
    Return(Option<Expr>),
    Call(Expr),
    Echo {
        kind: EchoKind,
        expressions: Vec<Expr>,
    },
    Execute {
        expressions: Vec<Expr>,
    },
    Throw(Expr),
    Break,
    Continue,
    Finish,
    Source(VimString),
    ScriptEncoding(String),
    ScriptVersion(i64),
    Delfunction(VimString),
    Eval(Expr),
    Ex(String),
}

#[derive(Clone, Debug)]
pub enum Flow {
    Normal,
    Break,
    Continue,
    Return(Value),
    Finish,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cmd {
    Let,
    Const,
    Unlet,
    Lockvar,
    Unlockvar,
    If,
    Elseif,
    Else,
    Endif,
    While,
    Endwhile,
    For,
    Endfor,
    Function,
    Endfunction,
    Return,
    Call,
    Echo,
    Echon,
    Echomsg,
    Echoerr,
    Execute,
    Try,
    Catch,
    Finally,
    Endtry,
    Throw,
    Break,
    Continue,
    Source,
    Finish,
    ScriptEncoding,
    ScriptVersion,
    Delfunction,
    Eval,
}

impl Cmd {
    fn name(self) -> &'static str {
        match self {
            Cmd::Let => "let",
            Cmd::Const => "const",
            Cmd::Unlet => "unlet",
            Cmd::Lockvar => "lockvar",
            Cmd::Unlockvar => "unlockvar",
            Cmd::If => "if",
            Cmd::Elseif => "elseif",
            Cmd::Else => "else",
            Cmd::Endif => "endif",
            Cmd::While => "while",
            Cmd::Endwhile => "endwhile",
            Cmd::For => "for",
            Cmd::Endfor => "endfor",
            Cmd::Function => "function",
            Cmd::Endfunction => "endfunction",
            Cmd::Return => "return",
            Cmd::Call => "call",
            Cmd::Echo => "echo",
            Cmd::Echon => "echon",
            Cmd::Echomsg => "echomsg",
            Cmd::Echoerr => "echoerr",
            Cmd::Execute => "execute",
            Cmd::Try => "try",
            Cmd::Catch => "catch",
            Cmd::Finally => "finally",
            Cmd::Endtry => "endtry",
            Cmd::Throw => "throw",
            Cmd::Break => "break",
            Cmd::Continue => "continue",
            Cmd::Source => "source",
            Cmd::Finish => "finish",
            Cmd::ScriptEncoding => "scriptencoding",
            Cmd::ScriptVersion => "scriptversion",
            Cmd::Delfunction => "delfunction",
            Cmd::Eval => "eval",
        }
    }
}

// Ordered like Vim's ex_cmds.h command table.  Ambiguous abbreviations are
// resolved to the first matching entry, so ":end" means ":endif" (endif comes
// before endwhile), which runtime syntax files rely on.
const COMMANDS: &[(&[u8], usize, Cmd)] = &[
    (b"break", 4, Cmd::Break),
    (b"call", 3, Cmd::Call),
    (b"catch", 3, Cmd::Catch),
    (b"continue", 3, Cmd::Continue),
    (b"const", 4, Cmd::Const),
    (b"delfunction", 4, Cmd::Delfunction),
    (b"echo", 2, Cmd::Echo),
    (b"echoerr", 5, Cmd::Echoerr),
    (b"echomsg", 5, Cmd::Echomsg),
    (b"echon", 5, Cmd::Echon),
    (b"else", 2, Cmd::Else),
    (b"elseif", 5, Cmd::Elseif),
    (b"endif", 4, Cmd::Endif),
    (b"endfunction", 4, Cmd::Endfunction),
    (b"endfor", 5, Cmd::Endfor),
    (b"endtry", 4, Cmd::Endtry),
    (b"endwhile", 3, Cmd::Endwhile),
    (b"eval", 3, Cmd::Eval),
    (b"execute", 3, Cmd::Execute),
    (b"finally", 4, Cmd::Finally),
    (b"finish", 4, Cmd::Finish),
    (b"for", 3, Cmd::For),
    (b"function", 2, Cmd::Function),
    (b"if", 2, Cmd::If),
    (b"let", 3, Cmd::Let),
    (b"lockvar", 5, Cmd::Lockvar),
    (b"return", 3, Cmd::Return),
    (b"scriptencoding", 7, Cmd::ScriptEncoding),
    (b"scriptversion", 7, Cmd::ScriptVersion),
    (b"source", 2, Cmd::Source),
    (b"throw", 2, Cmd::Throw),
    (b"try", 3, Cmd::Try),
    (b"unlet", 3, Cmd::Unlet),
    (b"unlockvar", 4, Cmd::Unlockvar),
    (b"while", 2, Cmd::While),
];

fn resolve_command(word: &[u8]) -> Option<Cmd> {
    for (full, _, command) in COMMANDS {
        if word == *full {
            return Some(*command);
        }
    }
    // Vim does not use the documented minimum abbreviations in the lookup;
    // any prefix resolves to the first table entry.  Single letters are Ex
    // commands of their own (":e" is :edit), so leave them alone.
    if word.len() < 2 {
        return None;
    }
    for (full, _, command) in COMMANDS {
        if full.starts_with(word) {
            return Some(*command);
        }
    }
    None
}

fn is_modifier(word: &[u8]) -> bool {
    const MODIFIERS: &[(&[u8], usize)] = &[
        (b"silent", 3),
        (b"verbose", 4),
        (b"noautocmd", 3),
        (b"keepalt", 5),
        (b"keepjumps", 5),
        (b"keepmarks", 5),
        (b"lockmarks", 5),
        (b"keeppatterns", 5),
        (b"sandbox", 5),
        (b"hide", 3),
        (b"confirm", 4),
        (b"noswapfile", 3),
        (b"vertical", 4),
        (b"aboveleft", 3),
        (b"belowright", 3),
        (b"leftabove", 5),
        (b"rightbelow", 6),
        (b"topleft", 4),
        (b"botright", 4),
    ];
    MODIFIERS
        .iter()
        .any(|(name, minimum)| word.len() >= *minimum && name.starts_with(word))
}

fn trim_start(bytes: &[u8]) -> &[u8] {
    let mut index = 0;
    while index < bytes.len() && matches!(bytes[index], b' ' | b'\t' | b'\r') {
        index += 1;
    }
    &bytes[index..]
}

fn trim_end(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && matches!(bytes[end - 1], b' ' | b'\t' | b'\r') {
        end -= 1;
    }
    &bytes[..end]
}

fn trim(bytes: &[u8]) -> &[u8] {
    trim_end(trim_start(bytes))
}

#[derive(Clone, Debug)]
struct SourceLine {
    line: u32,
    text: Vec<u8>,
}

#[derive(Clone, Debug)]
struct Terminator {
    command: Cmd,
    line: u32,
    rest: Vec<u8>,
    silent: bool,
}

#[derive(Clone, Debug)]
struct Head {
    command: Option<Cmd>,
    rest: Vec<u8>,
    bang: bool,
    silent: bool,
    force: bool,
    text: Vec<u8>,
}

fn read_word(bytes: &[u8]) -> (&[u8], &[u8]) {
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
        index += 1;
    }
    (&bytes[..index], &bytes[index..])
}

fn parse_head(line: &[u8]) -> Head {
    let mut rest = trim_start(line);
    while rest.first() == Some(&b':') {
        rest = trim_start(&rest[1..]);
    }
    let original = rest.to_vec();
    let mut silent = false;
    let mut force = false;
    loop {
        let trimmed = trim_start(rest);
        let (word, after) = read_word(trimmed);
        if word.is_empty() {
            return Head {
                command: None,
                rest: Vec::new(),
                bang: false,
                silent,
                force,
                text: original,
            };
        }
        let (after, bang) = match after.first() {
            Some(b'!') => (&after[1..], true),
            _ => (after, false),
        };
        if is_modifier(word) {
            let is_silent = word.len() >= 3 && b"silent".starts_with(word);
            silent |= is_silent;
            force |= is_silent && bang;
            let mut after = after;
            if word == b"verbose" {
                while after.first().is_some_and(|byte| byte.is_ascii_digit()) {
                    after = &after[1..];
                }
            }
            rest = after;
            continue;
        }
        if let Some(command) = resolve_command(word) {
            return Head {
                command: Some(command),
                rest: trim_start(after).to_vec(),
                bang,
                silent,
                force,
                text: original,
            };
        }
        return Head {
            command: None,
            rest: original.clone(),
            bang: false,
            silent: false,
            force: false,
            text: original,
        };
    }
}

fn is_vim9_command(word: &[u8]) -> bool {
    const VIM9: &[&[u8]] = &[
        b"def",
        b"enddef",
        b"class",
        b"endclass",
        b"interface",
        b"endinterface",
        b"enum",
        b"endenum",
        b"type",
        b"import",
        b"export",
    ];
    VIM9.contains(&word)
}

fn is_blank_or_comment(line: &[u8]) -> bool {
    let trimmed = trim_start(line);
    trimmed.is_empty() || trimmed[0] == b'"'
}

fn preprocess_source(source: &[u8]) -> Vec<SourceLine> {
    let source = source
        .strip_prefix(&[0xef, 0xbb, 0xbf][..])
        .unwrap_or(source);
    let mut raw: Vec<SourceLine> = Vec::new();
    let mut start = 0;
    let mut number = 1u32;
    for (index, byte) in source.iter().enumerate() {
        if *byte == b'\n' {
            let mut text = source[start..index].to_vec();
            if text.last() == Some(&b'\r') {
                text.pop();
            }
            raw.push(SourceLine { line: number, text });
            number += 1;
            start = index + 1;
        }
    }
    if start < source.len() {
        let mut text = source[start..].to_vec();
        if text.last() == Some(&b'\r') {
            text.pop();
        }
        raw.push(SourceLine { line: number, text });
    }

    let mut joined: Vec<SourceLine> = Vec::new();
    let mut raw = raw.into_iter().peekable();
    while let Some(line) = raw.next() {
        if let Some(start) = heredoc_start(&line.text) {
            let mut content: Vec<Vec<u8>> = Vec::new();
            let mut terminator = false;
            for next in raw.by_ref() {
                if trim_start(&next.text) == start.marker {
                    terminator = true;
                    break;
                }
                content.push(next.text);
            }
            if terminator {
                joined.push(SourceLine {
                    line: line.line,
                    text: heredoc_assignment(&line.text[..start.prefix_len], &content, start.trim),
                });
                continue;
            }
            // No terminator: fall back to treating the line normally.
            joined.push(line);
            let rest: Vec<SourceLine> = raw.collect();
            joined.extend(rest);
            break;
        }
        let trimmed = trim_start(&line.text);
        if trimmed.first() == Some(&b'\\') && !joined.is_empty() {
            let continuation = trimmed[1..].to_vec();
            if let Some(previous) = joined.last_mut() {
                previous.text.extend_from_slice(&continuation);
            }
            continue;
        }
        joined.push(line);
    }

    let mut output = Vec::new();
    for line in joined {
        // Commands that take raw text (":syntax" patterns, ":normal", ...)
        // treat '"' as part of their argument; only other commands can start a
        // comment with an unmatched double quote.
        let stripped = if raw_text_command(command_word(&line.text)) {
            line.text.clone()
        } else {
            strip_unterminated_comment(&line.text).to_vec()
        };
        let trimmed = trim_start(&stripped);
        if trimmed.is_empty() || trimmed[0] == b'"' {
            output.push(SourceLine {
                line: line.line,
                text: stripped,
            });
            continue;
        }
        for (index, piece) in split_commands(&stripped).into_iter().enumerate() {
            let text = if index == 0 {
                piece
            } else {
                trim_start(&piece).to_vec()
            };
            output.push(SourceLine {
                line: line.line,
                text,
            });
        }
    }
    output
}

/// A double quote with no matching quote is a comment in command position.
/// This removes it before tokenizing the rest of the command.
fn strip_unterminated_comment(text: &[u8]) -> &[u8] {
    let mut index = 0;
    let mut single = false;
    while index < text.len() {
        if single {
            if text[index] == b'\'' {
                if text.get(index + 1) == Some(&b'\'') {
                    index += 2;
                    continue;
                }
                single = false;
            }
            index += 1;
            continue;
        }
        match text[index] {
            b'\'' => {
                single = true;
                index += 1;
            }
            b'"' => {
                let mut scan = index + 1;
                let mut escaped = false;
                let mut closed = false;
                while scan < text.len() {
                    if escaped {
                        escaped = false;
                    } else if text[scan] == b'\\' {
                        escaped = true;
                    } else if text[scan] == b'"' {
                        closed = true;
                        break;
                    }
                    scan += 1;
                }
                if !closed {
                    return &text[..index];
                }
                index = scan + 1;
            }
            _ => index += 1,
        }
    }
    text
}

/// A parsed `let var =<< [trim] [eval] MARKER` heredoc opening.
struct HeredocStart {
    /// Byte offset of the `=<<` token; everything before it is kept.
    prefix_len: usize,
    /// End marker line (compared after leading whitespace is removed).
    marker: Vec<u8>,
    /// Remove leading whitespace and blank lines at both ends.
    trim: bool,
}

fn heredoc_start(text: &[u8]) -> Option<HeredocStart> {
    let trimmed = trim_start(text);
    let assignment = trimmed.starts_with(b"let ")
        || trimmed.starts_with(b"let	")
        || trimmed.starts_with(b"var ")
        || trimmed.starts_with(b"const ");
    if !assignment {
        return None;
    }
    let position = find_bytes(text, b"=<<")?;
    let mut index = position + 3;
    let mut trim = false;
    let mut marker: Option<Vec<u8>> = None;
    loop {
        while index < text.len() && (text[index] == b' ' || text[index] == b'\t') {
            index += 1;
        }
        if index >= text.len() || text[index] == b'"' {
            break;
        }
        let begin = index;
        while index < text.len() && (text[index].is_ascii_alphanumeric() || text[index] == b'_') {
            index += 1;
        }
        if begin == index {
            return None;
        }
        let word = &text[begin..index];
        if word == b"trim" {
            trim = true;
        } else if word == b"eval" {
            // An eval: heredoc needs expression evaluation, which is not
            // available while preprocessing; keep such lines untouched.
            return None;
        } else if marker.is_none() {
            marker = Some(word.to_vec());
        } else {
            return None;
        }
    }
    let marker = marker?;
    Some(HeredocStart {
        prefix_len: position,
        marker,
        trim,
    })
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn heredoc_assignment(prefix: &[u8], lines: &[Vec<u8>], trim: bool) -> Vec<u8> {
    let mut content: Vec<&[u8]> = if trim {
        lines.iter().map(|line| trim_start(line)).collect()
    } else {
        lines.iter().map(|line| line.as_slice()).collect()
    };
    if trim {
        while content.first().is_some_and(|line| line.is_empty()) {
            content.remove(0);
        }
        while content.last().is_some_and(|line| line.is_empty()) {
            content.pop();
        }
    }
    let mut out = prefix.to_vec();
    out.extend_from_slice(b"= [");
    for (index, line) in content.iter().enumerate() {
        if index > 0 {
            out.push(b',');
        }
        out.push(b'\'');
        for &byte in *line {
            if byte == b'\'' {
                out.extend_from_slice(b"''");
            } else {
                out.push(byte);
            }
        }
        out.push(b'\'');
    }
    out.push(b']');
    out
}

/// The first word of an Ex command line, ignoring leading whitespace and ':'.
fn command_word(line: &[u8]) -> &[u8] {
    let mut rest = trim_start(line);
    while rest.first() == Some(&b':') {
        rest = &rest[1..];
    }
    let (word, _) = read_word(trim_start(rest));
    word
}

/// Commands that consume their argument as raw text.  A '|' inside the
/// argument does not start a new command and a '"' does not start a comment.
/// Recognise a `:runtime` / `:runtime!` command line.
fn parse_runtime_invocation(command: &str) -> Option<(bool, String)> {
    let text = trim_start(command.as_bytes());
    let text = match text.strip_prefix(b":") {
        Some(rest) => trim_start(rest),
        None => text,
    };
    let (word, rest) = read_word(text);
    if word != b"runtime" {
        return None;
    }
    let rest = trim_start(rest);
    let (all, rest) = match rest.first() {
        Some(b'!') => (true, trim_start(&rest[1..])),
        _ => (false, rest),
    };
    let pattern = String::from_utf8_lossy(rest).trim().to_string();
    let pattern = pattern.trim_end_matches('!').to_string();
    if pattern.is_empty() {
        return None;
    }
    Some((all, pattern))
}

fn raw_text_command(word: &[u8]) -> bool {
    matches!(
        word,
        b"normal"
            | b"norm"
            | b"global"
            | b"g"
            | b"vglobal"
            | b"v"
            | b"autocmd"
            | b"au"
            | b"command"
            | b"com"
    ) || (word.len() >= 2 && b"syntax".starts_with(word))
        // User defined commands start with an uppercase letter.  Without a
        // "-bar" attribute their argument may contain '|' (runtime syntax
        // files rely on this for ':VimFolda').
        || word.first().is_some_and(|byte| byte.is_ascii_uppercase())
}

fn split_commands(line: &[u8]) -> Vec<Vec<u8>> {
    let trimmed = trim_start(line);
    let (word, _) = read_word(trimmed);
    // Commands whose argument is raw text: a '|' in the argument belongs to
    // the command, not to the next command.  Vim marks those with the lack of
    // the EX_TRLBAR flag.  ":syntax" patterns in particular contain bare
    // '|' alternations and '\|' escapes.
    if raw_text_command(word) {
        return vec![line.to_vec()];
    }
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut index = 0;
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    while index < line.len() {
        let byte = line[index];
        if double {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                double = false;
            }
            index += 1;
            continue;
        }
        if single {
            if byte == b'\'' {
                if line.get(index + 1) == Some(&b'\'') {
                    index += 1;
                } else {
                    single = false;
                }
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' => double = true,
            b'\'' => single = true,
            b'|' => {
                let previous = index.checked_sub(1).and_then(|position| line.get(position));
                let next = line.get(index + 1);
                // A backslash-escaped bar is part of the argument (e.g. a
                // regex "\|"); Vim does not treat it as a command separator.
                if previous == Some(&b'\\') {
                    index += 1;
                    continue;
                }
                if previous != Some(&b'|') && next != Some(&b'|') {
                    pieces.push(line[start..index].to_vec());
                    start = index + 1;
                }
            }
            _ => {}
        }
        index += 1;
    }
    pieces.push(line[start..].to_vec());
    pieces
}

fn annotate_line(error: VimError, line: u32) -> VimError {
    error.with_throwpoint(format!("line {line}"))
}

fn annotate_result<T>(result: Result<T>, line: u32) -> Result<T> {
    result.map_err(|error| annotate_line(error, line))
}

fn missing_terminator(code: u32, message: &str) -> VimError {
    VimError::new(code, message)
}

fn unexpected_terminator(command: Cmd, line: u32) -> VimError {
    let code = match command {
        Cmd::Elseif | Cmd::Else | Cmd::Endif => 582,
        Cmd::Endwhile => 583,
        Cmd::Endfor => 581,
        Cmd::Catch | Cmd::Finally | Cmd::Endtry => 584,
        Cmd::Endfunction => 193,
        _ => 580,
    };
    VimError::new(
        code,
        format!(":{} without matching block (line {line})", command.name()),
    )
}

struct BlockParser {
    lines: Vec<SourceLine>,
    index: usize,
}

impl BlockParser {
    fn parse_all(mut self) -> Result<Vec<Stmt>> {
        let (statements, terminator) = self.parse_statements(&[])?;
        if let Some(terminator) = terminator {
            return Err(unexpected_terminator(terminator.command, terminator.line));
        }
        Ok(statements)
    }

    fn parse_statements(&mut self, terminators: &[Cmd]) -> Result<(Vec<Stmt>, Option<Terminator>)> {
        let mut statements = Vec::new();
        while self.index < self.lines.len() {
            let line = self.lines[self.index].clone();
            if is_blank_or_comment(&line.text) {
                self.index += 1;
                continue;
            }
            let head = parse_head(&line.text);
            let Some(command) = head.command else {
                let trimmed = trim_start(&head.text);
                let (word, _) = read_word(trimmed);
                if trimmed.starts_with(b"vim9script") || is_vim9_command(word) {
                    // Vim9 script is deliberately out of scope (like the
                    // terminal emulator); report a coded error so tooling and
                    // the compatibility matrix do not silently treat the file
                    // as loaded.
                    return Err(VimError::new(
                        319,
                        format!(
                            "Sorry, the command is not available in this version: {} (Vim9 script is not supported)",
                            String::from_utf8_lossy(word)
                        ),
                    ));
                }
                self.index += 1;
                statements.push(Stmt {
                    line: line.line,
                    silent: head.silent,
                    force: head.force,
                    kind: StmtKind::Ex(String::from_utf8_lossy(&head.text).into_owned()),
                });
                continue;
            };
            if terminators.contains(&command) {
                self.index += 1;
                return Ok((
                    statements,
                    Some(Terminator {
                        command,
                        line: line.line,
                        rest: head.rest,
                        silent: head.silent,
                    }),
                ));
            }
            if matches!(
                command,
                Cmd::Elseif
                    | Cmd::Else
                    | Cmd::Endif
                    | Cmd::Endwhile
                    | Cmd::Endfor
                    | Cmd::Catch
                    | Cmd::Finally
                    | Cmd::Endtry
                    | Cmd::Endfunction
            ) {
                return Err(unexpected_terminator(command, line.line));
            }
            match command {
                Cmd::If => {
                    annotate_result(self.parse_if(&head, line.line, &mut statements), line.line)?
                }
                Cmd::While => annotate_result(
                    self.parse_while(&head, line.line, &mut statements),
                    line.line,
                )?,
                Cmd::For => {
                    annotate_result(self.parse_for(&head, line.line, &mut statements), line.line)?
                }
                Cmd::Try => {
                    annotate_result(self.parse_try(&head, line.line, &mut statements), line.line)?
                }
                Cmd::Function => annotate_result(
                    self.parse_function(&head, line.line, &mut statements),
                    line.line,
                )?,
                _ => {
                    self.index += 1;
                    let kind = parse_simple(command, &head, line.line)
                        .map_err(|error| annotate_line(error, line.line))?;
                    statements.push(Stmt {
                        line: line.line,
                        silent: head.silent,
                        force: head.force,
                        kind,
                    });
                }
            }
        }
        Ok((statements, None))
    }

    fn parse_if(&mut self, head: &Head, line: u32, statements: &mut Vec<Stmt>) -> Result<()> {
        let condition = parse_one_expression(&head.rest, line)?;
        self.index += 1;
        let (body, terminator) = self.parse_statements(&[Cmd::Elseif, Cmd::Else, Cmd::Endif])?;
        let mut terminator = terminator.ok_or_else(|| missing_terminator(171, "Missing :endif"))?;
        let mut branches = vec![(condition, body)];
        let mut else_body = None;
        loop {
            match terminator.command {
                Cmd::Elseif => {
                    let condition = parse_one_expression(&terminator.rest, terminator.line)
                        .map_err(|error| annotate_line(error, terminator.line))?;
                    let (body, next) =
                        self.parse_statements(&[Cmd::Elseif, Cmd::Else, Cmd::Endif])?;
                    branches.push((condition, body));
                    terminator = next.ok_or_else(|| missing_terminator(171, "Missing :endif"))?;
                }
                Cmd::Else => {
                    let (body, next) = self.parse_statements(&[Cmd::Endif])?;
                    else_body = Some(body);
                    terminator = next.ok_or_else(|| missing_terminator(171, "Missing :endif"))?;
                    if terminator.command != Cmd::Endif {
                        return Err(unexpected_terminator(terminator.command, terminator.line));
                    }
                    break;
                }
                Cmd::Endif => break,
                _ => return Err(unexpected_terminator(terminator.command, terminator.line)),
            }
        }
        let _ = terminator.silent;
        statements.push(Stmt {
            line,
            silent: head.silent,
            force: head.force,
            kind: StmtKind::If {
                branches,
                else_body,
            },
        });
        Ok(())
    }

    fn parse_while(&mut self, head: &Head, line: u32, statements: &mut Vec<Stmt>) -> Result<()> {
        let condition = parse_one_expression(&head.rest, line)?;
        self.index += 1;
        let (body, terminator) = self.parse_statements(&[Cmd::Endwhile])?;
        let terminator = terminator.ok_or_else(|| missing_terminator(172, "Missing :endwhile"))?;
        if terminator.command != Cmd::Endwhile {
            return Err(unexpected_terminator(terminator.command, terminator.line));
        }
        statements.push(Stmt {
            line,
            silent: head.silent,
            force: head.force,
            kind: StmtKind::While { condition, body },
        });
        Ok(())
    }

    fn parse_for(&mut self, head: &Head, line: u32, statements: &mut Vec<Stmt>) -> Result<()> {
        let input = trim_start(&head.rest);
        if input.is_empty() {
            return Err(codes::e15("missing :for variable"));
        }
        let mut parser = Parser::new(input)?;
        let target = if parser.check_punct(Punct::LBracket) {
            parse_destructure(&mut parser)?
        } else {
            AssignTarget::Expr(parser.parse_expression()?)
        };
        if !parser.eat_keyword(Keyword::In) {
            return Err(codes::e15("missing 'in' in :for"));
        }
        let iterable = parser.parse_expression()?;
        finish_expression(&parser, input)?;
        self.index += 1;
        let (body, terminator) = self.parse_statements(&[Cmd::Endfor])?;
        let terminator = terminator.ok_or_else(|| missing_terminator(173, "Missing :endfor"))?;
        if terminator.command != Cmd::Endfor {
            return Err(unexpected_terminator(terminator.command, terminator.line));
        }
        statements.push(Stmt {
            line,
            silent: head.silent,
            force: head.force,
            kind: StmtKind::For {
                target,
                iterable,
                body,
            },
        });
        Ok(())
    }

    fn parse_try(&mut self, head: &Head, line: u32, statements: &mut Vec<Stmt>) -> Result<()> {
        self.index += 1;
        let (body, terminator) = self.parse_statements(&[Cmd::Catch, Cmd::Finally, Cmd::Endtry])?;
        let mut terminator =
            terminator.ok_or_else(|| missing_terminator(600, "Missing :endtry"))?;
        let mut catches: Vec<(Option<Vec<u8>>, Vec<Stmt>)> = Vec::new();
        let mut finally_body = None;
        loop {
            match terminator.command {
                Cmd::Catch => {
                    let catch_pattern = parse_catch_pattern(&terminator.rest)
                        .map_err(|error| annotate_line(error, terminator.line))?;
                    let (catch_body, next) =
                        self.parse_statements(&[Cmd::Catch, Cmd::Finally, Cmd::Endtry])?;
                    catches.push((catch_pattern, catch_body));
                    terminator = next.ok_or_else(|| missing_terminator(600, "Missing :endtry"))?;
                }
                Cmd::Finally => {
                    let (final_statements, next) = self.parse_statements(&[Cmd::Endtry])?;
                    finally_body = Some(final_statements);
                    terminator = next.ok_or_else(|| missing_terminator(600, "Missing :endtry"))?;
                    if terminator.command != Cmd::Endtry {
                        return Err(unexpected_terminator(terminator.command, terminator.line));
                    }
                    break;
                }
                Cmd::Endtry => break,
                _ => return Err(unexpected_terminator(terminator.command, terminator.line)),
            }
        }
        statements.push(Stmt {
            line,
            silent: head.silent,
            force: head.force,
            kind: StmtKind::Try {
                body,
                catches,
                finally_body,
            },
        });
        Ok(())
    }

    fn parse_function(&mut self, head: &Head, line: u32, statements: &mut Vec<Stmt>) -> Result<()> {
        let mut definition = parse_function_header(&head.rest, line, head.bang)?;
        self.index += 1;
        let mut depth = 1;
        let mut body = Vec::new();
        while self.index < self.lines.len() {
            let current = &self.lines[self.index];
            let inner = parse_head(&current.text);
            match inner.command {
                Some(Cmd::Function) => depth += 1,
                Some(Cmd::Endfunction) => {
                    depth -= 1;
                    if depth == 0 {
                        self.index += 1;
                        break;
                    }
                }
                _ => {}
            }
            body.push(FuncLine {
                line: current.line,
                text: current.text.clone(),
            });
            self.index += 1;
        }
        if depth != 0 {
            return Err(missing_terminator(170, "Missing :endfunction"));
        }
        definition.body = body;
        statements.push(Stmt {
            line,
            silent: head.silent,
            force: head.force,
            kind: StmtKind::Function(definition),
        });
        Ok(())
    }
}

/// Parse a `:let` target, supporting Vim's dynamic `{expr}` name segments.
fn parse_let_target(parser: &mut Parser) -> Result<AssignTarget> {
    let mut scope = None;
    let mut parts: Vec<DynamicNamePart> = Vec::new();
    let mut dynamic = false;
    if let TokenKind::ScopedVar(scoped_scope, name) = parser.current_kind().clone() {
        if matches!(parser.peek_kind(1), Some(TokenKind::Punct(Punct::LBrace))) {
            parser.bump();
            scope = Some(scoped_scope);
            if !name.is_empty() {
                parts.push(DynamicNamePart::Literal(name));
            }
            dynamic = true;
        }
    } else if matches!(parser.current_kind(), TokenKind::Punct(Punct::LBrace)) {
        dynamic = true;
    }
    if !dynamic {
        return Ok(AssignTarget::Expr(parser.parse_expression()?));
    }
    loop {
        match parser.current_kind().clone() {
            TokenKind::Punct(Punct::LBrace) => {
                parser.bump();
                let expression = parser.parse_expression()?;
                parser.expect_punct(Punct::RBrace)?;
                parts.push(DynamicNamePart::Expr(expression));
            }
            TokenKind::Ident(name) => {
                parser.bump();
                parts.push(DynamicNamePart::Literal(name));
            }
            _ => break,
        }
    }
    if parts.is_empty() {
        return Err(codes::e15("empty variable name in :let"));
    }
    Ok(AssignTarget::DynamicName { scope, parts })
}

fn parse_simple(command: Cmd, head: &Head, line: u32) -> Result<StmtKind> {
    match command {
        Cmd::Let | Cmd::Const => {
            let input = trim_start(&head.rest);
            if input.is_empty() {
                return Err(codes::e15("missing :let argument"));
            }
            let mut parser = Parser::new(input)?;
            let target = if parser.check_punct(Punct::LBracket) {
                parse_destructure(&mut parser)?
            } else {
                parse_let_target(&mut parser)?
            };
            let op = parser
                .take_assignment_op()
                .ok_or_else(|| codes::e15("missing assignment operator in :let"))?;
            let value = parser.parse_expression()?;
            finish_expression(&parser, input)?;
            Ok(StmtKind::Let {
                target,
                op,
                value,
                lock: command == Cmd::Const,
            })
        }
        Cmd::Unlet => {
            let input = trim_start(&head.rest);
            if input.is_empty() {
                return Err(codes::e15("missing :unlet argument"));
            }
            let mut parser = Parser::new(input)?;
            let mut targets = Vec::new();
            while !parser.at_eof() {
                targets.push(parser.parse_expression()?);
                if parser.eat_punct(Punct::Comma) && parser.at_eof() {
                    break;
                }
            }
            Ok(StmtKind::Unlet {
                bang: head.bang,
                targets,
            })
        }
        Cmd::Lockvar | Cmd::Unlockvar => {
            let input = trim_start(&head.rest);
            if input.is_empty() {
                return Err(codes::e15("missing :lockvar argument"));
            }
            let mut parser = Parser::new(input)?;
            let depth = match parser.current_kind() {
                TokenKind::Number(number) => {
                    let depth = *number as i32;
                    parser.bump();
                    depth
                }
                _ => 1,
            };
            let mut targets = Vec::new();
            while !parser.at_eof() {
                targets.push(parser.parse_expression()?);
                if parser.eat_punct(Punct::Comma) && parser.at_eof() {
                    break;
                }
            }
            Ok(StmtKind::Lock {
                lock: command == Cmd::Lockvar,
                depth,
                targets,
            })
        }
        Cmd::Return => {
            let input = trim_start(&head.rest);
            if input.is_empty() || input[0] == b'"' {
                Ok(StmtKind::Return(None))
            } else {
                Ok(StmtKind::Return(Some(parse_one_expression(input, line)?)))
            }
        }
        Cmd::Call => Ok(StmtKind::Call(parse_one_expression(&head.rest, line)?)),
        Cmd::Echo | Cmd::Echon | Cmd::Echomsg | Cmd::Echoerr => {
            let kind = match command {
                Cmd::Echo => EchoKind::Echo,
                Cmd::Echon => EchoKind::Echon,
                Cmd::Echomsg => EchoKind::Echomsg,
                _ => EchoKind::Echoerr,
            };
            Ok(StmtKind::Echo {
                kind,
                expressions: parse_expression_list(&head.rest)?,
            })
        }
        Cmd::Execute => Ok(StmtKind::Execute {
            expressions: parse_expression_list(&head.rest)?,
        }),
        Cmd::Throw => Ok(StmtKind::Throw(parse_one_expression(&head.rest, line)?)),
        Cmd::Break => Ok(StmtKind::Break),
        Cmd::Continue => Ok(StmtKind::Continue),
        Cmd::Finish => Ok(StmtKind::Finish),
        Cmd::Source => {
            let path = raw_argument(&head.rest);
            if path.is_empty() {
                return Err(codes::e15("missing file name for :source"));
            }
            Ok(StmtKind::Source(VimString::from(path)))
        }
        Cmd::ScriptEncoding => {
            let encoding = raw_argument(&head.rest);
            if encoding.is_empty() {
                return Err(codes::e15("missing encoding for :scriptencoding"));
            }
            Ok(StmtKind::ScriptEncoding(encoding))
        }
        Cmd::ScriptVersion => {
            let value = raw_argument(&head.rest);
            let version = value
                .parse::<i64>()
                .map_err(|_| codes::e15("invalid :scriptversion"))?;
            Ok(StmtKind::ScriptVersion(version))
        }
        Cmd::Delfunction => {
            let input = trim_start(&head.rest);
            let mut parser = Parser::new(input)?;
            let name = parse_function_name(&mut parser)?;
            finish_expression(&parser, input)?;
            Ok(StmtKind::Delfunction(name))
        }
        Cmd::Eval => Ok(StmtKind::Eval(parse_one_expression(&head.rest, line)?)),
        _ => Err(codes::e15("unsupported statement")),
    }
}

fn parse_destructure(parser: &mut Parser) -> Result<AssignTarget> {
    parser.expect_punct(Punct::LBracket)?;
    let mut names = Vec::new();
    let mut rest = None;
    if parser.eat_punct(Punct::RBracket) {
        return Ok(AssignTarget::Destructure { names, rest });
    }
    loop {
        names.push(parser.parse_expression()?);
        if parser.eat_punct(Punct::Comma) {
            if parser.eat_punct(Punct::RBracket) {
                break;
            }
            continue;
        }
        if parser.eat_punct(Punct::Semicolon) {
            rest = Some(Box::new(parser.parse_expression()?));
        }
        parser.expect_punct(Punct::RBracket)?;
        break;
    }
    Ok(AssignTarget::Destructure { names, rest })
}

fn parse_function_name(parser: &mut Parser) -> Result<VimString> {
    let mut name = match parser.bump().kind {
        TokenKind::Ident(name) => {
            if name.as_bytes().eq_ignore_ascii_case(b"<SID>") {
                match parser.bump().kind {
                    TokenKind::Ident(rest) => {
                        let mut full = VimString::from("<SID>");
                        full.extend_from_slice(rest.as_bytes());
                        full
                    }
                    _ => return Err(codes::e15("missing function name after <SID>")),
                }
            } else {
                name
            }
        }
        TokenKind::ScopedVar(Scope::Script, name) => {
            VimString::from(format!("s:{}", name.to_string_lossy()))
        }
        _ => return Err(codes::e15("invalid function name")),
    };
    // Vim allows dotted names such as "s:ff.LeftConstant(...)" (used by
    // runtime/syntax/java.vim).
    while parser.eat_punct(Punct::Dot) {
        let TokenKind::Ident(part) = parser.bump().kind else {
            return Err(codes::e15("invalid function name"));
        };
        name.extend_from_slice(b".");
        name.extend_from_slice(part.as_bytes());
    }
    Ok(name)
}

fn parse_function_header(rest: &[u8], line: u32, bang: bool) -> Result<FunctionDef> {
    let input = trim_start(rest);
    let mut parser = Parser::new(input)?;
    let written_name = parse_function_name(&mut parser)?;
    parser.expect_punct(Punct::LParen)?;
    let mut params = Vec::new();
    let mut varargs = false;
    if !parser.eat_punct(Punct::RParen) {
        loop {
            if parser.eat_punct(Punct::Ellipsis) {
                varargs = true;
                parser.expect_punct(Punct::RParen)?;
                break;
            }
            let name = match parser.bump().kind {
                TokenKind::Ident(name) => name,
                _ => return Err(codes::e15("invalid function parameter")),
            };
            let default = if parser.eat_punct(Punct::Assign) {
                Some(parser.parse_expression()?)
            } else {
                None
            };
            params.push(FuncParam { name, default });
            if parser.eat_punct(Punct::Comma) {
                if parser.eat_punct(Punct::RParen) {
                    break;
                }
                continue;
            }
            parser.expect_punct(Punct::RParen)?;
            break;
        }
    }
    let mut dict = false;
    let mut abort = false;
    let mut range = false;
    let mut closure = false;
    while !parser.at_eof() {
        match parser.current_kind().clone() {
            TokenKind::Ident(name) => {
                match name.as_bytes() {
                    b"dict" => dict = true,
                    b"abort" => abort = true,
                    b"range" => range = true,
                    b"closure" => closure = true,
                    _ => {
                        return Err(codes::e488(&format!(
                            "unexpected function attribute {}",
                            name.to_string_lossy()
                        )))
                    }
                }
                parser.bump();
            }
            TokenKind::String(_) => break,
            other => {
                return Err(codes::e488(&format!(
                    "unexpected {other:?} after function header"
                )));
            }
        }
    }
    Ok(FunctionDef {
        written_name,
        params,
        varargs,
        dict,
        abort,
        range,
        closure,
        bang,
        body: Vec::new(),
        line,
    })
}

fn parse_catch_pattern(rest: &[u8]) -> Result<Option<Vec<u8>>> {
    let input = trim_start(rest);
    if input.is_empty() || input[0] == b'"' {
        return Ok(None);
    }
    let delimiter = input[0];
    if delimiter.is_ascii_alphanumeric() || delimiter == b'\\' {
        return Err(codes::e15("missing delimiter in :catch pattern"));
    }
    let mut pattern = Vec::new();
    let mut index = 1;
    while index < input.len() {
        let byte = input[index];
        if byte == b'\\' && index + 1 < input.len() {
            if input[index + 1] == delimiter {
                pattern.push(delimiter);
            } else {
                pattern.push(b'\\');
                pattern.push(input[index + 1]);
            }
            index += 2;
            continue;
        }
        if byte == delimiter {
            return Ok(Some(pattern));
        }
        pattern.push(byte);
        index += 1;
    }
    Err(codes::e15("unterminated :catch pattern"))
}

fn raw_argument(rest: &[u8]) -> String {
    let input = trim(rest);
    if input.is_empty() {
        return String::new();
    }
    let mut end = input.len();
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    let mut index = 0;
    while index < input.len() {
        let byte = input[index];
        if double {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                double = false;
            }
        } else if single {
            if byte == b'\'' {
                single = false;
            }
        } else {
            match byte {
                b'"' => {
                    if index == 0 || input.get(index - 1) == Some(&b' ') {
                        end = index;
                        break;
                    }
                    double = true;
                }
                b'\'' => single = true,
                _ => {}
            }
        }
        index += 1;
    }
    String::from_utf8_lossy(trim_end(&input[..end])).into_owned()
}

fn parse_one_expression(rest: &[u8], _line: u32) -> Result<Expr> {
    let input = trim_start(rest);
    if input.is_empty() {
        return Err(codes::e15("missing expression"));
    }
    let mut parser = Parser::new(input)?;
    let expression = parser.parse_expression()?;
    finish_expression(&parser, input)?;
    Ok(expression)
}

fn parse_expression_list(rest: &[u8]) -> Result<Vec<Expr>> {
    let input = trim_start(rest);
    let mut parser = Parser::new(input)?;
    let mut expressions = Vec::new();
    while !parser.at_eof() {
        expressions.push(parser.parse_expression()?);
    }
    Ok(expressions)
}

fn finish_expression(parser: &Parser, input: &[u8]) -> Result<()> {
    if parser.at_eof() {
        return Ok(());
    }
    let position = parser.current_pos();
    if input.get(position) == Some(&b'"') {
        return Ok(());
    }
    Err(codes::e488(&format!(
        "unexpected {:?}",
        parser.current_kind()
    )))
}

/// Whether a name is one of the internal script commands (used by exists(':x')).
pub(crate) fn is_internal_command(name: &str) -> bool {
    resolve_command(name.as_bytes()).is_some()
}

pub fn parse_script(source: &[u8]) -> Result<Vec<Stmt>> {
    BlockParser {
        lines: preprocess_source(source),
        index: 0,
    }
    .parse_all()
}

pub fn parse_function_body(lines: &[FuncLine]) -> Result<Vec<Stmt>> {
    let lines = lines
        .iter()
        .map(|line| SourceLine {
            line: line.line,
            text: line.text.clone(),
        })
        .collect();
    BlockParser { lines, index: 0 }.parse_all()
}

pub fn execute_script(
    source: &[u8],
    script_name: &str,
    runtime: &mut VimRuntime,
    host: &mut dyn VimHost,
) -> Result<()> {
    let id = runtime.push_script(script_name);
    runtime.enter_source();
    let mut ctx = Ctx::new(runtime, host);
    let result = run_script_source(&mut ctx, source);
    let result = match result {
        Ok(()) => Ok(()),
        Err(error) => {
            let error = ctx.decorate(error).into_uncaught_throw();
            ctx.emit_error(&error);
            Err(error)
        }
    };
    drop(ctx);
    runtime.leave_source();
    runtime.pop_script(id);
    result
}

/// Execute `source` as if it was the body of the script with `script_id`.
///
/// This is used for user command replacements, which run in the script
/// context where the command was defined so `s:` variables and functions
/// resolve there.
pub fn execute_script_in_context(
    source: &[u8],
    script_id: ScriptId,
    runtime: &mut VimRuntime,
    host: &mut dyn VimHost,
) -> Result<()> {
    runtime.enter_source();
    runtime.push_context_script(script_id);
    let mut ctx = Ctx::new(runtime, host);
    let result = run_script_source(&mut ctx, source);
    let result = match result {
        Ok(()) => Ok(()),
        Err(error) => {
            let error = ctx.decorate(error).into_uncaught_throw();
            ctx.emit_error(&error);
            Err(error)
        }
    };
    drop(ctx);
    runtime.pop_context_script();
    runtime.leave_source();
    result
}

fn run_script_source(ctx: &mut Ctx, source: &[u8]) -> Result<()> {
    let statements = parse_script(source).map_err(|error| ctx.attribute_parse_error(error))?;
    match exec_statements(ctx, &statements)? {
        Flow::Normal | Flow::Finish => Ok(()),
        Flow::Return(_) => Err(VimError::new(133, ":return not inside a function")),
        Flow::Break => Err(VimError::new(587, ":break without :while or :for")),
        Flow::Continue => Err(VimError::new(588, ":continue without :while or :for")),
    }
}

pub(crate) fn execute_commands_text(ctx: &mut Ctx, text: &str) -> Result<Flow> {
    let statements =
        parse_script(text.as_bytes()).map_err(|error| ctx.attribute_parse_error(error))?;
    exec_statements(ctx, &statements)
}

pub(crate) fn exec_statements(ctx: &mut Ctx, statements: &[Stmt]) -> Result<Flow> {
    for statement in statements {
        let old_silent = ctx.silent;
        let old_command = ctx.command;
        ctx.set_statement(statement.line, statement_command(statement));
        if statement.silent {
            ctx.silent = true;
        }
        let result = exec_statement(ctx, statement);
        let result = if statement.force {
            match result {
                Ok(flow) => Ok(flow),
                Err(error) => {
                    ctx.runtime.set_last_error(&error);
                    Ok(Flow::Normal)
                }
            }
        } else {
            match result {
                Ok(flow) => Ok(flow),
                Err(error) => Err(ctx.decorate(error)),
            }
        };
        ctx.silent = old_silent;
        ctx.command = old_command;
        match result {
            Ok(Flow::Normal) => {
                crate::gc::maybe_collect_cycles();
            }
            Ok(flow) => return Ok(flow),
            Err(error) => return Err(error),
        }
    }
    Ok(Flow::Normal)
}

fn statement_command(statement: &Stmt) -> &'static str {
    match &statement.kind {
        StmtKind::Let { .. } => "let",
        StmtKind::Unlet { .. } => "unlet",
        StmtKind::Lock { .. } => "lockvar",
        StmtKind::If { .. } => "if",
        StmtKind::While { .. } => "while",
        StmtKind::Try { .. } => "try",
        StmtKind::For { .. } => "for",
        StmtKind::Function(_) => "function",
        StmtKind::Return(_) => "return",
        StmtKind::Call(_) => "call",
        StmtKind::Echo { kind, .. } => match kind {
            EchoKind::Echo => "echo",
            EchoKind::Echon => "echon",
            EchoKind::Echomsg => "echomsg",
            EchoKind::Echoerr => "echoerr",
        },
        StmtKind::Execute { .. } => "execute",
        StmtKind::Throw(_) => "throw",
        StmtKind::Break => "break",
        StmtKind::Continue => "continue",
        StmtKind::Finish => "finish",
        StmtKind::Source(_) => "source",
        StmtKind::ScriptEncoding(_) => "scriptencoding",
        StmtKind::ScriptVersion(_) => "scriptversion",
        StmtKind::Delfunction(_) => "delfunction",
        StmtKind::Eval(_) => "eval",
        StmtKind::Ex(_) => "ex",
    }
}

fn exec_statement(ctx: &mut Ctx, statement: &Stmt) -> Result<Flow> {
    match &statement.kind {
        StmtKind::Let {
            target,
            op,
            value,
            lock,
        } => {
            let value = eval::eval(ctx, value)?;
            eval::assign_target(ctx, target, value, *op)?;
            if *lock {
                lock_target(ctx, target, true, 1)?;
            }
            Ok(Flow::Normal)
        }
        StmtKind::Unlet { bang, targets } => {
            for target in targets {
                delete_expression(ctx, target, *bang)?;
            }
            Ok(Flow::Normal)
        }
        StmtKind::Lock {
            lock,
            depth,
            targets,
        } => {
            for target in targets {
                lock_expression(ctx, target, *lock, *depth)?;
            }
            Ok(Flow::Normal)
        }
        StmtKind::If {
            branches,
            else_body,
        } => {
            for (condition, body) in branches {
                if ops::truthy(&eval::eval(ctx, condition)?)? {
                    return exec_statements(ctx, body);
                }
            }
            if let Some(body) = else_body {
                return exec_statements(ctx, body);
            }
            Ok(Flow::Normal)
        }
        StmtKind::While { condition, body } => {
            while ops::truthy(&eval::eval(ctx, condition)?)? {
                match exec_statements(ctx, body)? {
                    Flow::Normal => {}
                    Flow::Break => break,
                    Flow::Continue => continue,
                    other => return Ok(other),
                }
            }
            Ok(Flow::Normal)
        }
        StmtKind::For {
            target,
            iterable,
            body,
        } => {
            let values = eval::iterable_values(&eval::eval(ctx, iterable)?)?;
            for value in values {
                eval::assign_target(ctx, target, value, AssignOp::Assign)?;
                match exec_statements(ctx, body)? {
                    Flow::Normal => {}
                    Flow::Break => break,
                    Flow::Continue => continue,
                    other => return Ok(other),
                }
            }
            Ok(Flow::Normal)
        }
        StmtKind::Function(definition) => {
            define_function(ctx, definition)?;
            Ok(Flow::Normal)
        }
        StmtKind::Return(expression) => {
            if ctx.runtime.current_frame().is_none() {
                return Err(VimError::new(133, ":return not inside a function"));
            }
            let value = match expression {
                Some(expression) => eval::eval(ctx, expression)?,
                None => Value::Number(0),
            };
            Ok(Flow::Return(value))
        }
        StmtKind::Call(expression) => {
            eval::eval(ctx, expression)?;
            Ok(Flow::Normal)
        }
        StmtKind::Echo { kind, expressions } => {
            let mut values = Vec::new();
            for expression in expressions {
                values.push(eval::eval(ctx, expression)?);
            }
            let text = values
                .iter()
                .map(Value::echo_string)
                .collect::<Vec<_>>()
                .join(" ");
            match kind {
                EchoKind::Echo => ctx.emit(&text, true),
                EchoKind::Echon => ctx.emit(&text, false),
                EchoKind::Echomsg => {
                    ctx.runtime
                        .set_vvar("statusmsg", Value::String(VimString::from(text.clone())));
                    ctx.emit(&text, true);
                }
                EchoKind::Echoerr => {
                    let error = VimError {
                        code: None,
                        message: text.clone(),
                        throwpoint: None,
                        exception: Some(format!("Vim(echoerr):{text}")),
                        kind: crate::error::ErrorKind::Error,
                    };
                    return Err(error);
                }
            }
            Ok(Flow::Normal)
        }
        StmtKind::Execute { expressions } => {
            let mut pieces = Vec::new();
            for expression in expressions {
                pieces.push(ops::to_string(&eval::eval(ctx, expression)?)?);
            }
            let mut command = VimString::new();
            for (index, piece) in pieces.iter().enumerate() {
                if index > 0 {
                    command.push(b' ');
                }
                command.extend_from_slice(piece.as_bytes());
            }
            let text = String::from_utf8_lossy(command.as_bytes()).into_owned();
            execute_commands_text(ctx, &text)
        }
        StmtKind::Throw(expression) => {
            let value = eval::eval(ctx, expression)?;
            Err(eval::throw_value(&value)?)
        }
        StmtKind::Break => Ok(Flow::Break),
        StmtKind::Continue => Ok(Flow::Continue),
        StmtKind::Finish => Ok(Flow::Finish),
        StmtKind::Source(path) => {
            execute_source(ctx, path)?;
            Ok(Flow::Normal)
        }
        StmtKind::ScriptEncoding(encoding) => {
            let normalized = encoding.to_ascii_lowercase().replace('-', "");
            if normalized != "utf8" {
                return Err(VimError::plain(format!(
                    "scriptencoding {encoding} is not supported; RVim stores scripts as UTF-8"
                )));
            }
            Ok(Flow::Normal)
        }
        StmtKind::ScriptVersion(version) => {
            if !(1..=4).contains(version) {
                return Err(VimError::new(
                    481,
                    format!("Invalid scriptversion: {version}"),
                ));
            }
            Ok(Flow::Normal)
        }
        StmtKind::Delfunction(written) => {
            let written = written.to_string_lossy().into_owned();
            let canonical = eval::resolve_function_name(ctx, &written);
            if ctx.runtime.remove_function(&canonical).is_none() {
                return Err(codes::e130(&written));
            }
            Ok(Flow::Normal)
        }
        StmtKind::Eval(expression) => {
            eval::eval(ctx, expression)?;
            Ok(Flow::Normal)
        }
        StmtKind::Ex(command) => {
            // ":runtime" must load into this runtime; routing it through the
            // editor would start a nested script context and lose globals.
            if let Some((all, pattern)) = parse_runtime_invocation(command) {
                for (name, bytes) in ctx.host.runtime_files(&pattern, all)? {
                    run_sourced_script(ctx, &name, &bytes)?;
                }
                return Ok(Flow::Normal);
            }
            if let Some((expanded, script_id)) = ctx.host.user_command_expansion(command)? {
                // Run the replacement inside this runtime so nested :execute,
                // :call and :source statements keep working, and switch to the
                // defining script's s: context while it runs.
                let saved = (ctx.script_id, ctx.script_name.clone());
                if let Some(script_id) = script_id.filter(|id| !id.is_nil()) {
                    ctx.script_id = script_id;
                    ctx.script_name = ctx.runtime.script_name(script_id).unwrap_or_default();
                }
                let result = execute_commands_text(ctx, &expanded);
                ctx.script_id = saved.0;
                ctx.script_name = saved.1;
                result?;
                return Ok(Flow::Normal);
            }
            let context = ctx.ex_context();
            ctx.host.execute_ex(command, &context)?;
            Ok(Flow::Normal)
        }
        StmtKind::Try {
            body,
            catches,
            finally_body,
        } => {
            let mut outcome = exec_statements(ctx, body);
            if let Err(error) = &outcome {
                for (catch_pattern, catch_body) in catches {
                    let matched = match catch_pattern {
                        None => true,
                        Some(text) => match pattern::compile(text) {
                            Ok(compiled) => pattern::find(
                                &compiled,
                                error.exception_string().as_bytes(),
                                0,
                                ctx.ignorecase(),
                            )
                            .is_some(),
                            Err(pattern_error) => {
                                outcome = Err(pattern_error);
                                break;
                            }
                        },
                    };
                    if matched {
                        ctx.runtime.set_vvar(
                            "exception",
                            Value::String(VimString::from(error.exception_string())),
                        );
                        ctx.runtime.set_vvar(
                            "throwpoint",
                            Value::String(VimString::from(
                                error.throwpoint.clone().unwrap_or_default(),
                            )),
                        );
                        outcome = exec_statements(ctx, catch_body);
                        break;
                    }
                }
            }
            if let Some(finally) = finally_body {
                let final_outcome = exec_statements(ctx, finally);
                match final_outcome {
                    Ok(Flow::Normal) => {}
                    other => outcome = other,
                }
            }
            outcome
        }
    }
}

fn define_function(ctx: &mut Ctx, definition: &FunctionDef) -> Result<()> {
    if definition.range {
        return Err(VimError::plain(
            "the function attribute 'range' is not supported by this engine",
        ));
    }
    let written = definition.written_name.to_string_lossy().into_owned();
    let canonical = eval::resolve_function_name(ctx, &written);
    if !definition.bang && ctx.runtime.has_function(&canonical) {
        return Err(codes::e122(&written));
    }
    if builtins::is_builtin(&canonical) {
        return Err(VimError::new(
            705,
            format!("Function name conflicts with builtin: {written}"),
        ));
    }
    let scoped = written.starts_with("s:")
        || written.to_ascii_lowercase().starts_with("<sid>")
        || written.starts_with('<');
    if !scoped
        && written
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && !written.contains('#')
    {
        return Err(codes::e128(&written));
    }
    let closure_scope = if definition.closure {
        ctx.runtime.current_frame().map(|frame| {
            Rc::new(crate::value::ClosureScope::new(
                frame.locals.clone(),
                frame.args.clone(),
                frame.closure.clone(),
            ))
        })
    } else {
        None
    };
    let script_id = ctx
        .current_script_id()
        .unwrap_or_else(|| ScriptId::from_raw(0));
    let function = UserFunc {
        name: VimString::from(canonical),
        display_name: definition.written_name.clone(),
        params: definition.params.clone(),
        varargs: definition.varargs,
        dict: definition.dict,
        abort: definition.abort,
        range: definition.range,
        closure: definition.closure,
        body: definition.body.clone(),
        script_id,
        def_line: definition.line,
        closure_scope,
        cache: Rc::new(RefCell::new(None)),
    };
    ctx.runtime.define_function(function);
    Ok(())
}

fn parsed_body(ctx: &mut Ctx, function: &UserFunc) -> Result<Rc<Vec<Stmt>>> {
    if let Some(parsed) = function.cache.borrow().clone() {
        return Ok(parsed);
    }
    let parsed = Rc::new(
        parse_function_body(&function.body).map_err(|error| ctx.attribute_parse_error(error))?,
    );
    *function.cache.borrow_mut() = Some(parsed.clone());
    let _ = ctx;
    Ok(parsed)
}

pub(crate) fn call_user_function(
    ctx: &mut Ctx,
    function: &UserFunc,
    args: &[Value],
    self_dict: Option<Value>,
) -> Result<Value> {
    if ctx.runtime.frame_count() >= ctx.runtime.maxfuncdepth() {
        return Err(VimError::new(
            132,
            "Function call depth is higher than 'maxfuncdepth'",
        ));
    }
    let display = function.display_name.to_string_lossy().into_owned();
    let required = function
        .params
        .iter()
        .filter(|param| param.default.is_none())
        .count();
    if args.len() < required {
        return Err(codes::e119(&display));
    }
    if !function.varargs && args.len() > function.params.len() {
        return Err(codes::e118(&display));
    }
    if function.dict && self_dict.is_none() {
        return Err(VimError::new(
            725,
            format!("Dictionary required for function: {display}"),
        ));
    }
    let locals = dict_ref(VimDict::new());
    let arguments = dict_ref(VimDict::new());
    let frame = CallFrame {
        function: function.display_name.clone(),
        script_id: function.script_id,
        locals,
        args: arguments,
        closure: function.closure_scope.clone(),
        self_dict,
        line: function.def_line,
    };
    let saved = (
        ctx.script_id,
        ctx.script_name.clone(),
        ctx.function.clone(),
        ctx.line,
    );
    ctx.runtime.push_frame(frame);
    ctx.script_id = function.script_id;
    ctx.script_name = ctx
        .runtime
        .script_name(function.script_id)
        .unwrap_or_default();
    ctx.function = Some(display);
    ctx.line = function.def_line;

    let outcome: Result<Flow> = (|| {
        let arguments = ctx
            .runtime
            .current_frame()
            .map(|frame| frame.args.clone())
            .unwrap_or_else(|| dict_ref(VimDict::new()));
        for (index, param) in function.params.iter().enumerate() {
            let value = match args.get(index) {
                Some(value) => value.clone(),
                None => match &param.default {
                    Some(default) => eval::eval(ctx, default)?,
                    None => return Err(codes::e119(&function.display_name.to_string_lossy())),
                },
            };
            arguments
                .try_borrow_mut()
                .map_err(|_| VimError::plain("argument dictionary is already borrowed"))?
                .set(param.name.clone(), value);
        }
        let extras = if function.varargs {
            args.get(function.params.len()..).unwrap_or(&[]).to_vec()
        } else {
            Vec::new()
        };
        {
            let mut arguments = arguments
                .try_borrow_mut()
                .map_err(|_| VimError::plain("argument dictionary is already borrowed"))?;
            arguments.set_str("0", Value::Number(extras.len() as i64));
            arguments.set_str("000", Value::list(extras.clone()));
            for (index, value) in extras.into_iter().enumerate() {
                arguments.set(VimString::from((index + 1).to_string()), value);
            }
        }
        let body = parsed_body(ctx, function)?;
        exec_statements(ctx, &body)
    })();
    let outcome = match outcome {
        Ok(flow) => Ok(flow),
        Err(error) => Err(ctx.decorate(error)),
    };
    ctx.runtime.pop_frame();
    ctx.script_id = saved.0;
    ctx.script_name = saved.1;
    ctx.function = saved.2;
    ctx.line = saved.3;
    match outcome? {
        Flow::Return(value) => Ok(value),
        Flow::Normal | Flow::Finish => Ok(Value::Number(0)),
        Flow::Break => Err(VimError::new(587, ":break without :while or :for")),
        Flow::Continue => Err(VimError::new(588, ":continue without :while or :for")),
    }
}

pub(crate) fn call_lambda(
    ctx: &mut Ctx,
    data: &crate::value::FuncrefData,
    args: Vec<Value>,
) -> Result<Value> {
    let Some(body) = &data.lambda else {
        return Err(codes::e117(&data.name.to_string_lossy()));
    };
    if ctx.runtime.frame_count() >= ctx.runtime.maxfuncdepth() {
        return Err(VimError::new(
            132,
            "Function call depth is higher than 'maxfuncdepth'",
        ));
    }
    if args.len() < body.params.len() {
        return Err(codes::e119("<lambda>"));
    }
    if !body.varargs && args.len() > body.params.len() {
        return Err(codes::e118("<lambda>"));
    }
    let locals = dict_ref(VimDict::new());
    let arguments = dict_ref(VimDict::new());
    {
        let mut arguments = arguments
            .try_borrow_mut()
            .map_err(|_| VimError::plain("argument dictionary is already borrowed"))?;
        for (index, name) in body.params.iter().enumerate() {
            arguments.set(name.clone(), args[index].clone());
        }
        let extras = if body.varargs {
            args.get(body.params.len()..).unwrap_or(&[]).to_vec()
        } else {
            Vec::new()
        };
        arguments.set_str("0", Value::Number(extras.len() as i64));
        arguments.set_str("000", Value::list(extras.clone()));
        for (index, value) in extras.into_iter().enumerate() {
            arguments.set(VimString::from((index + 1).to_string()), value);
        }
    }
    let display = data.name.to_string_lossy().into_owned();
    let frame = CallFrame {
        function: data.name.clone(),
        script_id: data.script_id,
        locals,
        args: arguments,
        closure: data.closure.clone(),
        self_dict: None,
        line: body.line,
    };
    let saved = (
        ctx.script_id,
        ctx.script_name.clone(),
        ctx.function.clone(),
        ctx.line,
    );
    ctx.runtime.push_frame(frame);
    ctx.script_id = data.script_id;
    ctx.script_name = ctx.runtime.script_name(data.script_id).unwrap_or_default();
    ctx.function = Some(display);
    ctx.line = body.line;
    let outcome = eval::eval(ctx, &body.body);
    let outcome = match outcome {
        Ok(value) => Ok(value),
        Err(error) => Err(ctx.decorate(error)),
    };
    ctx.runtime.pop_frame();
    ctx.script_id = saved.0;
    ctx.script_name = saved.1;
    ctx.function = saved.2;
    ctx.line = saved.3;
    outcome
}

/// Expand `<sfile>` in a `:source` argument using the currently sourced
/// script name and the `:p`, `:h`, `:t`, `:r`, `:e` modifiers.
fn expand_sfile(path: &str, script: &str) -> String {
    const MARKER: &str = "<sfile>";
    if script.is_empty() || !path.contains(MARKER) {
        return path.to_string();
    }
    let mut out = String::new();
    let mut rest = path;
    while let Some(position) = rest.find(MARKER) {
        out.push_str(&rest[..position]);
        let mut value = script.to_string();
        let mut tail = &rest[position + MARKER.len()..];
        while let Some(after) = tail.strip_prefix(':') {
            let Some(modifier) = after.chars().next() else {
                break;
            };
            let replacement = match modifier {
                'p' => Some(std::path::Path::new(&value).canonicalize().map_or_else(
                    |_| {
                        std::env::current_dir().map_or_else(
                            |_| value.clone(),
                            |directory| directory.join(&value).display().to_string(),
                        )
                    },
                    |path| path.display().to_string(),
                )),
                'h' => std::path::Path::new(&value)
                    .parent()
                    .map(|parent| parent.display().to_string())
                    .filter(|parent| !parent.is_empty())
                    .or_else(|| Some(".".to_string())),
                't' => std::path::Path::new(&value)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned()),
                'r' => std::path::Path::new(&value)
                    .file_stem()
                    .map(|name| name.to_string_lossy().into_owned()),
                'e' => std::path::Path::new(&value)
                    .extension()
                    .map(|extension| extension.to_string_lossy().into_owned()),
                _ => None,
            };
            let Some(replacement) = replacement else {
                break;
            };
            value = replacement;
            tail = &after[modifier.len_utf8()..];
        }
        out.push_str(&value);
        rest = tail;
    }
    out.push_str(rest);
    out
}

fn execute_source(ctx: &mut Ctx, path: &VimString) -> Result<()> {
    let raw = path.to_string_lossy().into_owned();
    let name = expand_sfile(&raw, &ctx.script_name);
    let source = ctx.host.read_file(&name)?;
    run_sourced_script(ctx, &name, &source)
}

/// Run script bytes in the current runtime, as if a `:source` loaded them.
///
/// Used for `:source`, `:runtime` and autoload scripts so globals and
/// functions defined by the script stay in the runtime that is executing.
pub(crate) fn run_sourced_script(ctx: &mut Ctx, name: &str, source: &[u8]) -> Result<()> {
    if ctx.runtime.source_depth() >= 100 {
        return Err(VimError::new(22, "Script sourcing is nested too deeply"));
    }
    let id = ctx.runtime.push_script(name);
    ctx.runtime.enter_source();
    let saved = (
        ctx.script_id,
        ctx.script_name.clone(),
        ctx.function.clone(),
        ctx.line,
    );
    ctx.script_id = id;
    ctx.script_name = name.to_string();
    ctx.function = None;
    ctx.line = 1;
    let result = (|| -> Result<()> {
        let statements = parse_script(source).map_err(|error| ctx.attribute_parse_error(error))?;
        match exec_statements(ctx, &statements)? {
            Flow::Normal | Flow::Finish => Ok(()),
            Flow::Return(_) => Err(VimError::new(133, ":return not inside a function")),
            Flow::Break => Err(VimError::new(587, ":break without :while or :for")),
            Flow::Continue => Err(VimError::new(588, ":continue without :while or :for")),
        }
    })();
    let result = match result {
        Ok(()) => Ok(()),
        Err(error) => Err(ctx.decorate(error)),
    };
    ctx.script_id = saved.0;
    ctx.script_name = saved.1;
    ctx.function = saved.2;
    ctx.line = saved.3;
    ctx.runtime.leave_source();
    ctx.runtime.pop_script(id);
    result
}

fn delete_expression(ctx: &mut Ctx, expression: &Expr, bang: bool) -> Result<()> {
    match expression {
        Expr::Var { scope, name } => eval::delete_var(ctx, *scope, name, bang),
        Expr::Index { base, index } => {
            let base = eval::eval(ctx, base)?;
            let index = eval::eval(ctx, index)?;
            let Value::Dict(dict) = base else {
                return Err(codes::e15("only dictionary entries can be unlet by index"));
            };
            let key = match index {
                Value::String(key) => key,
                other => ops::to_string(&other)?,
            };
            let mut dict = dict
                .try_borrow_mut()
                .map_err(|_| VimError::plain("Dictionary is already borrowed"))?;
            if dict.locked || dict.entry_locked(&key) {
                return Err(codes::e741(&key.to_string_lossy()));
            }
            if dict.remove(&key).is_some() || bang {
                Ok(())
            } else {
                Err(codes::e716(&key.to_string_lossy()))
            }
        }
        Expr::Member { base, key } => {
            let base = eval::eval(ctx, base)?;
            let Value::Dict(dict) = base else {
                return Err(codes::e15("only dictionary entries can be unlet by member"));
            };
            let mut dict = dict
                .try_borrow_mut()
                .map_err(|_| VimError::plain("Dictionary is already borrowed"))?;
            if dict.locked || dict.entry_locked(key) {
                return Err(codes::e741(&key.to_string_lossy()));
            }
            if dict.remove(key).is_some() || bang {
                Ok(())
            } else {
                Err(codes::e716(&key.to_string_lossy()))
            }
        }
        _ => Err(codes::e15("invalid :unlet target")),
    }
}

fn lock_target(ctx: &mut Ctx, target: &AssignTarget, locked: bool, depth: i32) -> Result<()> {
    match target {
        AssignTarget::Expr(expression) => lock_expression(ctx, expression, locked, depth),
        AssignTarget::Destructure { names, rest } => {
            for name in names {
                lock_expression(ctx, name, locked, depth)?;
            }
            if let Some(rest) = rest {
                lock_expression(ctx, rest, locked, depth)?;
            }
            Ok(())
        }
        AssignTarget::DynamicName { scope, parts } => {
            let name = eval::resolve_dynamic_name(ctx, parts)?;
            lock_expression(
                ctx,
                &Expr::Var {
                    scope: *scope,
                    name,
                },
                locked,
                depth,
            )
        }
    }
}

fn lock_expression(ctx: &mut Ctx, expression: &Expr, locked: bool, depth: i32) -> Result<()> {
    match expression {
        Expr::Var { scope, name } => {
            let display = format!(
                "{}{}",
                scope.map_or("", Scope::prefix),
                name.to_string_lossy()
            );
            match scope {
                Some(Scope::Global) => {
                    ctx.runtime
                        .lock_global(&name.to_string_lossy(), locked, depth.max(1))
                }
                None if ctx.runtime.current_frame().is_none() => {
                    ctx.runtime
                        .lock_global(&name.to_string_lossy(), locked, depth.max(1))
                }
                _ => {
                    let dict = scope_dict(ctx, *scope, name)?;
                    let mut dict = dict
                        .try_borrow_mut()
                        .map_err(|_| VimError::plain("Dictionary is already borrowed"))?;
                    let Some(entry) = dict.entries.iter_mut().find(|entry| entry.key == *name)
                    else {
                        return Err(codes::e108(&display));
                    };
                    entry.locked = locked;
                    lock_container(&entry.value, locked, depth.max(1));
                    Ok(())
                }
            }
        }
        Expr::Index { base, index } => {
            let base = eval::eval(ctx, base)?;
            let index = eval::eval(ctx, index)?;
            match &base {
                Value::Dict(dict) => {
                    let key = match index {
                        Value::String(key) => key,
                        other => ops::to_string(&other)?,
                    };
                    let mut dict = dict
                        .try_borrow_mut()
                        .map_err(|_| VimError::plain("Dictionary is already borrowed"))?;
                    let Some(entry) = dict.entries.iter_mut().find(|entry| entry.key == key) else {
                        return Err(codes::e716(&key.to_string_lossy()));
                    };
                    entry.locked = locked;
                    lock_container(&entry.value, locked, depth.max(1));
                    Ok(())
                }
                Value::List(list) => {
                    let number = ops::to_number(&index)?;
                    let list = list
                        .try_borrow()
                        .map_err(|_| VimError::plain("List is already borrowed"))?;
                    let Some(value) = index_list(&list.items, number) else {
                        return Err(codes::e684(number));
                    };
                    lock_container(&value, locked, depth.max(1));
                    Ok(())
                }
                _ => Err(codes::e15("invalid :lockvar target")),
            }
        }
        Expr::Member { base, key } => {
            let base = eval::eval(ctx, base)?;
            let Value::Dict(dict) = base else {
                return Err(codes::e15("invalid :lockvar target"));
            };
            let mut dict = dict
                .try_borrow_mut()
                .map_err(|_| VimError::plain("Dictionary is already borrowed"))?;
            let Some(entry) = dict.entries.iter_mut().find(|entry| entry.key == *key) else {
                return Err(codes::e716(&key.to_string_lossy()));
            };
            entry.locked = locked;
            lock_container(&entry.value, locked, depth.max(1));
            Ok(())
        }
        _ => Err(codes::e15("invalid :lockvar target")),
    }
}

fn index_list(items: &[Value], number: i64) -> Option<Value> {
    let length = items.len() as i64;
    let index = if number < 0 { length + number } else { number };
    if index < 0 || index >= length {
        None
    } else {
        Some(items[index as usize].clone())
    }
}

fn scope_dict(
    ctx: &mut Ctx,
    scope: Option<Scope>,
    name: &VimString,
) -> Result<crate::value::DictRef> {
    match scope {
        Some(Scope::Script) => ctx
            .current_script_vars()
            .ok_or_else(|| codes::e108(&name.to_string_lossy())),
        Some(Scope::Local) | Some(Scope::Argument) => ctx
            .runtime
            .current_frame()
            .map(|frame| {
                if scope == Some(Scope::Argument) {
                    frame.args.clone()
                } else {
                    frame.locals.clone()
                }
            })
            .ok_or_else(|| codes::e108(&name.to_string_lossy())),
        None => {
            if let Some(frame) = ctx.runtime.current_frame() {
                Ok(frame.locals.clone())
            } else {
                Err(codes::e15(
                    ":lockvar on a global variable requires g: prefix",
                ))
            }
        }
        _ => Err(codes::e15(":lockvar is not supported for this scope")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::BasicHost;
    use crate::value::Value;

    fn run(source: &str) {
        let mut runtime = VimRuntime::new();
        let mut host = BasicHost::new();
        execute_script(source.as_bytes(), "test.vim", &mut runtime, &mut host).unwrap();
    }

    #[test]
    fn block_structure_parses() {
        let statements = parse_script(
            b"if 1\n  let g:x = 1\nelseif 0\n  let g:x = 2\nelse\n  let g:x = 3\nendif\n",
        )
        .unwrap();
        assert_eq!(statements.len(), 1);
        assert!(matches!(statements[0].kind, StmtKind::If { .. }));
    }

    #[test]
    fn functions_and_calls() {
        let mut runtime = VimRuntime::new();
        let mut host = BasicHost::new();
        execute_script(
            b"function! Add(a, b)\n  return a:a + a:b\nendfunction\nlet g:result = Add(2, 3)\n",
            "test.vim",
            &mut runtime,
            &mut host,
        )
        .unwrap();
        assert_eq!(runtime.get_global("result"), Some(Value::Number(5)));
    }

    #[test]
    fn try_catch_finally() {
        let mut runtime = VimRuntime::new();
        let mut host = BasicHost::new();
        execute_script(
            b"let g:log = []\ntry\n  throw 'boom'\ncatch /boom/\n  call add(g:log, 'caught')\nfinally\n  call add(g:log, 'finally')\nendtry\n",
            "test.vim",
            &mut runtime,
            &mut host,
        )
        .unwrap();
        let log = runtime.get_global("log").unwrap();
        let list = log.as_list().unwrap();
        let items: Vec<String> = list
            .borrow()
            .items
            .iter()
            .map(|value| value.echo_string())
            .collect();
        assert_eq!(items, vec!["caught", "finally"]);
    }

    #[test]
    fn loops_and_conditionals() {
        run("let g:sum = 0\nfor i in range(1, 4)\n  let g:sum += i\nendfor\n");
        let mut runtime = VimRuntime::new();
        let mut host = BasicHost::new();
        execute_script(
            b"let g:sum = 0\nfor i in range(1, 4)\n  let g:sum += i\nendfor\n",
            "test.vim",
            &mut runtime,
            &mut host,
        )
        .unwrap();
        assert_eq!(runtime.get_global("sum"), Some(Value::Number(10)));
    }
}
