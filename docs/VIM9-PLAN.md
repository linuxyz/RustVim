# Vim9 script implementation plan

Status: **Phase 0/1 complete; Phase 2 (evaluator) in progress**.  Target: runtime-grade Vim9 (see "Goal" below).
Owner: lead agent; work is split into stages with hard gates.  Budget: ~100M
model tokens total, with a stop-loss rule per phase.

## Goal and acceptance criteria

Target **B**: a Vim9 implementation good enough that the pinned Vim 9.2.1338
runtime works with it, not a byte-for-byte clone of Vim's compiler.

Acceptance gates for the whole project (all must hold):

1. "has('vim9script')" returns 1 and Vim9 scripts are sourced normally,
   including imported autoload scripts.
2. The 19 remaining Tier C failures caused by "Vim9 script is not supported"
   pass, and so does Tier B "compiler/context.vim".
3. The runtime compatibility matrix does not regress: Tier A 14/14, Tier B
   48/48, Tier C 789/790 (the one remaining failure is syntax/glimmer.vim, a
   harness artifact that also fails standalone in Vim).
4. The legacy VimL engine keeps its existing test suite and behavior; it is
   not rewritten.
5. Unsupported Vim9 features are documented explicitly (no silent gaps).
6. cargo test, fmt, clippy (with -D warnings) and doc gates stay clean.

Non-goals (documented approximations, cut first if the budget runs out):
generics beyond simple declarations, interface inheritance details, exact
Vim diagnostic E-codes for invalid programs, ":disassemble", ":defcompile"
profiling, Vim9 debugger support, bytecode-level performance parity.

## Where we start from

- Vim's Vim9 implementation is ~34k lines of dense C in src/vim9*.c plus
  hooks in eval/typval; docs vim9.txt (6260 lines) and vim9class.txt (1368).
- The pinned runtime contains 114 Vim9 files (28659 lines); the 19 failing
  checks only use basic constructs (vim9script, var/const/final, typed def,
  interpolation, ->, lambdas, import/export) plus one basic class in
  runtime/syntax/2html.vim (public vars, constructor, methods, this, nested
  def) and an enum in termdebug (not loaded at startup).
- RVim has 73196 lines of Rust; rvim-viml (the legacy interpreter) is 13194.
  Vim9 currently dies at the single E319 gate in viml/src/script.rs.

## Architecture decisions

1. **AST interpreter with a static type-checking pass, not a bytecode VM.**
   Vim9 semantics require compile-time checks, not bytecode.  This saves the
   equivalent of vim9execute.c (8354 C lines) and keeps ":disassemble" out of
   scope.  A bytecode backend can be added later behind the same front end.
2. **Gradual, permissive type checker.**  Reject clear mismatches; fall back to
   dynamic behavior when the checker is unsure.  Exact Vim error codes for
   invalid programs are not a goal; accepting valid runtime files is.
3. **On-demand builtin signatures.**  Type signatures are declared for the
   builtins actually used by the runtime and tests; unknown builtins accept
   and return "any".  A complete table can be generated later.
4. **Isolation from the legacy engine.**  Everything lives under
   crates/rvim-viml/src/vim9/.  The legacy evaluator is only touched at one
   narrow boundary (calling legacy functions from Vim9 and vice versa), never
   restructured.
5. **The has('vim9script') flip is the last step.**  Until the runtime files
   and the matrix pass locally, the feature stays 0 so trunk stays green.
6. **Differential testing against Vim is the primary correctness tool.**  A
   corpus is executed under a real Vim oracle and under RVim; outputs are
   compared, not just exit codes.

## Module layout

    crates/rvim-viml/src/vim9/
        mod.rs        module wiring
        token.rs      tokens, punctuation, heredoc/interpolation payloads
        lexer.rs      tokenizer (incremental cursor + whole-file tokenize)
        ast.rs        surface AST (statements, expressions, types, classes)
        parser.rs     recursive-descent parser (line-oriented; Ex command
                      lines are captured verbatim instead of being tokenized)
        error.rs      Vim9 error helpers and E-codes
        value9.rs     Vim9 values (legacy Value + tuples), formatting
        eval.rs       AST evaluator, builtin dispatch, commands
        eval_helpers.rs  free helper functions used by the evaluator
        check.rs      static checks and type inference        (Phase 5)
        class.rs      classes/interfaces/enums               (Phase 4)
        import.rs     import/export and script namespaces    (Phase 3)
        tests_*.rs    unit tests per layer

Ex command lines inside Vim9 scripts ("set", "execute", "echomsg", ...) are
captured verbatim by the parser; execution routes them through the existing Ex
command dispatcher.

## Phases, deliverables and gates

Token budgets are estimates for this project (dense compiler work, including
tests and debugging); they sum to ~58-93M for target B.

### Phase 0 - design, skeleton, harness (3-6M) [complete]
- docs/VIM9-PLAN.md (this file).
- token.rs / ast.rs / error.rs / module skeleton committed.
- tools/vim9_diff.py: "parse" gate, "oracle" capture with real Vim,
  "diff" plumbing (activated in Phase 2), corpus under tools/vim9_corpus/.
- Gate: harness runs; corpus check/oracle modes pass; skeleton compiles with
  fmt/clippy clean; legacy tests unaffected.

### Phase 1 - lexer and parser (12-20M) [complete]
- Complete Vim9 lexer: numbers/floats/blobs, strings and $"" interpolation,
  heredocs, comments, scoped vars/options/env/registers, operators,
  autoload names, line continuation tokens.
- Recursive-descent parser: var/final/const (with list destructuring), def
  (typed/default/optional/varargs params, nested defs, generics),
  if/elseif/else, while, for (incl. destructuring), try/catch/finally,
  return/throw/break/continue, assignment, calls and method chains, lambdas,
  list/dict/tuple literals, type expressions, class/interface/enum/type
  declarations, import/export, raw Ex command lines.
- Gate: every corpus file parses (negative files fail with the right kind of
  error); Vim-valid files in the corpus also parse; unit tests cover each
  construct class; fmt/clippy/test clean; matrix unchanged (feature still 0).

### Phase 2 - checks and evaluator (25-35M) [complete]
- Variable scopes and immutability (final/const, script-local locking),
  function calls (Vim9 def, legacy function, builtin, funcref/partial),
  closures incl. nested def capture, list/dict/tuple values, string
  interpolation, heredoc eval, command capture execution, control flow,
  try/catch, compare/convert semantics, Vim9 expression rules (no implicit
  string/number conversion in the legacy way, method call dispatch).
- Static pass: declaration types, assignment/call/return checking for
  annotated types, void handling; permissive fallback.
- Builtin signature table for the builtins used by the corpus and runtime.
- Gate: corpus executes with outputs equal to the Vim oracle (diff mode);
  Vim's own test_vim9_*.vim subset has a documented pass rate and no panics;
  legacy suite green; matrix unchanged.
  Phase gate met: all 26 oracle-checked corpus cases pass byte for byte
  (p27 is no-oracle).  Classes, enums and imports were brought into the
  evaluator early to complete the corpus; only the editor-side script-loader
  integration is left for Phase 3.  Vim's own test_vim9_*.vim subset and the
  strict static type checker remain Phase 5 hardening.

### Phase 3 - script loader, import/export, autoload (8-12M) [complete]
- "vim9script" header handling in the script loader, script namespaces,
  export tables, import path resolution (relative, runtimepath, the 9.2
  "import/" directory, "import autoload" deferral), script-local lifetime,
  mixing rules with legacy (:vim9cmd, :legacy, scriptversion), error E319
  removal.
- Gate: the runtime Vim9 scripts load and imports resolve; Tier A/B/C run with
  the feature enabled locally; no regression.
  Gate met: Vim9 sourcing, vim9-mix preambles, Ex command dispatch (syntax,
  setlocal, autocmd, normal, runtime, source), legacy-to-Vim9 bridging,
  imports (relative/runtimepath/import/autoload) and the feature flip work:
  the runtime Vim9 scripts load and the matrix is A 14/14, B 48/48, C 789/790.

### Phase 4 - classes, interfaces, enums (8-14M) [complete]
- Member declarations/access (public/private/static/abstract), constructors,
  this/super, methods, objects, nested defs capturing this, class variables,
  abstract classes, interfaces (declaration and conformance as far as the
  runtime needs), enums, type aliases.
- Gate: runtime/syntax/2html.vim (ProgressBar class) executes identically to
  the oracle; class corpus passes.  The corpus class/enum/interface cases
  (p17-p20, p23, p26) now pass byte for byte; super member access is not
  needed by the corpus or runtime files and remains a documented gap.

### Phase 5 - diagnostics, Vim test subset, hardening (8-15M) [in progress]
- Remaining type checks and Vim-like diagnostics needed by the test corpus;
  :defcompile; :disassemble (optional, best effort); profiling out of scope.
- Run Vim's own test_vim9_*.vim subset headless through the differential
  harness; triage failures into fixes vs documented gaps.
- Gate: no panics/fuzz findings in the parser/evaluator; all success criteria
  above met; docs updated.

### Phase 6 - integration and release (5-10M)
- Flip has('vim9script'); run the full compat matrix; fix fallout; update
  docs/STATUS.md, docs/COMPATIBILITY.md, README status, feature list.
- Gate: final matrix A 14/14, B 48/48, C 789/790; CI green; docs updated.

## Testing strategy

- Unit tests per layer (lexer, parser, later checker/evaluator/classes).
- tools/vim9_diff.py:
  - "parse": corpus parse gate against the rvim-viml example binary.
  - "oracle": record deterministic outputs from real Vim into
    tools/vim9_corpus/expected/.
  - "diff": after Phase 2, execute the corpus with target/debug/rvim and
    compare with the recorded oracle output.
- Vim's own test_vim9_script.vim etc. are a reference, not the gate; a
  documented subset becomes a regression suite in Phase 5.
- The runtime compatibility matrix stays the top-level gate; every phase ends
  with A/B/C numbers recorded in the commit message.

## Risk register / stop-loss

- **Type inference is the budget sink** (vim9expr.c is 4271 C lines and the
  most failure-prone logic).  Mitigation: permissive checker, no exact
  diagnostics, cut generics/interface checks first.
- **has('vim9script') fallout**: enabling it activates runtime capability
  branches, exactly like the plugin-loading regression earlier.  Mitigation:
  flip locally at the end of Phase 3, run the full matrix, keep the flip
  revertable; never land the flip with the matrix red.
- **Import resolution subtleties** ("./", runtimepath, import/ dir,
  autoload).  Mitigation: differential tests against Vim for each form.
- **Long-horizon consistency**: stages are separate context windows.
  Mitigation: this document, commit-level progress notes, harness gates, and
  no cross-phase refactors without a note here.
- **Budget stop-loss**: if a phase exceeds 1.5x its estimate, cut features to
  the "minimum that passes the phase gate", record the gap in
  docs/STATUS.md, and move on.

## Documented approximations to carry into docs/STATUS.md

- No bytecode/profiling/:disassemble (unless Phase 5 finds cheap wins).
- Permissive type checking: some invalid Vim9 programs may be accepted, and
  some diagnostic E-codes differ from Vim.
- Generics/interface conformance limited to what the runtime and tests need.
- Only the two-way subset of Vim9 features exercised by the runtime is
  guaranteed; exotic corner cases (deep variadic tuples, quoted heredoc
  markers, key-code edge cases) may be approximate.
- The 9.2 "import/" directory convention is implemented; behavior under
  exotic 'runtimepath' mutations may differ.

## Progress log

- 2026-11-15: Phase 0 and Phase 1 complete.
  - docs/VIM9-PLAN.md, docs/VIM9-ORACLE-NOTES.md, tools/VIM9_HARNESS.md.
  - crates/rvim-viml/src/vim9/: token.rs, ast.rs, error.rs, lexer.rs,
    parser.rs, module wiring, unit tests (parser tests in tests_parser.rs).
  - tools/vim9_diff.py + tools/vim9_corpus (27 positive, 12 negative,
    support modules, 26 recorded Vim oracle outputs) + VIM9_HARNESS.md.
  - Parser coverage: every corpus file parses (negatives rejected with the
    expected error kind); all 65 real Vim9 runtime files in the pinned 9.2
    runtime parse (mixed legacy preambles are detected and skipped by the
    front end; the loader split is Phase 3).
  - Language constructs handled: vim9script (incl. noclear), comments,
    numbers/floats/blobs with digit separators, strings, $"" interpolation,
    heredocs (trim/eval), var/final/const with destructuring and per-item
    types, if/elseif/else (incl. single-line "if c | stmt | endif" bodies),
    while/for (incl. inline), try/catch/finally with regex catch patterns,
    return/throw/break/continue/defer, def with typed/default/optional/
    varargs/generic params, nested def, script-level conditional def,
    def g:Name, lambdas (all forms, expression and block bodies), method
    chains, ?? falsy operator, lists/dicts/tuples, subscripts/slices, class
    (public/private/static/abstract, constructor, this, nested def),
    interface (no :enddef), enum (constructor arguments, member body/var,
    constructor), type aliases, import/export, raw Ex command capture with
    backslash/bar continuations.
  - Gates: 615 workspace tests, fmt/clippy/rustdoc clean; runtime compatibility
    matrix unchanged (the feature flag is still 0; evaluation is next).
- 2026-11-15: Phase 2 evaluator landed for the base language.
  - crates/rvim-viml/src/vim9/value9.rs (legacy Value + tuples and Vim9
    formatting), eval.rs (scopes, closures, def/lambda calls, control flow,
    try/catch/finally, Vim9 operators, builtin dispatch, higher order builtins
    map/mapnew/filter/reduce/foreach/any/all/sort with closures, command
    execution for echo/echomsg/execute/set/normal!/call, heredoc trim+eval).
  - Call-site generic arguments (First<number>(...)) are parsed; ==?/!=?/=~?/!~?
    case-insensitive comparators implemented.
  - tools/vim9_diff.py diff mode runs crates/rvim-viml/examples/vim9_run.rs
    and compares expected/*.out byte for byte.  Result now: 19 passed,
    0 failed, 8 skipped (7 later-phase class/import/enum cases, 1 no-oracle).
  - Evaluator unit tests cover formatting, operators, closures, higher order
    builtins, destructuring, try/catch, heredocs, generic calls, Ex commands
    and const locking.
  - Still deliberately permissive: no Vim9 static type checker yet; that moves
    to Phase 5 hardening (documented approximation).

- 2026-11-15: Phase 2 complete and evaluator-side Phase 3/4 landed.
  - eval.rs now has script frames with export tables, relative/runtimepath
    import resolution and a cyclic-import guard; imported functions/classes/
    enums/constants resolve through namespace dictionaries.
  - Runtime class model: instance/static variables, constructors (new),
    methods with this, inheritance, abstract classes, virtual dispatch,
    static methods, nested defs capturing this, and typed-but-uninitialized
    variable defaults.
  - Runtime enum model: members with constructor arguments and fields,
    .name/.ordinal, body variables and new(this.member, ...) mapping.
  - Parser: raw Ex command continuations starting with .. / -> / operators
    are merged (needed by echomsg expressions spanning lines).
  - Differential gate: 26 passed, 0 failed, 1 skipped (p27 is no-oracle).
  - New unit tests: classes/static members, abstract classes + enums,
    relative imports (temp script pair).  Total workspace tests: 629.
  - Remaining for Phase 3: editor-side sourcing of vim9script files, Ex
    command dispatch from Vim9 (so syntax files can run), vim9-mix preamble
    execution, and then the has(vim9script) flip in Phase 5.
- 2026-11-15: Phase 3/4 complete, feature flip and compatibility gate met.
  - Editor script loader: vim9script header detection (including column-0
    headers after legacy code in vim9-mix files), legacy preamble execution,
    Vim9 execution against the real editor host (Ex/syntax/setlocal/runtime/
    source/normal dispatch with the interpreter runtime installed temporarily
    so both engines share globals), :import/export and autoload loading.
  - Legacy interop: mixed-mode def/def! blocks are skipped, legacy :import
    binds Vim9 exports as stub namespaces, legacy :runtime/:source/autoload
    dispatch Vim9 files through the loader, dotted dictionary functions work,
    unlet of locked variables is allowed.
  - Runtime fixes found by the matrix: column-0 header scan, whole-scope b:/
    g:/v: dictionaries, option aliases (vts/ts/...), has('vartabs') and
    has('termguicolors') true, getline(start,end) always returns a list,
    setline grows empty buffers, range-only Ex commands move the cursor,
    chained search addresses start from the previous address, append('.')
    resolves the cursor line, '!' substitute delimiter and Vim9 comment
    stripping, list concatenation, the exe abbreviation, and generated
    def/enddef blocks from :execute.
  - Matrix: A 14/14, B 48/48, C 789/790 (only the documented glimmer
    artefact); Vim9 differential corpus 26/26; workspace 629 tests;
    fmt/clippy/rustdoc clean.
  - Remaining documented approximations: no strict static checker, legacy
    import stubs, mixed def skipping, no cross-source Vim9 callable state, no
    super member access, generics/interface conformance permissive.

- 2026-11-15: :global/:vglobal (multi-repeat) fixes.
  - :g! and :v now invert correctly; :g sets last search and substitute pattern;
    invalid/missing pattern delimiters report E146/E148/E35; recursive :global
    is limited to the current line (E147); per-line errors are reported and
    processing continues; :normal leaves insert mode before the next match.
  - Line marks are adjusted for delete/move/copy so :g/pat/m$, :g/pat/t$ and
    mixed delete/substitute commands stay aligned; the whole :global command
    is a single undo step.
  - Substitutes that match at end-of-line (s/$/x/) no longer duplicate text.
  - Regression tests added in crates/rvim-editor/tests/basic.rs (workspace now
    635 tests); differential checks against Vim cover delete/inverse, ranges,
    delimiters, nesting, move/copy, :normal and last-pattern semantics.
