# Roadmap and milestone gates

The roadmap is dependency-first, not calendar-first. Each milestone produces a
running editor, tests and a demo. Phase labels come from
[FEATURES.md](FEATURES.md); a milestone is complete only when its gate passes.

**Scope note:** the terminal emulator is descoped. There is no TERM milestone;
do not schedule <code>:terminal</code>, Terminal-Job mode or <code>term_*</code>.
Jobs/channels remain.

---

## 1. Milestones

### M0 — Foundation and contracts (P0 enablers)

**Goal:** make the architecture executable with fake content.

- Cargo workspace, crate graph, CI, MSRV.
- Core IDs/errors, registries, option table, event loop skeleton.
- <code>rvim-ui-api</code> frame/cell/attr model, <code>UiBackend</code> trait.
- Headless backend and golden-frame harness.
- CLI skeleton with <code>--headless</code>.
- Buffer line store + file I/O subset.
- Minimal TUI: raw mode, alt screen, resize, basic keys, frame present.

**Demo:** a hard-coded frame with text, a statusline and a cursor appears in the
TUI; the same frame is snapshot-tested headlessly.

**Gate:** <code>cargo test --workspace</code>; headless frame round-trip; TUI
starts and quits cleanly; no terminal left in raw mode on panic/exit.

---

### M1 — Walking skeleton (P0)

**Goal:** a genuinely usable bare editor.

- Normal mode: h/j/k/l, words, 0/$/gg/G, counts.
- Insert/append/open-line, basic special keys.
- Operators d/c/y/p, undo/redo, one undo block per action.
- One window, viewport, scrolling, tabs/spaces rendering.
- Ex: <code>:e</code>, <code>:w</code>, <code>:q</code>, <code>:wq</code>,
  <code>:q!</code>; command-line editor.
- Statusline with file/mode/position; cmdline; messages.
- File read/write, unix/dos line endings, UTF-8.

**Demo:** open a file, edit, undo, write, quit; diff the file against Vim for the
same key sequence.

**Gate:** P0 features in the catalog pass; differential key corpus for supported
motions/operators; no data-loss bug; file round-trip byte-exact for unix/dos.

---

### M2 — Daily driver (P1)

**Goal:** replace Vim for ordinary editing.

- Splits/tabs, window navigation, resize, statusline/tabline.
- Visual/Select modes, registers, marks, jumplist/changelist.
- Search/incsearch/hlsearch, <code>:s</code>, <code>:g</code>, <code>:normal</code>.
- Mappings, abbreviations, macros, dot repeat.
- Options/scopes/modelines, autocommands + filetype events.
- VimL core: values/GC, parser, functions, scopes, control flow, autoload.
- Vim regex core (backtracking reference), syntax highlighting basics.
- Runtime: <code>runtimepath</code>, filetype detection, ftplugin/indent/syntax
  loading for common languages, help/tags.
- Clipboard, change list, <code>:sort</code>, formatting basics.

**Demo:** edit a small Rust/Python project with syntax, splits, search/replace,
macros, autocommands and a user function; run <code>:help</code>.

**Gate:** P1 coverage; Tier A runtime smoke; differential tests for editing,
VimL core and regex; no crash on malformed input.

---

### M3 — Runtime Tier A and display parity

**Goal:** Vim's default runtime feels native.

- Filetype/ftplugin/indent for the top ~30 languages.
- Syntax corpus for the same set; incremental invalidation.
- Folds (manual/indent/expr/marker/syntax), signs, text properties basics.
- Diff mode and internal diff.
- Quickfix/location lists, errorformat, <code>:make</code>/<code>:grep</code>,
  compiler plugins.
- Tags/help completion, <code>:helpgrep</code>, preview window.
- Sessions/views, viminfo/shada, persistent undo.
- Insert completion (keyword, file, tags, omni) and cindent.
- Formatting/indent operators.

**Demo:** compile errors populate quickfix; jump between tags; diff two files;
fold a large source file; save/restore a session.

**Gate:** Tier A 100% pass; syntax corpus no errors; diff output matches Vim;
quickfix errorformat corpus; fold/sign/property APIs.

---

### M4 — Scripting depth and regex performance

**Goal:** runtime scripts and plugins run at usable speed.

- Full legacy VimL builtins by family (string/list/dict/file/system/time/JSON).
- VimL profiler and <code>:debug</code> basics.
- Regex NFA fast path, timeouts/limits, differential corpus.
- Syntax sync/incremental performance; <code>:syntime</code>.
- Vim9 runtime skip policy; strict audit mode.
- Runtime Tier B (netrw core, matchit, archive plugins).
- Popup windows, text properties virtual text/lines, advanced completion.

**Demo:** run a real Vim plugin (matchit/netrw subset) and a 1k-line VimL
function workload; syntax on a 100k-line file within budget.

**Gate:** regex corpus; VimL builtin coverage; plugin smoke; performance budgets
for startup/render/syntax/regex.

---

### M5 — QuickJS and jobs

**Goal:** modern automation without compromising Vim compatibility.

- QuickJS build/runtime/limits/interrupt.
- <code>:js</code>/<code>:jsdo</code>/<code>:jsfile</code>, <code>vim</code> API,
  conversion, function bridge.
- Modules, JS plugin loading, promises/microtasks.
- Jobs/channels/timers (PTY for jobs; no terminal emulator).
- <code>js_eval</code>/<code>js_call</code> bridge and VimL callbacks.
- Sandbox/restricted mode.
- Runtime Tier C (everything not excluded); Vim9/terminal gaps classified.

**Demo:** a JS plugin defines a command/autocmd, edits buffers, runs a job and
awaits it; a VimL function calls a JS callback; memory/interrupt limits tested.

**Gate:** JS API contract; GC/shutdown; promise/job interleaving; Tier C matrix
with no unexpected failures; QuickJS disabled build passes core tests.

---

### M6 — Hardening and optional features

**Goal:** release-quality.

- Encryption (blowfish2/zip), security/fuzzing, DoS tests.
- Printing/RTL/client-server/sound, only if selected.
- Platform polish (macOS/Linux; Windows if targeted).
- Performance tune, memory bounds, large-file tests.
- Documentation, packaging, runtime update process.
- Vim test-suite adaptation and release matrix.

**Gate:** security review; fuzz targets clean; Tier A/B regressions zero; all
known gaps documented; reproducible runtime manifest.

---

## 2. Dependency-ordered slices

The milestones are vertical slices. Within each, work by dependency:

~~~text
cell/frame + backend
   -> buffer + viewport
      -> input/mode/operators
         -> ex + VimL
            -> options/autocommands/runtime
               -> syntax/folds/diff/quickfix
                  -> jobs/QuickJS
~~~

Parallel tracks once contracts exist:

- **UI track:** frame protocol, TUI, statusline/tabline/cmdline widgets.
- **Text track:** line tree, undo, marks, properties.
- **Script track:** VimL values/GC/parser/evaluator, builtins.
- **Regex/syntax track:** regex parser/VM, syntax engine.
- **Runtime track:** path loader, filetype, ftplugin/indent, help/tags.
- **Jobs/JS track:** event loop, channels, QuickJS.

Each track must keep the headless editor green; no track owns a private
"temporary" architecture that later needs replacing.

---

## 3. First vertical slice (recommended)

If only one slice is built first, build this:

1. <code>rvim-types</code> + <code>rvim-ui-api</code> + headless backend.
2. <code>rvim-text</code> line buffer + file read/write.
3. <code>rvim-view</code> single window + statusline widget.
4. <code>rvim-editor</code> Normal/Insert dispatch + <code>d/y/p</code>.
5. <code>rvim-ex</code> parser + <code>:e/:w/:q</code>.
6. <code>rvim-tui</code> raw mode + key decoding.
7. Golden frame + differential key test.

This is M0+M1. It forces the UI contract, byte model, undo and command dispatch
to be real before syntax, VimL or QuickJS add pressure.

---

## 4. Release gates

A release is not "all features done". It is a set of measurable gates:

| Gate | Requirement |
|---|---|
| Core editing | P0/P1 selected features pass; differential corpus green |
| Runtime | Tier A 100%; Tier B/C classified; no unexpected errors |
| UI | TUI and headless frames agree; statusline/tabline/cmdline tests green |
| Scripting | VimL corpus; JS API (if built); GC stress clean |
| Security | sandbox, modeline, shell quoting, regex DoS, fuzz clean |
| Performance | startup/render/syntax/regex/JS within budgets |
| Build | QuickJS-on and QuickJS-off; macOS/Linux; no runtime edits |
| Docs | exclusions, matrix, known gaps, license notices updated |

---

## 5. Out of scope / not scheduled

- Terminal emulator (<code>:terminal</code>, Terminal-Job, <code>term_*</code>).
- Vim9 script (separate future milestone, see [EXCLUSIONS.md](EXCLUSIONS.md)).
- GUI backend (interface only; future crate).
- Vi/Ex compatibility mode.
- Neovim API, LSP, tree-sitter.

If any of these is re-approved, it gets its own milestone and catalog entries;
it does not get smuggled into a hardening milestone.

---

## 6. Estimating and tracking

Use the catalog, not intuition:

- Complexity is S/M/L/XL; it is relative effort, not days.
- Each milestone lists feature IDs; the selection tool produces the exact set:
  <code>python3 tools/select_features.py --phase P1 --with-deps</code>.
- Track status per feature (planned, in progress, done, blocked) in the issue
  tracker, keyed by ID.
- A feature is never "done" because code exists; the definition of done in
  [FEATURES.md](FEATURES.md#definition-of-done-per-feature) applies.

The roadmap is allowed to change. The gates and the catalog are what keep the
change honest.
