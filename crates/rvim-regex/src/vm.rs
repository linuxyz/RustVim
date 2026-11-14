//! Backtracking byte-code virtual machine.
//!
//! The VM is deliberately a reference implementation: it explores alternatives
//! in Vim's priority order (earlier start, earlier branch, greedy before lazy)
//! and keeps a configurable step budget so pathological patterns fail cleanly.

use crate::ast::{Anchor, LookKind};
use crate::charclass::{byte_eq, is_word_byte};
use crate::compiler::{Inst, Program};
use crate::error::MatchError;
use crate::{Match, Regex};

#[derive(Debug, Clone, Copy, Default)]
struct CaptureSlot {
    start: Option<usize>,
    end: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
struct RepeatFrame {
    count: u64,
    last_pos: usize,
    min: u32,
    max: Option<u32>,
    greedy: bool,
    body: usize,
    after: usize,
    loop_pc: usize,
}

#[derive(Debug, Clone)]
struct VmState {
    captures: Vec<CaptureSlot>,
    match_start: Option<usize>,
    match_end: Option<usize>,
    repeats: Vec<RepeatFrame>,
}

impl VmState {
    fn new(groups: usize) -> Self {
        Self {
            captures: vec![CaptureSlot::default(); groups],
            match_start: None,
            match_end: None,
            repeats: Vec::new(),
        }
    }
}

#[derive(Debug)]
struct Frame {
    pc: usize,
    pos: usize,
    state: VmState,
}

#[derive(Debug)]
struct RunOutcome {
    pos: usize,
    state: VmState,
}

struct Ctx<'a> {
    programs: &'a [Program],
    haystack: &'a [u8],
    ignorecase: bool,
}

struct Budget {
    remaining: u64,
    limit: u64,
}

impl Budget {
    fn step(&mut self) -> Result<(), MatchError> {
        if self.remaining == 0 {
            return Err(MatchError::StepLimitExceeded { limit: self.limit });
        }
        self.remaining -= 1;
        Ok(())
    }
}

/// Search from \`start\` (inclusive) for the leftmost match.
pub(crate) fn find_at(
    regex: &Regex,
    haystack: &[u8],
    start: usize,
    allow_empty: bool,
) -> Result<Option<Match>, MatchError> {
    if start > haystack.len() {
        return Ok(None);
    }
    let ctx = Ctx {
        programs: &regex.programs,
        haystack,
        ignorecase: regex.ignorecase,
    };
    let mut budget = Budget {
        remaining: regex.step_limit,
        limit: regex.step_limit,
    };
    let mut attempt = start;
    while attempt <= haystack.len() {
        let initial = VmState::new(regex.groups);
        if let Some(outcome) = run(&ctx, 0, attempt, initial, None, &mut budget)? {
            let matched = make_match(&outcome, attempt, regex.groups);
            if allow_empty || matched.start < matched.end {
                return Ok(Some(matched));
            }
        }
        attempt += 1;
    }
    Ok(None)
}

fn make_match(outcome: &RunOutcome, attempt: usize, groups: usize) -> Match {
    let start = outcome.state.match_start.unwrap_or(attempt);
    let end = outcome.state.match_end.unwrap_or(outcome.pos).max(start);
    let mut captures = Vec::with_capacity(groups);
    for slot in outcome.state.captures.iter().take(groups) {
        match (slot.start, slot.end) {
            (Some(start), Some(end)) => captures.push(Some((start, end.max(start)))),
            _ => captures.push(None),
        }
    }
    Match {
        start,
        end,
        captures,
    }
}

fn run(
    ctx: &Ctx<'_>,
    program_id: usize,
    start: usize,
    initial: VmState,
    require_end: Option<usize>,
    budget: &mut Budget,
) -> Result<Option<RunOutcome>, MatchError> {
    let program = match ctx.programs.get(program_id) {
        Some(program) => program,
        None => return Ok(None),
    };
    let mut pc = 0usize;
    let mut pos = start;
    let mut state = initial;
    let mut stack: Vec<Frame> = Vec::new();

    macro_rules! fail {
        () => {
            match stack.pop() {
                Some(frame) => {
                    pc = frame.pc;
                    pos = frame.pos;
                    state = frame.state;
                    continue;
                }
                None => return Ok(None),
            }
        };
    }

    loop {
        budget.step()?;
        let inst = match program.code.get(pc) {
            Some(inst) => inst,
            None => return Ok(None),
        };
        match inst {
            Inst::Literal(byte) => {
                if pos < ctx.haystack.len() && byte_eq(ctx.haystack[pos], *byte, ctx.ignorecase) {
                    pos += 1;
                    pc += 1;
                } else {
                    fail!();
                }
            }
            Inst::Any => {
                if pos < ctx.haystack.len() && ctx.haystack[pos] != b'\n' {
                    pos += 1;
                    pc += 1;
                } else {
                    fail!();
                }
            }
            Inst::AnyNewline => {
                if pos < ctx.haystack.len() {
                    pos += 1;
                    pc += 1;
                } else {
                    fail!();
                }
            }
            Inst::Class(class) => {
                if pos < ctx.haystack.len() && class.matches(ctx.haystack[pos], ctx.ignorecase) {
                    pos += 1;
                    pc += 1;
                } else {
                    fail!();
                }
            }
            Inst::Anchor(anchor) => {
                if anchor_matches(*anchor, ctx.haystack, pos) {
                    pc += 1;
                } else {
                    fail!();
                }
            }
            Inst::SaveStart(index) => {
                if let Some(slot) = state.captures.get_mut(*index) {
                    slot.start = Some(pos);
                    slot.end = None;
                }
                pc += 1;
            }
            Inst::SaveEnd(index) => {
                if let Some(slot) = state.captures.get_mut(*index) {
                    slot.end = Some(pos);
                    if slot.start.is_none() {
                        slot.start = Some(pos);
                    }
                }
                pc += 1;
            }
            Inst::SetStart => {
                state.match_start = Some(pos);
                pc += 1;
            }
            Inst::SetEnd => {
                state.match_end = Some(pos);
                pc += 1;
            }
            Inst::Backref(index) => {
                let span = state
                    .captures
                    .get(*index - 1)
                    .and_then(|slot| slot.start.zip(slot.end));
                let (capture_start, capture_end) = match span {
                    Some(span) => span,
                    None => fail!(),
                };
                if capture_start > capture_end {
                    fail!();
                }
                let length = capture_end - capture_start;
                let end = match pos.checked_add(length) {
                    Some(end) if end <= ctx.haystack.len() => end,
                    _ => fail!(),
                };
                for offset in 0..length {
                    budget.step()?;
                    if !byte_eq(
                        ctx.haystack[pos + offset],
                        ctx.haystack[capture_start + offset],
                        ctx.ignorecase,
                    ) {
                        fail!();
                    }
                }
                pos = end;
                pc += 1;
            }
            Inst::Split { first, second } => {
                stack.push(Frame {
                    pc: *second,
                    pos,
                    state: state.clone(),
                });
                pc = *first;
            }
            Inst::Jmp(target) => pc = *target,
            Inst::RepeatStart {
                min,
                max,
                greedy,
                body,
                after,
            } => {
                if *max == Some(0) {
                    pc = *after;
                } else {
                    let loop_pc = pc + 1;
                    state.repeats.push(RepeatFrame {
                        count: 0,
                        last_pos: pos,
                        min: *min,
                        max: *max,
                        greedy: *greedy,
                        body: *body,
                        after: *after,
                        loop_pc,
                    });
                    pc = loop_pc;
                }
            }
            Inst::RepeatLoop => {
                let frame = match state.repeats.last().copied() {
                    Some(frame) => frame,
                    None => fail!(),
                };
                if frame.count < u64::from(frame.min) {
                    pc = frame.body;
                } else if frame.max.is_some_and(|max| frame.count >= u64::from(max)) {
                    state.repeats.pop();
                    pc = frame.after;
                } else if frame.greedy {
                    let mut alternative = state.clone();
                    alternative.repeats.pop();
                    stack.push(Frame {
                        pc: frame.after,
                        pos,
                        state: alternative,
                    });
                    pc = frame.body;
                } else {
                    stack.push(Frame {
                        pc: frame.body,
                        pos,
                        state: state.clone(),
                    });
                    state.repeats.pop();
                    pc = frame.after;
                }
            }
            Inst::RepeatEnd => {
                let mut frame = match state.repeats.pop() {
                    Some(frame) => frame,
                    None => fail!(),
                };
                frame.count += 1;
                if pos == frame.last_pos {
                    let may_continue = frame.count < u64::from(frame.min)
                        && frame.max.is_none_or(|max| frame.count < u64::from(max));
                    if may_continue {
                        state.repeats.push(frame);
                        pc = frame.loop_pc;
                    } else {
                        pc = frame.after;
                    }
                } else {
                    frame.last_pos = pos;
                    if frame.max.is_some_and(|max| frame.count >= u64::from(max)) {
                        pc = frame.after;
                    } else {
                        state.repeats.push(frame);
                        pc = frame.loop_pc;
                    }
                }
            }
            Inst::Look {
                kind,
                program,
                limit,
            } => match kind {
                LookKind::Ahead | LookKind::NotAhead => {
                    let seed = VmState {
                        captures: state.captures.clone(),
                        match_start: None,
                        match_end: None,
                        repeats: Vec::new(),
                    };
                    let result = run(ctx, *program, pos, seed, None, budget)?;
                    let positive = matches!(kind, LookKind::Ahead);
                    if result.is_some() == positive {
                        if let Some(outcome) = result {
                            state.captures = outcome.state.captures;
                        }
                        pc += 1;
                    } else {
                        fail!();
                    }
                }
                LookKind::Behind | LookKind::NotBehind => {
                    let lower = lookbehind_lower(ctx.haystack, pos, *limit);
                    let positive = matches!(kind, LookKind::Behind);
                    let mut found = None;
                    for candidate in (lower..=pos).rev() {
                        let seed = VmState {
                            captures: state.captures.clone(),
                            match_start: None,
                            match_end: None,
                            repeats: Vec::new(),
                        };
                        if let Some(outcome) =
                            run(ctx, *program, candidate, seed, Some(pos), budget)?
                        {
                            found = Some(outcome);
                            break;
                        }
                    }
                    if found.is_some() == positive {
                        if let Some(outcome) = found {
                            state.captures = outcome.state.captures;
                        }
                        pc += 1;
                    } else {
                        fail!();
                    }
                }
            },
            Inst::Atomic { program } => {
                let seed = VmState {
                    captures: state.captures.clone(),
                    match_start: state.match_start,
                    match_end: state.match_end,
                    repeats: Vec::new(),
                };
                match run(ctx, *program, pos, seed, None, budget)? {
                    Some(outcome) => {
                        // Keep any repeat frames that were active outside the
                        // atomic group; only the nested capture/boundary state
                        // is merged back.
                        state.captures = outcome.state.captures;
                        state.match_start = outcome.state.match_start;
                        state.match_end = outcome.state.match_end;
                        pos = outcome.pos;
                        pc += 1;
                    }
                    None => fail!(),
                }
            }
            Inst::Match => {
                if require_end.is_none_or(|end| pos == end) {
                    return Ok(Some(RunOutcome { pos, state }));
                }
                fail!();
            }
        }
    }
}

fn anchor_matches(anchor: Anchor, haystack: &[u8], pos: usize) -> bool {
    match anchor {
        Anchor::LineStart => pos == 0 || haystack[pos - 1] == b'\n',
        Anchor::LineEnd => pos == haystack.len() || haystack[pos] == b'\n',
        Anchor::WordStart => {
            pos < haystack.len()
                && is_word_byte(haystack[pos])
                && (pos == 0 || !is_word_byte(haystack[pos - 1]))
        }
        Anchor::WordEnd => {
            pos > 0
                && is_word_byte(haystack[pos - 1])
                && (pos == haystack.len() || !is_word_byte(haystack[pos]))
        }
        Anchor::TextStart => pos == 0,
        Anchor::TextEnd => pos == haystack.len(),
        Anchor::Never => false,
    }
}

/// Start of the current line and the line before it.  Vim documents this
/// bound for look-behind so that variable-length look-behind stays finite.
fn lookbehind_lower(haystack: &[u8], pos: usize, limit: Option<usize>) -> usize {
    let mut newlines = 0usize;
    let mut cursor = pos;
    let mut lower = 0usize;
    while cursor > 0 {
        cursor -= 1;
        if haystack[cursor] == b'\n' {
            newlines += 1;
            if newlines == 2 {
                lower = cursor + 1;
                break;
            }
        }
    }
    if let Some(limit) = limit {
        lower.max(pos.saturating_sub(limit))
    } else {
        lower
    }
}
