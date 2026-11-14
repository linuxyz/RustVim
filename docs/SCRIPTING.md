# Legacy Vim script engine

**Status:** design baseline. This document covers legacy Vim script (VimL), not
Vim9 script and not QuickJS. QuickJS is described in [QUICKJS.md](QUICKJS.md).

Legacy VimL is mandatory: the Vim runtime, ftplugins, indent files, syntax files,
colorschemes and most plugins are written in it. Without a compatible engine,
runtime compatibility is impossible.

---

## 1. Approach

RVim interprets VimL; it does not transpile it to Rust or to another language.
Transpilation would change error timing, scoping and side effects, and would make
dynamic features (autoload, Funcref, dict functions, <code>:execute</code>)
fragile.

Pipeline:

~~~text
:source file
  -> script encoding/continuation normalization
  -> Ex command parser (rvim-ex)
  -> script executor
       - :let/:if/:while/:function/... dispatched as commands
       - expressions parsed by the VimL expression parser
       - function bodies stored; parsed on first call
  -> evaluator (AST + call frames)
  -> VimHost for editor access
~~~

The Ex parser and VimL parser share tokens where practical, but they are
separate grammars: Ex commands are line-oriented; expressions are
precedence-based.

---

## 2. Values

~~~rust
pub enum Value {
    Number(i64),
    Float(f64),
    String(VimString),        // bytes, NUL-safe
    List(GcRef),
    Dict(GcRef),
    Blob(GcRef),
    Funcref(GcRef),           // user function, builtin, partial, lambda, JS
    Bool(bool),
    Special(SpecialValue),    // v:null, v:none
    Channel(ChannelId),
    Job(JobId),
    // Vim9 Class/Object/Type are deferred.
}
~~~

Rules:

- Numbers are 64-bit signed; overflow follows Vim (wrap or float promotion where
  Vim does it, documented per operator).
- Strings are byte strings. Indexing/slicing is byte-based unless the function is
  explicitly character-based (<code>strcharpart()</code>, etc.).
- Lists and Dicts are ordered (insertion order); Vim does not guarantee Dict
  order, but deterministic order makes tests and <code>keys()</code> stable. We
  document this as an implementation guarantee, not a Vim guarantee.
- Dict keys are byte strings; missing key access returns 0 (with
  <code>v:errmsg</code> rules) or errors depending on context.
- Blobs are byte arrays, indexable like lists.
- Funcref is a first-class value with a name, bound args/dict, closure and
  script context.
- Bool and Special are distinct from Number; <code>type()</code> and
  <code>typename()</code> match Vim.

### 2.1 Conversions

Conversion is explicit and table-driven:
- Number <-> String (decimal/hex/octal, leading zeros, errors),
- Number <-> Float,
- List <-> Blob (via <code>list2blob()</code>/<code>blob2list()</code>),
- String <-> List of chars/bytes,
- Bool to Number where Vim allows it,
- Funcref to String (function name) where Vim allows it.

No implicit Rust <code>From</code> conversions leak into the interpreter: every
conversion has a Vim-compatible error path.

---

## 3. Memory management

VimL permits cycles:

~~~vim
let l = []
call add(l, l)
~~~

Reference counting alone would leak. We use a mark-sweep heap:

~~~rust
pub struct GcHeap {
    slots: Vec<Option<GcObject>>,
    free: Vec<u32>,
    generation: Vec<u16>,
    gray: Vec<GcRef>,
    bytes: usize,
    threshold: usize,
}

pub struct GcRef { index: u32, generation: u16 }

pub enum GcObject {
    List(Vec<Value>),
    Dict(IndexMap<VimString, Value>),
    Blob(Vec<u8>),
    Function(UserFunction),
    Partial(Partial),
    Lambda(Lambda),
    JsFunction(JsHandle),
}
~~~

### 3.1 Roots

Collection starts from:

- global variables (<code>g:</code>),
- script-local dictionaries for every loaded script,
- buffer/window/tab dictionaries and variables,
- the call-frame stack (locals, arguments, temporaries),
- registers, options that hold Funcrefs, autocommands, mappings,
- timers, jobs/channels with callbacks,
- QuickJS persistent handles that wrap Vim values,
- exception state (<code>v:exception</code>) and the current command.

The root enumeration is a single method on <code>Editor</code>. A missed root is
a use-after-free bug; tests exercise cycles, closures, timers and JS callbacks.

### 3.2 Collection points

- On allocation when <code>bytes &gt; threshold</code>.
- At safe points between top-level commands (never in the middle of a Rust
  borrow of a GC object).
- Explicitly at shutdown after JS handles are released.

The collector is non-moving: <code>GcRef</code> is stable. This keeps Funcref
identity and C/JS handles simple. Compaction, if ever needed, is a later
optimization with the same semantics.

### 3.3 Why not Rc + Weak?

VimL values are mutable through aliases (<code>let b = a</code> shares the same
List). Rust ownership would require pervasive <code>Rc&lt;RefCell&gt;</code>
and a cycle collector anyway. A single heap with explicit roots is simpler to
audit and matches Vim's GC model.

---

## 4. Parser and evaluator

### 4.1 Expressions

A Pratt parser handles:

- literals (Number, Float, String, List, Dict, Blob, Funcref),
- variables and scopes (<code>g: b: w: t: s: l: a: v:</code>),
- indexing/slicing, dict member access, method calls (<code>-&gt;</code>),
- calls, <code>function()</code>, <code>funcref()</code>, lambdas
  (<code>{x -&gt; ...}</code>),
- unary/binary/ternary operators, comparison with <code>#</code>/<code>?</code>,
- <code>is</code>/<code>isnot</code>, <code>=~</code>/<code>!~</code>,
- string escapes, number formats, comments.

The AST is typed enough for error positions and source attribution, but values
stay dynamic.

### 4.2 Commands

Ex commands are parsed by <code>rvim-ex</code> and dispatched through the command
registry. Script-specific commands include:

- <code>:let</code>, <code>:const</code>, <code>:unlet</code>,
  <code>:lockvar</code>/<code>:unlockvar</code>,
- <code>:if/:elseif/:else/:endif</code>, <code>:while</code>, <code>:for</code>,
  <code>:break/:continue</code>,
- <code>:function/:endfunction/:return/:delfunction</code>,
- <code>:try/:catch/:finally/:endtry/:throw</code>,
- <code>:execute</code>, <code>:eval</code>, <code>:call</code>,
- <code>:source</code>, <code>:finish</code>, <code>:scriptencoding</code>,
  <code>:scriptversion</code>.

The executor is a loop with a program counter over the current script/function
body. It handles line continuation before parsing, and preserves the exact line
number for errors and <code>v:throwpoint</code>.

### 4.3 Function bodies

<code>:function</code> stores the raw body (list of lines) and metadata. On first
call:

1. Parse the body into a statement list.
2. Resolve local variable names (the <code>l:</code> dict is created lazily).
3. Cache the AST by script ID and function name.

Parsing on definition instead of first call is possible, but Vim reports some
errors at call time; we match Vim's timing. Cached ASTs are discarded when a
function is redefined or its script is unloaded.

### 4.4 Call frames

~~~rust
pub struct Frame {
    pub func: Option<Funcref>,
    pub args: Vec<Value>,
    pub locals: DictRef,
    pub a000: Vec<Value>,
    pub a0: usize,
    pub script_id: ScriptId,
    pub line: u32,
    pub try_stack: Vec<TryFrame>,
    pub abort: bool,
}
~~~

There is an explicit stack, not Rust recursion, so
<code>'maxfuncdepth'</code>, interrupts and error unwinding are controllable. A
recursive VimL function must not overflow the Rust stack.

---

## 5. Scopes and scripts

| Scope | Storage | Lifetime |
|---|---|---|
| <code>g:</code> | editor globals | session |
| <code>b:</code> | Buffer vars | buffer |
| <code>w:</code> | Window vars | window |
| <code>t:</code> | TabPage vars | tab |
| <code>s:</code> | Script dict | script loaded |
| <code>l:</code> | current frame | call |
| <code>a:</code> | current args | call |
| <code>v:</code> | predefined read-only (mostly) | session |

- Scripts get stable IDs. <code>&lt;SID&gt;</code> and <code>&lt;script&gt;</code>
  resolve to function names/script-local references.
- Autoload naming (<code>foo#bar#baz</code>) maps to
  <code>autoload/foo/bar.vim</code>; loading is lazy and cached.
- <code>:scriptnames</code> lists files and IDs in load order.
- <code>s:var</code> is not visible from other scripts, including scripts with
  the same basename.
- Deleting a script (e.g., <code>:bwipeout</code> of its buffer) releases its
  script dict through GC roots.

---

## 6. Exceptions and abort semantics

VimL errors are values with:

- E-code and message,
- throwpoint (script/line/function),
- an "exception" string for <code>:throw</code>,
- an abort flag.

<code>:try</code> pushes a <code>TryFrame</code>; <code>:catch</code> matches
<code>v:exception</code> against patterns; <code>:finally</code> always runs;
<code>:endtry</code> restores. An error inside a function with
<code>abort</code> stops the function; otherwise execution continues to the next
line, matching Vim's legacy behavior.

Errors from Ex commands (including <code>:substitute</code> failures) are
catchable. <code>:silent!</code> suppresses display but not the error state where
Vim keeps it.

The interpreter never uses Rust panics for Vim errors. A Rust panic is a bug and
is caught at the top-level command boundary for diagnostics, not converted into
<code>:catch</code>.

---

## 7. Host boundary

~~~rust
pub trait VimHost {
    fn get_option(&mut self, scope: OptionScope, name: &str) -> Result<Value>;
    fn set_option(&mut self, scope: OptionScope, name: &str, value: Value) -> Result<()>;
    fn get_var(&mut self, scope: VarScope, name: &str) -> Result<Value>;
    fn set_var(&mut self, scope: VarScope, name: &str, value: Value) -> Result<()>;
    fn buffer_line(&mut self, buf: BufferId, lnum: u32) -> Result<VimString>;
    fn set_buffer_line(&mut self, buf: BufferId, lnum: u32, text: &VimString) -> Result<()>;
    fn execute_ex(&mut self, cmd: &str, ctx: ExContext) -> Result<Value>;
    fn call_funcref(&mut self, f: &Funcref, args: &[Value]) -> Result<Value>;
    // windows, tabs, search, jobs, syntax, quickfix, ...
}
~~~

- Pure functions live in <code>rvim-viml</code> and never see the host.
- Host functions live in <code>rvim-editor</code> and are registered into the
  same function table.
- A host call that invokes VimL (for example <code>map()</code> with a Funcref)
  receives a call handle, not a <code>&amp;mut Editor</code> borrow that could
  alias the interpreter. Reentrancy is explicitly allowed only where Vim allows
  it; a guard prevents recursive editor mutation during a callback.
- <code>:execute</code> and <code>execute()</code> capture messages through a
  sink; they do not scrape the screen.

The host boundary is also the test seam: a mock host can run pure interpreter
tests without a full editor.

---

## 8. Built-in functions

The function registry has 605 entries from the pinned 9.2.1338 runtime. They
are grouped:

- pure (string, list, dict, blob, math, JSON, base64, hash, time),
- editor context (buffer, window, tab, cursor, marks, options, search),
- syntax/highlight/folds/signs/properties,
- quickfix/tags/help,
- jobs/channels/timers (terminal functions descoped),
- popup/menu/balloon,
- introspection (`getinfo`, `getscriptinfo`, `listener_*`),
- redraw listeners (`redraw_listener_*`, main-thread callbacks),
- testing/assert/profile,
- JS bridge (<code>js_eval</code>, <code>js_call</code>, <code>js_function</code>).

Metadata includes arity, return type, purity, feature gate and doc tag. The same
table powers <code>exists('*f')</code>, completion and the inventory audit.
Functions that are not implemented are absent; they do not return a fake zero.

---

## 9. execute, redir and message capture

<code>:execute</code> evaluates an expression list, joins with spaces, and runs
the resulting string as Ex commands. The result can contain newlines and command
separators. Errors propagate with the script/line of the executed command.

<code>:redir</code> redirects messages to a variable, file or register. The
interpreter must capture:
- normal echo/echomsg output,
- error messages (unless suppressed),
- listing commands (<code>:ls</code>, <code>:map</code>, ...),
- output from nested scripts.

Ordering and trailing newlines matter to runtime scripts. Tests compare captured
text with Vim, not just substrings.

---

## 10. Performance

Targets are in [ARCHITECTURE.md](ARCHITECTURE.md#15-performance-budgets).

- Parse once, evaluate many; cache function ASTs.
- Avoid allocation for small strings/numbers; intern identifiers.
- Explicit frame stack; no Rust recursion per VimL call.
- GC only at safe points; tune threshold to allocation rate.
- Optional future bytecode compiler for hot functions, with identical semantics
  and the same error/throwpoint behavior.
- Profiling hooks (<code>:profile</code>) instrument AST nodes/frames without
  changing results.

Legacy VimL is not expected to match Vim9 compiled speed, but it must be fast
enough for runtime scripts and plugins to feel normal.

---

## 11. Vim9 boundary

The interpreter does not parse Vim9:

- <code>vim9script</code> files are skipped by the runtime loader.
- <code>:def</code>, <code>:class</code>, <code>:interface</code>,
  <code>:enum</code>, <code>:type</code>, <code>:import</code>/<code>:export</code>
  are explicit errors, not partial implementations.
- <code>has('vim9script')</code> is false.
- A legacy script that calls a Vim9-only function gets a normal unknown-function
  error.

This boundary is tested: no Vim9 token can accidentally be accepted by the
legacy parser and produce wrong behavior.

---

## 12. Testing

- **Value/GC:** cycles, aliases, closures, timers, JS handles, collection
  stress, use-after-free detectors under Miri where possible.
- **Parser:** valid/invalid expressions, line continuation, heredocs, escapes,
  error positions.
- **Semantics:** a corpus of VimL snippets with expected values/errors, run
  against Vim for differential results.
- **Runtime functions:** invoke real autoload/plugin functions on sample inputs.
- **Exceptions:** try/catch/finally, nested calls, abort vs continue, throwpoint.
- **Performance:** function call, list/dict operations, autoload dispatch.
- **Fuzz:** parser, evaluator, GC roots; no panics or leaks.

A VimL feature that cannot be tested against Vim is either documented as an
extension or not implemented.
