//! Lowering of the AST into a compact backtracking program.
//!
//! Programs are a flat instruction array with absolute jump targets.  Each
//! look-around (and \@>) gets its own subprogram in the same program table so
//! capture group numbers remain global.

use crate::ast::{Anchor, Ast, LookKind};
use crate::charclass::CharClass;

/// A compiled instruction.
#[derive(Debug, Clone)]
pub(crate) enum Inst {
    /// One literal byte, compared with optional ASCII case folding.
    Literal(u8),
    /// Any byte except a line break.
    Any,
    /// Any byte including a line break.
    AnyNewline,
    /// A character class.
    Class(CharClass),
    /// A zero-width boundary.
    Anchor(Anchor),
    /// Start of capturing group \`index\` (zero based storage).
    SaveStart(usize),
    /// End of capturing group \`index\`.
    SaveEnd(usize),
    /// \`\\zs\`.
    SetStart,
    /// \`\\ze\`.
    SetEnd,
    /// Back reference to group \`index\` (one based).
    Backref(usize),
    /// Try \`first\`, remember \`second\`.
    Split { first: usize, second: usize },
    /// Unconditional jump.
    Jmp(usize),
    /// Enter a repetition loop.
    RepeatStart {
        min: u32,
        max: Option<u32>,
        greedy: bool,
        body: usize,
        after: usize,
    },
    /// Decide whether to enter the body or leave the loop.
    RepeatLoop,
    /// Finish one iteration.
    RepeatEnd,
    /// A look-around assertion referring to a subprogram.
    Look {
        kind: LookKind,
        program: usize,
        limit: Option<usize>,
    },
    /// Match a subprogram once, without allowing it to be re-entered.
    Atomic { program: usize },
    /// Successful end of a program.
    Match,
}

/// One instruction sequence.  Index 0 in a Regex is the main program; the
/// remaining entries are subprograms for look-around and atomic atoms.
#[derive(Debug, Clone)]
pub(crate) struct Program {
    pub(crate) code: Vec<Inst>,
}

struct Compiler {
    programs: Vec<Program>,
}

/// Compile an AST, returning the program table.
pub(crate) fn compile(ast: &Ast) -> Vec<Program> {
    let mut compiler = Compiler {
        programs: vec![Program { code: Vec::new() }],
    };
    let mut code = Vec::new();
    compiler.compile_node(ast, &mut code);
    code.push(Inst::Match);
    compiler.programs[0] = Program { code };
    compiler.programs
}

impl Compiler {
    fn compile_node(&mut self, node: &Ast, code: &mut Vec<Inst>) {
        match node {
            Ast::Empty => {}
            Ast::Literal(byte) => code.push(Inst::Literal(*byte)),
            Ast::Any => code.push(Inst::Any),
            Ast::AnyNewline => code.push(Inst::AnyNewline),
            Ast::Class(class) => code.push(Inst::Class(*class)),
            Ast::Anchor(anchor) => code.push(Inst::Anchor(*anchor)),
            Ast::Concat(items) => {
                for item in items {
                    self.compile_node(item, code);
                }
            }
            Ast::Alt(branches) => {
                if branches.is_empty() {
                    return;
                }
                let mut jumps_to_end = Vec::new();
                for (index, branch) in branches.iter().enumerate() {
                    if index + 1 == branches.len() {
                        self.compile_node(branch, code);
                    } else {
                        let split_at = code.len();
                        code.push(Inst::Split {
                            first: 0,
                            second: 0,
                        });
                        let branch_start = code.len();
                        self.compile_node(branch, code);
                        let jump_at = code.len();
                        code.push(Inst::Jmp(0));
                        let next_branch = code.len();
                        code[split_at] = Inst::Split {
                            first: branch_start,
                            second: next_branch,
                        };
                        jumps_to_end.push(jump_at);
                    }
                }
                let end = code.len();
                for jump_at in jumps_to_end {
                    code[jump_at] = Inst::Jmp(end);
                }
            }
            Ast::Group { index, node } => {
                code.push(Inst::SaveStart(*index - 1));
                self.compile_node(node, code);
                code.push(Inst::SaveEnd(*index - 1));
            }
            Ast::Repeat {
                node,
                min,
                max,
                greedy,
            } => {
                let start = code.len();
                code.push(Inst::RepeatStart {
                    min: *min,
                    max: *max,
                    greedy: *greedy,
                    body: 0,
                    after: 0,
                });
                code.push(Inst::RepeatLoop);
                let body = code.len();
                self.compile_node(node, code);
                code.push(Inst::RepeatEnd);
                let after = code.len();
                code[start] = Inst::RepeatStart {
                    min: *min,
                    max: *max,
                    greedy: *greedy,
                    body,
                    after,
                };
            }
            Ast::Backref(index) => code.push(Inst::Backref(*index)),
            Ast::SetStart => code.push(Inst::SetStart),
            Ast::SetEnd => code.push(Inst::SetEnd),
            Ast::Look { kind, node, limit } => {
                let program = self.compile_subprogram(node);
                code.push(Inst::Look {
                    kind: *kind,
                    program,
                    limit: *limit,
                });
            }
            Ast::Atomic(node) => {
                let program = self.compile_subprogram(node);
                code.push(Inst::Atomic { program });
            }
        }
    }

    fn compile_subprogram(&mut self, node: &Ast) -> usize {
        let program = self.programs.len();
        self.programs.push(Program { code: Vec::new() });
        let mut code = Vec::new();
        self.compile_node(node, &mut code);
        code.push(Inst::Match);
        self.programs[program] = Program { code };
        program
    }
}
