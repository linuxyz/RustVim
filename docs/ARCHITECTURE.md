# RVim architecture

**Status:** design baseline. This document describes the target architecture for
the features selected from [FEATURES.md](FEATURES.md); it is not a report on an
existing implementation.

**Target:** Vim 9.2.1338 behavior, Rust, terminal UI first, GUI-ready
interfaces, legacy Vim script, QuickJS as an extension language.

---

## 1. Goals, non-goals and constraints

### Goals

1. **Behavioral compatibility.** Match Vim's observable behavior: buffer text,
   cursor positions, registers, options, messages, Ex semantics, script values,
   runtime loading order. Source-level similarity to Vim's C code is irrelevant.
2. **A backend-neutral presentation layer.** The editor decides what every cell
   contains. TUI and GUI are renderers, not owners of editor policy.
3. **A complete Vim experience, not a subset clone.** The catalog covers the
   documented surface; features are gated explicitly when deferred.
4. **Vim runtime as the compatibility oracle.** Run the shipped Vim 9.2
   <code>runtime/</code> tree; do not fork it.
5. **A real extension language.** Legacy Vim script is required for runtime
   compatibility; QuickJS gives new plugins a modern language with access to
   the same editor API.
6. **A testable core.** A headless backend can drive the whole editor and
   compare cell frames and model state deterministically.

### Non-goals (this phase)

- **GUI implementation.** We define the GUI contract and keep the core
  backend-neutral; no winit/wgpu window in this phase.
- **Vim9 script.** Deferred. <code>vim9script</code> files are detected and
  skipped by default with an explicit warning; <code>has('vim9script')</code>
  is false.
- **Vi/Ex compatibility.** No <code>'compatible'</code> behavior, no
  <code>-e</code>/<code>-E</code> Ex mode, no Vi defaults, no vi binary. Vim's
  Ex command language is still in scope.
- **Terminal emulator.** <code>:terminal</code>, Terminal-Job mode and
  <code>term_*</code> functions are descoped. The TUI still needs to decode the
  physical terminal's input; that is not the same as emulating one.
- **Neovim API/UI protocol.** We design our own smaller protocol; no attempt to
  be wire-compatible with Neovim.
- **A built-in LSP client, tree-sitter, or AI features.** These are not Vim
  9.2 features and would distort compatibility priorities.

### Hard constraints

| Constraint | Consequence |
|---|---|
| tabline, statusline and cmdline are not UI-bound | They are view widgets in <code>rvim-view</code>, rendered into the same cells as buffer text; they never import <code>rvim-tui</code> or a GUI crate. |
| Window implementation belongs with the UI layer | <code>Window</code>, <code>TabPage</code>, layout and compositing live in the presentation layer; the buffer model only exposes text/marks/options. |
| GUI is deferred | <code>UiBackend</code> and <code>UiCapabilities</code> are the only GUI seam; <code>has('gui')</code> is false. |
| QuickJS is integrated | A first-class <code>vim</code> JS API, JS callbacks, modules and event-loop integration. |
| Vim runtime compatibility | Directory semantics, load order, filetype/ftplugin/indent/syntax/colors/compiler, help tags and packages. |
| No Vi/Ex compatibility | Vim defaults and Vim command semantics are the only target. |

---

## 2. Design principles

### 2.1 Three layers, one direction

~~~text
+---------------------------------------------------------------+
|  Backends: rvim-tui (now), rvim-gui (future), rvim-headless   |
|  - read Frame, write UiEvent                                  |
|  - know nothing about Vim commands, options or buffers         |
+---------------------------^-----------------------------------+
                            | Frame / UiEvent
+---------------------------+-----------------------------------+
|  Presentation: rvim-view                                      |
|  - windows, tab pages, layout, viewports, folds display       |
|  - statusline, tabline, cmdline, messages, popup menu         |
|  - compositor -> Frame of cells                               |
|  - UI-agnostic, but owns the screen model                     |
+---------------------------^-----------------------------------+
                            | read-only view of model + commands
+---------------------------+-----------------------------------+
|  Editor core: buffers, undo, options, registers, marks,       |
|  modes, Ex, VimL, regex, syntax, runtime, jobs, QuickJS       |
|  - no cells, no terminal, no pixels                            |
+---------------------------------------------------------------+
~~~

Dependencies point upward only. <code>rvim-view</code> does not depend on
<code>rvim-tui</code> or <code>rvim-gui</code>. <code>rvim-editor</code> does not
depend on any backend. <code>rvim-tui</code> does not depend on
<code>rvim-editor</code>; the binary wires them together.

### 2.2 Byte fidelity over Rust idioms

Vim is byte-oriented. A line is a byte string that may contain invalid UTF-8 and
NUL; a cursor column is a byte index; <code>col()</code> returns a byte column;
regex offsets are byte offsets. Rust's <code>String</code>/<code>char</code>
APIs are used only where Vim also works in characters (display width, character
motions). Core types are <code>VimString</code> (byte string) and
<code>BufPos { lnum: u32, col: u32 }</code>.

### 2.3 A single-threaded editor core

All editor state, Vim script, QuickJS and syntax evaluation run on one thread.
This removes data races, makes Vim's reentrancy semantics tractable, and lets
callbacks observe a consistent editor. Background threads are limited to I/O
reactors and process reaping; they deliver events into a queue and never touch
editor state.

### 2.4 Registry-driven Vim surface

Commands, options, functions, events and <code>has()</code> flags are metadata
tables, not ad-hoc match statements. The same table drives parsing, completion,
<code>:help</code> links, the inventory audit and tests. Adding a Vim function
means adding one spec and one handler; forgetting metadata becomes a test
failure.

### 2.5 Runtime is data, not code

<code>runtime/</code> is pinned, unmodified Vim data. If a runtime file fails,
the engine is wrong or a documented feature is missing. We never patch runtime
scripts to make RVim pass.

### 2.6 Explicit gates, honest answers

A deferred feature must:
- return false from <code>has()</code>/<code>exists()</code> where Vim would
  report availability,
- fail with a documented <code>E###</code>-style error where a command is
  invoked,
- appear in the compatibility matrix with a reason,
- never silently produce a wrong buffer or option.

---

## 3. Crate graph

The workspace is deliberately split at testable seams. "Crate or module" is an
implementation choice; the dependency direction is the contract.

| Crate | Responsibility | May depend on |
|---|---|---|
| <code>rvim-types</code> | IDs, <code>VimString</code>, <code>VimError</code>, version/feature flags, primitives | std only |
| <code>rvim-ui-api</code> | <code>Frame</code>/<code>Grid</code>/<code>Cell</code>/<code>Attr</code>, <code>UiBackend</code>, <code>UiEvent</code>, capabilities, clipboard ops | <code>rvim-types</code> |
| <code>rvim-platform</code> | filesystem, process, PTY, signals, terminal size, clipboard, locale, environment | <code>rvim-types</code> |
| <code>rvim-text</code> | line tree, buffer storage, marks, text properties, undo transactions, listeners | <code>rvim-types</code> |
| <code>rvim-regex</code> | Vim regex parser, compiler, backtracking VM, NFA fast path, match spans | <code>rvim-types</code> |
| <code>rvim-viml</code> | values/GC, lexer/parser, evaluator, pure builtins, <code>VimHost</code> trait | <code>rvim-types</code> |
| <code>rvim-ex</code> | Ex lexer/parser, ranges/addresses, command AST, no handlers | <code>rvim-types</code> |
| <code>rvim-edit</code> | motions, operators, text objects, registers, mode state machines | <code>rvim-types</code>, <code>rvim-text</code>, <code>rvim-regex</code> |
| <code>rvim-syntax</code> | highlight table, syntax items, incremental line state, match cache | <code>rvim-types</code>, <code>rvim-text</code>, <code>rvim-regex</code> |
| <code>rvim-runtime</code> | runtimepath discovery, <code>:runtime</code> lookup, filetype/ftplugin/syntax file resolution, help tag index | <code>rvim-types</code>, <code>rvim-platform</code> |
| <code>rvim-js</code> | QuickJS runtime, <code>vim</code> API, conversion, modules, promise/event integration | <code>rvim-types</code>, <code>rvim-viml</code> |
| <code>rvim-view</code> | windows, tabs, layout, viewports, statusline/tabline/cmdline widgets, compositor, popup menu, messages | <code>rvim-types</code>, <code>rvim-ui-api</code>, <code>rvim-text</code>, <code>rvim-syntax</code>, <code>rvim-regex</code> |
| <code>rvim-editor</code> | orchestration: buffers, options, commands, autocommands, scripts, jobs, quickfix, tags, spell, editor APIs | all except backends |
| <code>rvim-tui</code> | raw terminal, terminfo, input decoding, clipboard provider, present/event loop | <code>rvim-types</code>, <code>rvim-ui-api</code>, <code>rvim-platform</code> |
| <code>rvim-headless</code> | deterministic cell backend and snapshot tooling | <code>rvim-types</code>, <code>rvim-ui-api</code> |
| <code>rvim-cli</code> | binary <code>rvim</code>, args, startup, wiring, <code>--headless</code> | <code>rvim-editor</code>, <code>rvim-tui</code>, <code>rvim-headless</code> |
| <code>rvim-gui</code> (future) | winit/wgpu or native toolkit backend | <code>rvim-types</code>, <code>rvim-ui-api</code>, <code>rvim-platform</code> |

### Dependency rules (enforced in CI)

1. <code>rvim-ui-api</code> has no terminal, GUI, editor or filesystem dependency.
2. <code>rvim-view</code> has no <code>rvim-tui</code>/<code>rvim-gui</code>/<code>rvim-editor</code> dependency. It reads the model through a <code>RenderHost</code> trait defined in <code>rvim-view</code>; <code>rvim-editor</code> implements it.
3. <code>rvim-tui</code>/<code>rvim-gui</code> have no dependency on <code>rvim-editor</code>, <code>rvim-viml</code> or <code>rvim-ex</code>. They must not resolve a Vim command.
4. <code>rvim-editor</code> never links a backend; <code>rvim-cli</code> is the only crate that knows both.
5. <code>rvim-viml</code> defines <code>VimHost</code>; <code>rvim-editor</code> implements it. Pure builtins live in <code>rvim-viml</code>; editor-dependent builtins live in <code>rvim-editor</code> and are registered into the same function table.
6. <code>rvim-ex</code> parses but does not execute; handlers live in <code>rvim-editor</code>.
7. No cyclic crate dependencies; <code>cargo deny</code>/<code>cargo tree</code> checks in CI.

### Why these seams

- **Testability:** text, regex, VimL and syntax can be tested without a UI.
- **UI independence:** a rule can fail at compile time, not at a review.
- **Parallel work:** UI protocol, VimL, regex and runtime can progress independently once trait boundaries settle.
- **Future GUI:** a new backend adds a crate, not editor changes.

---

## 4. Core state and ownership

### 4.1 The editor

~~~rust
pub struct Editor {
    pub ids: IdGenerator,
    pub buffers: Arena<Buffer>,
    pub view: ViewState,              // rvim-view: windows, tabs, layout
    pub globals: GlobalState,         // registers, global marks, jumplist, ...
    pub options: OptionStore,
    pub scripts: ScriptRegistry,
    pub viml: VimRuntime,             // values, functions, autoload state
    pub js: Option<JsRuntime>,        // behind the quickjs feature
    pub runtime: RuntimePaths,        // rtp, $VIMRUNTIME, package cache
    pub messages: MessageState,
    pub cmdline: CmdlineState,
    pub mappings: MappingState,
    pub autocommands: AutocmdState,
    pub quickfix: QuickfixState,
    pub tags: TagState,
    pub spell: SpellState,
    pub jobs: JobState,
    pub history: HistoryState,
    pub last_error: Option<VimError>,
    pub redraw: RedrawState,
}
~~~

The editor is not a global singleton. Tests construct independent editors; the
CLI owns exactly one. IDs are generational newtypes, never raw indices.

### 4.2 Buffers

~~~rust
pub struct Buffer {
    pub id: BufferId,
    pub lines: LineTree,              // chunked line tree, stable LineId
    pub meta: BufferMeta,             // name, flags, changedtick, fileinfo
    pub undo: UndoTree,
    pub marks: BufferMarks,           // a-z, [ ] < >, etc.
    pub props: TextPropStore,
    pub syntax: SyntaxCache,
    pub options: LocalOptions,        // buffer-scoped option overrides
    pub vars: VimDict,                // b:
    pub listeners: Vec<ListenerId>,
}
~~~

- <code>LineTree</code> stores lines in chunks (target 64-256 lines) with stable
  line IDs. It provides line count, line lookup by number, byte access and
  O(log n) insert/delete. The exact structure is hidden behind a
  <code>BufferStore</code> trait so the first implementation can be a simple
  <code>Vec&lt;Line&gt;</code> if needed, then upgraded.
- Line text is <code>VimString</code> (bytes). Embedded NUL is a line break in
  Vim's internal representation; the storage layer exposes the distinction and
  the file I/O layer converts to/from the file format.
- Marks, signs and text properties store stable positions (<code>LineId</code>
  + byte column), not fragile line numbers. Public APIs convert to line numbers.
- <code>changedtick</code> increments on every text mutation; syntax and
  statusline caches key off it.

### 4.3 Windows and tabs

Windows are in the presentation layer, but the editor owns the
<code>ViewState</code> container. A window holds a <code>BufferId</code>, not a
borrowed buffer:

~~~rust
pub struct Window {
    pub id: WindowId,
    pub buffer: BufferId,
    pub viewport: Viewport,           // topline/leftcol/skipcol/curswant
    pub cursor: BufPos,
    pub folds: FoldState,
    pub options: WindowOptions,       // window-scoped overrides
    pub vars: VimDict,                // w:
    pub statusline: StatuslineCache,
}
~~~

There is exactly one cursor per window, so the same buffer can be displayed in
two windows with independent cursors, folds and scroll positions, as in Vim.
Tab pages own a window tree; the current tab/window is editor state. See
[WINDOWS.md](WINDOWS.md).

### 4.4 Borrow splitting for rendering

Rendering needs mutable view state (caches, dirty flags) and read access to the
model. Avoid <code>Rc&lt;RefCell&lt;Editor&gt;&gt;</code>. <code>Editor</code>
exposes a split:

~~~rust
impl Editor {
    pub fn split_for_render(&mut self) -> (&mut ViewState, RenderContext<'_>) {
        // disjoint field borrows: view, then buffers/options/syntax/...
        (&mut self.view, RenderContext {
            buffers: &self.buffers,
            options: &self.options,
            syntax: &self.syntax,
            globals: &self.globals,
        })
    }
}
~~~

<code>RenderContext</code> implements <code>rvim-view::RenderHost</code>. If a
widget needs to mutate model state (a statusline expression calls a function),
evaluate it in a separate phase with <code>&amp;mut Editor</code> and pass the
resulting segments to the render phase. This two-phase rule prevents aliasing
and makes expensive expressions explicit.

### 4.5 Global state

<code>GlobalState</code> holds:

- registers (<code>"</code>, <code>0-9</code>, <code>a-z</code>,
  <code>A-Z</code>, <code>-</code>, <code>:</code>, <code>.</code>,
  <code>%</code>, <code>#</code>, <code>=</code>, <code>_</code>),
- global marks and the alternate file,
- jumplist and changelist,
- argument list (global/local),
- search pattern/direction and last substitute,
- current working directory (with window/tab overrides),
- command-line and search history,
- <code>v:oldfiles</code> and session state.

All of it is owned by <code>Editor</code>; there are no mutable statics.

---

## 5. Input, command and render pipeline

### 5.1 One keystroke

~~~text
UiBackend::poll_event
  -> UiEvent::Key(KeyEvent)
  -> InputDecoder: KeyEvent -> Vim key notation (KeyCode)
  -> MappingEngine: expand mappings, honor timeout/ttimeout/paste
  -> TypeaheadBuffer: counts, register prefix, pending operator
  -> ModeDispatcher (Normal/Insert/Visual/Select/Cmdline/Terminal)
  -> Action (Motion | Operator | TextObject | ExCommand | InsertText | ...)
  -> EditTx (undo block, marks, listeners, changedtick)
  -> buffer + view mutation
  -> autocommands (TextChanged, TextChangedI, CursorMoved, ...)
  -> redraw scheduler
~~~

The pipeline is synchronous. Asynchronous events (job output, timer, resize)
enter through the same queue and are processed at safe points between actions;
they never preempt a Vim script call or a QuickJS call.

### 5.2 Edit transactions

Every mutation that should be one undo step goes through <code>EditTx</code>:

~~~rust
pub struct EditTx<'a> {
    buf: &'a mut Buffer,
    undo: UndoBlock,
    changed: ChangedRange,
    marks: MarkAdjust,
    props: PropAdjust,
}

impl EditTx<'_> {
    pub fn replace(&mut self, range: Range, text: &VimString) -> Result<()>;
    pub fn insert_line(&mut self, lnum: u32, text: &VimString) -> Result<()>;
    pub fn delete_lines(&mut self, range: Range) -> Result<Vec<VimString>>;
    pub fn commit(self) -> EditEffects;   // listeners, changedtick, redraw
}
~~~

Operators, insert mode, Ex line commands and VimL <code>setline()</code> all
use the same transaction type. This is where Vim-compatible undo blocks, mark
adjustment and text-property movement are enforced.

### 5.3 Rendering

~~~text
Editor::render:
  0. Fire redraw on_start listeners
  1. ViewState::layout(screen_size)              (window tree -> rects)
  2. For each window: render buffer text         (syntax spans, folds, wrap)
  3. Gutters, tabpanel and separators
  4. Statuslines and tabline                     (core widgets, 1..N rows)
  5. Cmdline, messages, popup menu, popups       (core widgets)
  6. Compositor: z-order, clipping, cursor       -> Frame
  7. Backend::present(Frame)
  8. Fire redraw on_end listeners
~~~

Steps 1-6 are in <code>rvim-view</code> and use only <code>rvim-ui-api</code>
cell types. The backend receives a complete frame and has no Vim knowledge.
Statusline, tabline and cmdline are produced by the same pipeline, which is the
structural guarantee that they are not UI-bound. Redraw listeners wrap the
pipeline; they are callbacks, not a second rendering path.

### 5.4 Redraw scheduling

- Mutations set a dirty bit and a damage rectangle; rendering is coalesced until
  the input queue is empty or a frame deadline is reached.
- <code>'lazyredraw'</code> defers redraw during macros/scripts;
  <code>:redraw</code> forces a flush.
- Cursor-only changes avoid rerendering unchanged text when possible.
- <code>redraw_listener_add()</code> callbacks run at the start/end of a redraw
  on the main thread; they cannot add listeners recursively or force a nested
  redraw.
- The headless backend can render on demand without a scheduler.

---

## 6. Registries: the Vim surface as data

### 6.1 Ex commands

~~~rust
pub struct ExCommandSpec {
    pub name: &'static str,          // "substitute"
    pub min_abbrev: &'static str,    // "s"
    pub flags: ExFlags,              // BANG, RANGE, COUNT, REGISTER, BAR, ...
    pub addr_type: AddrType,         // none, line, buffer, window, ...
    pub min_args: u8,
    pub max_args: u8,                // u8::MAX for "any"
    pub completion: CompletionKind,
    pub handler: ExHandler,
    pub doc_tag: &'static str,
    pub since: Version,
}

pub type ExHandler = fn(&mut Editor, &ExContext) -> Result<ExFlow, VimError>;
~~~

The table is sorted by Vim's command order. Abbreviation resolution uses the
explicit minimum abbreviation, not a generic prefix rule, because Vim has
special cases (<code>:s</code> is substitute, not set). The same table powers
<code>:command</code> completion and the inventory appendix.

### 6.2 Options

~~~rust
pub struct OptionSpec {
    pub name: &'static str,
    pub short: Option<&'static str>,
    pub scope: OptionScope,          // Global, Buffer, Window, Tab, GlobalLocal
    pub typ: OptionType,             // Bool, Int, Str
    pub default: OptionDefault,
    pub flags: OptionFlags,          // comma-list, file, hidden, secure, ...
    pub validator: OptionValidator,
    pub doc_tag: &'static str,
}

pub enum OptionValue { Bool(bool), Int(i64), Str(VimString) }
~~~

Storage is layered: global value plus per-buffer/window/tab overrides. A
global-local option (<code>'statusline'</code>, <code>'errorformat'</code>,
<code>'tags'</code>) has both a global and a local value; <code>:set</code>
writes both, <code>:setlocal</code> writes only local. The lookup rule lives in
one place and is unit-tested against a table of Vim cases.

### 6.3 Built-in functions

~~~rust
pub struct FunctionSpec {
    pub name: &'static str,
    pub min_args: u8,
    pub max_args: u8,
    pub ret: ValueType,
    pub flags: FunctionFlags,        // pure, context, lazy-args, feature gate
    pub handler: FunctionHandler,
    pub doc_tag: &'static str,
}

pub enum FunctionHandler {
    Pure(fn(&mut VimRuntime, &[Value]) -> Result<Value, VimError>),
    Host(fn(&mut dyn VimHost, &[Value]) -> Result<Value, VimError>),
    Special(SpecialForm),            // call(), function(), eval(), ...
}
~~~

Pure functions (string/list/math) live in <code>rvim-viml</code>. Host functions
(buffers/windows/options/search/syntax/jobs) live in <code>rvim-editor</code>.
<code>exists('*f')</code> consults the merged table. Missing features are
absent, not stubs that return zero.

### 6.4 Events and features

Events have a spec with name, pattern rules and payload schema.
<code>has()</code> answers come from a compile-time feature table plus runtime
capabilities (GUI, terminal, clipboard). The table is the single source for
<code>:version</code> output and the inventory appendix.

---

## 7. Vim script engine (summary)

Full design: [SCRIPTING.md](SCRIPTING.md).

- **Values:** Number (64-bit), Float, byte String, List, Dict, Blob, Funcref,
  Partial, Bool, Special, Channel, Job; Vim9 Class/Object are deferred.
- **Memory:** a mark-sweep heap in <code>VimRuntime</code>. Vim script permits
  cycles (<code>let l = [] | call add(l, l)</code>), so reference counting alone
  leaks. Roots are enumerated from the editor (globals, script dictionaries,
  call frames, registers, options, autocommands, timers/jobs, JS persistent
  handles).
- **Execution:** parse once into an AST; evaluate with an explicit call-frame
  stack. Function bodies are parsed on definition/first call and cached. A
  bytecode compiler is a later optimization, not a semantic change.
- **Scopes:** <code>g: b: w: t: s: l: a: v:</code>, script IDs, autoload
  naming, <code>&lt;SID&gt;</code>.
- **Errors:** Vim-style exceptions with
  <code>v:exception</code>/<code>v:throwpoint</code>; errors are values, not
  Rust panics. <code>:try</code>/<code>:catch</code> can catch Ex command
  errors.
- **Host boundary:** <code>VimHost</code> gives the interpreter typed access to
  buffers, windows, options and commands. The interpreter never borrows
  <code>&amp;mut Editor</code> across a callback.

Vim9 script is not parsed by this engine. <code>vim9script</code> files are
handled by the runtime loader's skip policy and reported in the compatibility
matrix.

---

## 8. Regular expressions and syntax

There is no external crate that implements Vim's dialect, so this is a
first-class subsystem and the highest-risk dependency.

### 8.1 Regex

- **Byte-oriented** over <code>&amp;[u8]</code>; UTF-8 is decoded only for
  character classes, case folding and column mapping.
- **Parser:** magic modes (<code>\v \m \M \V</code>), Vim atoms, groups,
  quantifiers, lookaround, <code>\zs/\ze</code>, buffer-context atoms, POSIX
  classes, equivalence classes (Unicode approximation documented).
- **Backtracking VM:** captures, backreferences, lookaround, leftmost-first
  semantics. It operates on a <code>MatchContext</code> that provides the
  subject bytes, cursor/visual/mark context and an interrupt check.
- **NFA fast path:** literal prefix scan plus PikeVM for patterns without
  backrefs/lookaround; <code>'regexpengine'</code> selects or hints. The
  backtracking VM remains the semantic reference.
- **Limits:** step budget, <code>got_int</code> checks,
  <code>'redrawtime'</code>/<code>'matchtime'</code> integration. A
  catastrophic pattern must fail, not freeze the editor.
- **Shared compiled cache:** <code>(pattern, magic, ignorecase, smartcase)</code>
  to compiled program, LRU-bounded.

### 8.2 Syntax

- Syntax items are compiled to matchers over the regex engine: keywords,
  matches, regions with start/end/skip, clusters, includes.
- Each line stores a region stack/state. On edit, invalidate the changed line
  and continue until state convergence or a bounded lookahead; if the bound is
  exceeded, restart from a known-good sync point (<code>syn sync</code>).
- Highlight spans are produced per line and cached by <code>changedtick</code>
  plus window options. Rendering intersects spans with folds, conceal, text
  properties and search matches.
- The runtime syntax corpus (790 files in the pinned 9.2.1338 tree) is a
  release gate; a syntax file that errors is a compatibility bug.

---

## 9. QuickJS integration (summary)

Full design: [QUICKJS.md](QUICKJS.md).

- **Engine:** QuickJS-NG or a maintained QuickJS fork, vendored and built behind
  a <code>quickjs</code> Cargo feature. No system-library requirement. The FFI
  layer is small and audited; a safe wrapper (for example rquickjs, if it tracks
  the chosen fork) is preferred over hand-written bindings.
- **One runtime per editor**, created after the editor core and dropped before
  it. All JS runs on the editor thread; background threads only deliver events.
- **API:** a <code>vim</code> global with <code>cmd</code>, <code>eval</code>,
  <code>fn</code>, <code>api</code>, <code>g/b/w/t/v</code>, version and
  notification helpers. JS functions can be registered as Vim callbacks; Vim
  funcrefs are callable from JS.
- **Conversion:** primitives map directly; arrays/objects map to List/Dict;
  <code>Uint8Array</code> maps to Blob; functions become persistent handles
  rooted across both garbage collectors; objects crossing the boundary are
  copied, not aliased.
- **Event loop:** after each JS entry and each callback, drain the QuickJS job
  queue (promises/microtasks). Timers and jobs may resolve promises. No blocking
  <code>await</code> on the editor thread.
- **Modules:** ES modules resolved from <code>$VIMRUNTIME/js</code>,
  <code>&lt;rtp&gt;/js</code> and pack directories. No Node resolution, no
  <code>node_modules</code>.
- **Security:** no <code>std</code>/<code>os</code> modules by default, no raw
  filesystem/process access, memory and stack limits, interrupt handler tied to
  <code>got_int</code>, <code>:sandbox</code>/<code>-Z</code> enforcement.
- **Feature honesty:** <code>has('quickjs')</code> is true only when compiled
  and initialized; <code>v:js_version</code> reports the engine version.

QuickJS is an extension surface, not a replacement for legacy Vim script. The
runtime corpus must run without QuickJS installed; JS plugins are optional.

---

## 10. Vim runtime compatibility (summary)

Full design: [RUNTIME-COMPAT.md](RUNTIME-COMPAT.md).

- **Pinned runtime.** Vim 9.2.1338's <code>runtime/</code> tree is vendored or
  downloaded with a checksum; no local edits. Licensing is handled explicitly.
- **Path semantics.** <code>'runtimepath'</code> defaults, <code>$VIMRUNTIME</code>,
  <code>after/</code> ordering, <code>:runtime</code>/<code>:runtime!</code>
  globbing, packages and plugin load order.
- **Filetype pipeline.** <code>filetype.vim</code>/<code>scripts.vim</code> ->
  <code>FileType</code> -> <code>ftplugin/</code> and <code>indent/</code> ->
  <code>Syntax</code> -> <code>syntax/</code>; <code>b:did_*</code> guards and
  <code>b:undo_*</code> strings.
- **Help.** <code>doc/tags</code> parsing, <code>:help</code> buffers,
  <code>:helptags</code>, completion and <code>:helpgrep</code>.
- **Tiers.** Tier A (core editing/help/syntax), Tier B (common plugins and
  languages), Tier C (everything). The matrix records pass/fail and known gaps.
- **Vim9 gate.** Files whose first line is <code>vim9script</code> (with or
  without <code>noclear</code>) are skipped with one verbose warning; an opt-in
  strict mode errors. <code>has('vim9script')</code> is false.
- **Measurement.** A per-file smoke harness sources runtime scripts, loads
  ftplugins/indents/syntax/colors/compilers and compares outcomes; regressions
  block releases.

The runtime is the largest compatibility test suite we have. It is not an
optional integration test; it is the primary oracle.

---

## 11. UI, windows and core widgets

Full designs: [UI-PROTOCOL.md](UI-PROTOCOL.md) and [WINDOWS.md](WINDOWS.md).

### 11.1 Frame contract

The core produces a <code>Frame</code>:

~~~rust
pub struct Frame {
    pub seq: u64,
    pub size: Size,
    pub layers: Vec<Layer>,       // base + popup/overlay layers, z-ordered
    pub cursor: Option<Cursor>,
    pub mouse_shape: MouseShape,
    pub title: Option<String>,
    pub bell: bool,
    pub clipboard: Option<ClipboardRequest>,
}
~~~

Cells carry a grapheme cluster, a width and an <code>AttrId</code>. Attributes
are resolved per backend capabilities (mono/16/256/truecolor) in the core, so
the same highlight semantics work in TUI and GUI. The backend never interprets
Vim highlight names or statusline syntax.

### 11.2 Statusline, tabline, tabpanel and cmdline

These are widgets in <code>rvim-view</code>:

- <code>Statusline</code> evaluates <code>'statusline'</code>, expands items,
  applies highlights/truncation and emits segments. Its height is 1..N from
  <code>'statuslineopt'</code>; the compositor reserves the rows per window.
- <code>Tabline</code> evaluates <code>'tabline'</code>, emits tab labels,
  close boxes and click regions. <code>Tabpanel</code> renders the vertical
  sidebar variant from <code>'tabpanel'</code>/<code>'tabpanelopt'</code>.
- <code>%[FuncName]</code> regions become semantic click actions carrying the
  callback, <code>minwid</code> and area; the widget never invokes VimL itself.
- <code>Cmdline</code> owns editing/completion state and renders the prompt,
  text, cursor and completion popup.

They use the same <code>GridWriter</code> as buffer text and have unit tests
that render into a headless frame. No widget imports <code>rvim-tui</code> or a
GUI crate. This is enforced by crate dependencies, not convention.

### 11.3 Windows

<code>Window</code> is a viewport in the presentation layer: it owns cursor,
scroll, folds and window-local options, and paints its rectangle. Layout is a
frame tree; splits, separators, statusline rows, the optional tabpanel and
resizing are computed in cells.
The buffer model has no notion of a window. Mouse hit regions reference
semantic window/tab/statusline actions, so the backend only sends coordinates.

### 11.4 GUI seam

A future GUI implements <code>UiBackend</code> and consumes the same frames.
Optional GUI extensions (pixels, IME preedit, native menus, scrollbars, font
metrics, ligatures) are separate traits negotiated through
<code>UiCapabilities</code>. Vim options that only affect a GUI
(<code>'guifont'</code>, <code>'guioptions'</code>, toolbar) are stored and
reported, but have no effect in the TUI. <code>has('gui')</code> and
<code>has('gui_running')</code> are false.

---

## 12. Threading and concurrency

| Component | Thread | Rule |
|---|---|---|
| Editor model, VimL, JS, syntax, regex | main | single-threaded; no locks in hot paths |
| TUI input/output | main (backend driver) | waits with a deadline; no editor callbacks from inside the backend |
| Job/PTY I/O reactor | background | only moves bytes/events; never touches editor state; PTY is used by job_start, not by an emulator |
| Process reaping / timers | background reactor or main | wake the main loop via event queue; timers fire on main |
| Clipboard providers | sync read, async write | provider does not interpret registers |
| GUI (future) | main/event loop | forwards events; core stays single-threaded |

The main loop computes the nearest deadline (timer, redraw, input poll) and
waits. Job output is queued; the main thread drains the queue at safe points
(between actions, during <code>:sleep</code>, before blocking for input). This
is how Vim stays responsive without running Vim script concurrently.

Shutdown order: stop accepting input, cancel timers, close channels/jobs/PTYs,
drain callbacks, drop JS persistent handles, collect VimL GC, destroy QuickJS
runtime, restore terminal, flush logs. Encryption keys are zeroized.

---

## 13. Errors, messages and diagnostics

- <code>VimError</code> carries an E-code, a message, an optional throwpoint and
  a category (user error, unsupported feature, internal error). User errors are
  values; Rust panics are bugs.
- Ex commands and VimL calls return <code>Result&lt;_, VimError&gt;</code>. The
  dispatcher decides whether to display, convert to an exception, or abort a
  script based on <code>:silent</code>, <code>:try</code> and abort flags.
- Messages go through one pipeline: echo/echomsg/echoerr/redir, message
  history, hit-enter and <code>v:errmsg</code>/<code>v:statusmsg</code>.
- A panic hook logs the crash, restores the terminal and writes a report; the
  process exits non-zero. We do not use panics for control flow, including
  "unsupported feature".
- <code>:verbose</code> and category logs explain which script/line/option
  triggered an action. Secrets (encryption keys, clipboard contents, JS
  sandbox data) are redacted.
- Every unsupported feature has a stable message and a documentation link, not
  a generic "not implemented".

---

## 14. Testing strategy

| Level | What | Tooling |
|---|---|---|
| Unit | text/line tree, options, regex AST/VM, VimL values, Ex parser, statusline items | Rust tests, property tests |
| Integration | key sequences, Ex commands, autocommands, undo, mappings | headless editor driver |
| Golden frames | windows/splits, statusline/tabline/cmdline, popup menu, syntax highlights | deterministic cell snapshots |
| Differential | buffer/cursor/registers/options/messages vs pinned Vim | headless Vim oracle, canonicalized |
| Runtime smoke | plugins, ftplugins, indents, syntax, colors, compilers, help tags | per-file harness and dashboard |
| Fuzz | regex, Ex, VimL, modeline, input decoder, terminal parser, JS bridge | cargo-fuzz, minimized corpus |
| Performance | startup, large files, scrolling, syntax, regex, JS | benchmarks with budgets |
| Security | sandbox escapes, regex DoS, malicious runtime data | negative tests |

The oracle harness runs Vim with <code>-Nu NONE -i NONE -es</code> (or the
equivalent headless invocation) in a temp directory, feeds commands, and
captures <code>:redir</code> output plus written files. Cases that depend on
GUI/Vim9/Vi are classified, not silently skipped. Every differential test names
the Vim patch level it was recorded against.

The runtime smoke harness runs each script in a fresh process so a crash or
<code>:finish</code> cannot contaminate the next file. Results are stored as
JSON with file hash, error, and classification (pass, expected gap, regression).

---

## 15. Performance budgets

These are design budgets, not measurements. They are enforced by benchmarks
once the walking skeleton exists.

| Scenario | Budget |
|---|---|
| Cold start, <code>--clean</code>, 80x24 | under 100 ms |
| Start with runtime plugins (Tier A) | under 300 ms |
| Key-to-frame latency, 200x60, syntax on | p99 under 16 ms |
| Full frame render, 200x60, no syntax | under 4 ms |
| Incremental syntax update, 10k-line buffer | under 10 ms |
| Open 100 MB file | under 1 s to first frame |
| Insert/delete at random line, 1M lines | under 2 ms |
| Typical regex match on a 1 KB line | under 1 ms |
| Pathological regex | bounded by budget, interruptible |
| VimL function call overhead | under 2 microseconds |
| JS call overhead (empty function) | under 20 microseconds |
| QuickJS runtime startup | under 10 ms |
| Runtime smoke suite (Tier A) | under 15 minutes in CI |

Memory: no more than roughly 2x file size plus indexes for large files; syntax
and undo caches are bounded; QuickJS memory limit is configurable. A regression
over 10% on a tracked benchmark requires justification.

---

## 16. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Vim regex is a large, quirky dialect | blocks search, syntax, Ex, runtime | implement first behind a differential corpus; backtracking reference + NFA fast path; explicit unsupported atoms list |
| Legacy VimL breadth and edge cases | blocks runtime and plugins | value/GC design first; function registry; oracle tests; implement by runtime demand |
| Runtime corpus assumptions (GUI, Vim9, optional features) | false failures | honest <code>has()</code>, tiering, classification, skip policy |
| Spell binary formats | high effort for one feature | read-only <code>.spl</code> first, <code>:mkspell</code> later, explicit gap |
| UI coupling creep | violates core requirement | crate dependency checks; widgets only emit cells; headless tests |
| JS/VimL GC interaction | leaks/crashes | persistent handle registry, root enumeration, drop-order tests |
| Performance regressions | unusable editor | budgets from day one, benchmarks in CI |
| Scope explosion (GUI, Vim9, Neovim API) | never ships | explicit exclusions and phase gates |
| Security of modelines/plugins/JS | code execution | sandbox/restricted mode, escaping tests, no raw JS OS access |

**Kill criteria:** if the regex engine cannot meet the differential corpus and
latency budget after the NFA fast path, fall back to a narrower supported-atom
set with explicit errors rather than silently wrong matches. If QuickJS cannot
be built reproducibly on a target, ship it as an opt-in feature and keep the
core independent.

---

## 17. Key decisions (ADR summary)

| ADR | Decision | Rationale |
|---|---|---|
| ADR-001 | Single-threaded editor core | Vim semantics are reentrant and stateful; locks would leak into plugins. |
| ADR-002 | One composited cell grid per frame, with overlay layers | TUI needs a final grid; GUI can still use layer metadata. Avoids multi-grid complexity. |
| ADR-003 | Windows live in <code>rvim-view</code>, not in the buffer model | Satisfies "window with UI layer" while keeping rendering backend-neutral. |
| ADR-004 | Byte strings and byte positions internally | Vim compatibility for <code>col()</code>, marks, regex and file formats. |
| ADR-005 | Custom Vim regex engine | No off-the-shelf engine implements <code>\@&lt;=</code>, <code>\zs</code>, magic modes and Vim's classes. |
| ADR-006 | Mark-sweep GC for VimL | Cycles are legal; ownership cannot express them; explicit roots match Vim's semantics. |
| ADR-007 | QuickJS-NG-class engine, vendored | Small, fast, embeddable, ES2023; no Node dependency. |
| ADR-008 | Pinned, unmodified Vim runtime | Compatibility oracle and license clarity; no local patches. |
| ADR-009 | No Vim9, no Vi/Ex, no GUI in this phase | Focus; interfaces remain open. |
| ADR-010 | Registry-driven Vim surface | One source for parsing, completion, help, inventory and tests. |
| ADR-011 | TUI renders directly, not through a widget framework | Vim needs cell-level control (wide chars, conceal, folds, cursor); a framework hides too much. |
| ADR-012 | Headless backend is a first-class backend | Enables golden tests, CI and a future server/daemon. |

---

## 18. Startup sequence

The exact order matters to runtime scripts. The target is Vim's documented
startup behavior, not a simplified approximation.

1. Parse arguments; handle <code>--version</code>/<code>--help</code> and
   <code>--startuptime</code> before touching the screen.
2. Initialize locale-independent UTF-8 internal encoding; record
   <code>v:argv</code>, <code>v:version</code>, feature flags.
3. Resolve <code>$VIMRUNTIME</code>, <code>'runtimepath'</code> defaults and
   <code>--clean</code>/<code>-u</code> modes.
4. Construct the editor, registries, buffers and message system.
5. Initialize the backend (size, capabilities) so startup messages can render.
6. Run <code>--cmd</code> commands (before vimrc).
7. Load the user vimrc and plugins unless disabled; run package startup.
8. Process <code>-c</code>/<code>+cmd</code>/<code>-S</code> commands and load
   files/argument list/session.
9. Fire <code>VimEnter</code>, set <code>v:vim_did_enter = 1</code>, render the
   first frame.
10. Enter the event loop. Errors before this point are reported with a
    non-zero exit code; <code>--headless</code> uses the same order without a
    TUI.

Startup errors are never silently swallowed. A missing vimrc is normal; a
syntax error in a sourced script is reported with file/line and does not abort
the rest of startup unless Vim would abort.

---

## 19. Proposed source tree

~~~text
rvim/
  Cargo.toml                 workspace
  runtime.lock               pinned Vim tag/source/checksum
  crates/
    rvim-types/
    rvim-ui-api/
    rvim-platform/
    rvim-text/
    rvim-regex/
    rvim-viml/
    rvim-ex/
    rvim-edit/
    rvim-syntax/
    rvim-runtime/
    rvim-js/
    rvim-view/
    rvim-editor/
    rvim-tui/
    rvim-headless/
    rvim-cli/                binary rvim
  runtime/                   pinned Vim 9.2.1338 runtime (not forked)
  docs/                      this design set
  tools/                     inventory/catalog/selection tooling
  tests/                     integration/oracle/fixtures
  benches/
~~~

No source tree is created until the feature selection is fixed. The first
milestone creates the crates needed for the P0 walking skeleton, not all of
them at once.

---

## 20. Scope guard

Changes to scope go through the catalog first:

1. Add or change a feature row in [FEATURES.md](FEATURES.md).
2. Validate with <code>python3 tools/validate_catalog.py</code>.
3. Record the design impact here or in the relevant deep-dive document.
4. Add acceptance tests and a phase gate.

Do not add behavior that is not in the catalog and not required by the pinned
runtime without recording it. Compatibility gaps are acceptable; invisible
behavior is not.


