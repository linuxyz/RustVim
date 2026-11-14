# QuickJS integration

**Status:** design baseline. QuickJS is an RVim extension, not a Vim 9.2
feature. It must not change Vim script semantics, runtime compatibility or
<code>has()</code> answers for Vim-only features.

---

## 1. Goals

1. Embed a small, fast, ES2023-capable JavaScript engine with no Node.js
   dependency.
2. Give JS full access to the editor through a stable, typed-enough API:
   commands, expressions, buffers, windows, options, variables, callbacks,
   timers and jobs.
3. Make JS functions first-class Vim callbacks and Vim funcrefs callable from
   JS.
4. Integrate promises/microtasks with the editor event loop without blocking
   the main thread.
5. Enforce memory, stack and sandbox limits.
6. Keep the core buildable and fully functional without QuickJS.

Non-goals: Node compatibility, npm resolution, Web APIs, worker threads, JS
implementations of Vim9 classes, or a second copy of Vim script semantics.

---

## 2. Engine choice and build

- **Engine:** QuickJS-NG (preferred) or a maintained QuickJS fork. It is small,
  embeddable, supports ES2023 features we need (modules, BigInt, promises,
  typed arrays) and has a C API that is practical to vendor.
- **Bindings:** a thin audited FFI layer or a safe wrapper crate that tracks the
  chosen fork. We do not hand-write a large binding surface if a maintained
  wrapper exists; we do keep the boundary small.
- **Build:** C sources vendored under <code>third_party/quickjs/</code>, built by
  <code>cc</code> behind the <code>quickjs</code> Cargo feature. No network at
  build time, no system libquickjs, no ABI surprises.
- **Platforms:** macOS and Linux first; Windows when the editor's platform
  layer supports it. If a target cannot build QuickJS, the feature is disabled
  and <code>has('quickjs')</code> is false.
- **Determinism:** pin the engine version and build flags; record them in
  <code>:version</code> and <code>v:js_version</code>.

The editor must compile and pass its core tests with the <code>quickjs</code>
feature disabled. QuickJS is not a core dependency.

---

## 3. Runtime lifecycle and threading

~~~text
editor starts
  -> create editor core, registries, runtime paths
  -> initialize QuickJS runtime/context (if feature enabled)
  -> expose vim
  -> source vimrc / plugins (JS plugins can run)
  -> event loop
  -> shutdown: drain jobs/promises, release persistent handles,
     collect VimL GC, destroy QuickJS runtime/context
~~~

- **One runtime and one context per editor.** Tests create independent editors
  with independent JS heaps.
- **All JS runs on the editor thread.** No JS execution from the I/O reactor,
  no shared <code>JSValue</code> across threads.
- **No editor borrow across JS.** A JS call receives a host handle; the
  interpreter is not holding <code>&amp;mut Editor</code> while JS runs.
  Results and callbacks are applied at safe points.
- **Interrupts:** QuickJS's interrupt handler checks the editor's
  <code>got_int</code> flag and a deadline, so <code>CTRL-C</code> can stop a
  runaway script. A hard instruction/time budget is configurable.
- **Memory:** <code>setMemoryLimit</code> (default configurable, e.g. 64 MB) and
  <code>setMaxStackSize</code> are enforced per runtime. Exceeding them raises a
  JS error that becomes a Vim error; it does not abort the process.
- **Shutdown order:** stop timers/jobs, reject pending promises, drain
  microtasks, release persistent function handles, collect VimL, destroy
  QuickJS. A JS finalizer must not call back into a half-destroyed editor.

---

## 4. Commands and script loading

| Command | Behavior |
|---|---|
| <code>:js {code}</code> | Evaluate code as a script in the current JS context. |
| <code>:jsdo {code}</code> | Like <code>:js</code>, with a line range available as <code>vim.line1</code>/<code>vim.line2</code>. |
| <code>:jsfile {file}</code> | Source a JS file as a module/script; errors carry file/line. |
| <code>:jsversion</code> | Print engine version and build flags. |

- Each sourced file gets a script ID for error attribution and
  <code>:scriptnames</code> (kind <code>js</code>).
- <code>:js</code> code is evaluated as a script, not a module, so top-level
  <code>var</code>/<code>function</code> declarations are scoped to that script
  wrapper, not leaked to <code>globalThis</code>. Modules use normal ES module
  scope.
- A JS exception aborts the command and becomes a Vim error/exception.
- <code>:jsfile</code> is the recommended plugin entry point; <code>:js</code>
  is for small expressions/tests.

---

## 5. The vim API

The API is deliberately small and Vim-shaped. It is not a clone of Neovim's
<code>vim</code> object.

~~~js
// Commands and expressions
vim.cmd("set number");
vim.cmd(["set number", "set relativenumber"]);
const ts = vim.eval("&tabstop");          // number
const name = vim.eval("expand('%:t')");   // string

// Functions
const now = vim.fn.strftime("%Y-%m-%d");
const lines = vim.fn.getline(1, "$");
const ok = vim.call("setreg", '"', "hello");

// Variables
vim.g.my_plugin_enabled = true;
vim.b.local_flag = 1;
vim.w.winvar = "x";
vim.t.tabvar = "y";
vim.v.errmsg;                              // read-only where Vim is read-only

// Options (extension ergonomics; values still go through the option engine)
vim.o.number = true;
vim.bo.shiftwidth = 4;
vim.wo.wrap = false;

// Notifications and version
vim.notify("loaded", "info");
vim.version;                               // {major, minor, patch, engine}
~~~

### 5.1 vim.api

<code>vim.api</code> is a typed wrapper over a curated set of hot editor
operations. It exists for performance and for callbacks, not to duplicate every
<code>vim.fn</code> function.

~~~js
const buf = vim.api.getCurrentBuffer();
const line = vim.api.getBufferLine(buf, 1);
vim.api.setBufferLine(buf, 1, "new text");

const win = vim.api.getCurrentWindow();
vim.api.setCursor(win, { line: 10, col: 4 });
vim.api.createAutocmd("BufWritePost", {
  pattern: "*.rs",
  callback: () => vim.notify("saved"),
});
~~~

The exact names are generated from the editor API table, documented with
types, and versioned. <code>vim.fn</code> remains the compatibility path for
Vim functions; <code>vim.api</code> is the stable, typed path for JS plugins.

### 5.2 Fast vs safe callbacks

Some events (for example, text-change notifications or job output) may occur
while the editor is in a state where arbitrary commands are unsafe. The API:

- queues such callbacks to the next safe point by default;
- exposes <code>vim.in_fast_event()</code> so a plugin can detect the restricted
  context;
- in a fast event, only a documented subset (read-only queries, scheduling) is
  allowed. Calling a mutating command throws a clear error.

This mirrors the real constraint without pretending JS runs concurrently.

---

## 6. Type conversion

| JavaScript | Vim | Rules |
|---|---|---|
| <code>number</code> | Number or Float | integral, safe range -> Number; non-integral -> Float; outside i64 -> error |
| <code>bigint</code> | Number | range-checked; overflow error |
| <code>string</code> | String (bytes) | UTF-8 encoded; NUL allowed; lone surrogates replaced/error |
| <code>boolean</code> | Bool | direct |
| <code>null</code> | <code>v:null</code> | direct |
| <code>undefined</code> | <code>v:none</code> | direct |
| <code>Array</code> | List | deep copy; holes become <code>v:none</code> |
| <code>Object</code> | Dict | own enumerable string keys; deep copy |
| <code>Uint8Array</code>/<code>ArrayBuffer</code> | Blob | byte copy |
| <code>function</code> | Funcref | persistent handle; callable from Vim |
| Vim Funcref | JS function | wrapper that calls into the interpreter |
| <code>Date</code> | Float | Unix seconds (documented extension) |
| <code>Map</code>/<code>Set</code> | error | convert explicitly; no silent shape change |
| cycle | error | conversion detects cycles; no graph aliasing |

The boundary is a **copy**, not a live view. Mutating a JS array after passing it
does not mutate the Vim List, and vice versa. This avoids two heaps sharing
mutable graph nodes. Function handles are the exception: they are identity-
bearing and rooted.

### 6.1 Persistent function handles

~~~rust
pub struct JsHandle {
    runtime: JsRuntimeId,
    value: Persistent<Function>,
    generation: u32,
}

pub enum Funcref {
    User(...),
    Builtin(...),
    Partial(...),
    Lambda(...),
    Js(JsHandle),
}
~~~

- A JS function crossing into Vim becomes a <code>Funcref::Js</code> with a
  persistent QuickJS handle.
- The VimL GC treats the handle as a leaf; the JS runtime's own GC keeps the
  function alive while the persistent handle exists.
- When the VimL GC collects the last reference, the handle is released on the
  next safe point (not inside the collector).
- When the JS runtime is destroyed, all handles are invalidated; any remaining
  Vim Funcref calls fail with a clear "JS runtime is gone" error.
- Generation checks prevent use-after-free if a handle is reused (it is not).

---

## 7. Modules

### 7.1 Resolution

For <code>import x from "specifier"</code>:

1. Relative specifiers resolve against the importing file.
2. Bare specifiers resolve against:
   - <code>$VIMRUNTIME/js/</code>,
   - each <code>'runtimepath'</code> entry's <code>js/</code> directory,
   - each pack's <code>js/</code> directory.
3. Try exact path, then <code>.js</code>, then <code>.mjs</code>, then
   <code>/index.js</code>.
4. No <code>node_modules</code>, no npm, no network, no <code>node:</code>
   builtins.

Resolution is deterministic and cached by absolute path. A module is evaluated
once per runtime; circular imports follow the ES module live-binding rules
(QuickJS provides them).

### 7.2 Directory conventions

~~~text
runtime/js/                 shipped JS modules (optional; not required by Vim)
~/.vim/js/                  user modules
pack/*/start/*/js/          package modules
plugin/**/*.js              auto-loaded plugin entry points
~~~

<code>plugin/**/*.js</code> files are sourced as module entry points at startup
(after <code>.vim</code> plugins in the same directory), in runtimepath order,
with <code>after/</code> last. Each plugin should guard with
<code>vim.g.loaded_&lt;name&gt;</code>.

<code>:jsfile</code> can source any file; the loader records it in
<code>:scriptnames</code>.

---

## 8. Promises, timers and jobs

QuickJS has a job queue for promises/microtasks. The editor drives it:

- After every JS command, callback and job/timer callback, drain the job queue
  until empty (with a budget to avoid starvation).
- A Promise may resolve from a timer or job callback. The editor does not block
  waiting for a Promise.
- <code>async</code> functions may use <code>await</code>; the continuation runs
  when the awaited Promise resolves on the main thread.
- <code>vim.defer(fn, ms)</code> schedules a one-shot timer and returns a
  Promise; it is sugar over the timer API.
- On shutdown, pending promises are rejected with a shutdown error; timers and
  jobs are stopped first.

~~~js
async function saveAndNotify() {
  await vim.fn.systemlist("git status");
  vim.notify("git status finished");
}

vim.api.createAutocmd("BufWritePost", {
  pattern: "*.rs",
  callback: () => { void saveAndNotify(); },
});
~~~

No JS runs from a background thread. "Async" means "later on the main loop",
not "parallel".

---

## 9. Errors and diagnostics

- JS <code>throw</code> becomes a Vim error with:
  - a stable error code (RVim JS extension, allocated in the error registry),
  - the JS message and stack,
  - <code>v:exception</code>/<code>v:throwpoint</code> containing
    <code>js:file:line</code>,
  - the originating <code>:jsfile</code>/plugin in <code>:scriptnames</code>.
- Vim errors inside <code>vim.cmd</code>/<code>vim.fn</code> become JS
  <code>Error</code> objects with <code>.code</code> (e.g. <code>E492</code>)
  and <code>.vimError = true</code>; they can be caught with normal
  <code>try/catch</code>.
- <code>console.log/info/warn/error</code> go to the message system with
  appropriate highlights; <code>vim.notify(msg, level)</code> uses the same
  levels as Vim messages.
- A JS exception must not leave a half-applied editor command. Commands are
  applied atomically where Vim is atomic; otherwise the error reports what
  changed.
- Stack traces are preserved in <code>:messages</code> under verbose mode and in
  the crash log. Secrets are redacted.

---

## 10. Sandbox and limits

Default (trusted plugin) mode:

- <code>vim</code> API and ES modules from the runtimepath.
- No <code>std</code>/<code>os</code> QuickJS modules.
- No direct filesystem/process/network access; use Vim APIs
  (<code>readfile()</code>, <code>job_start()</code>, channels).
- Memory/stack/instruction limits.

Restricted mode (<code>-Z</code>, <code>:sandbox</code>):

- <code>vim.cmd</code> is limited to the same commands Vim's sandbox allows;
  shell/write/process commands fail.
- Module loading is restricted to trusted roots; user/project modules can be
  denied.
- <code>js_eval</code> of untrusted strings is still allowed only within the
  sandbox's API limits; it cannot escape through <code>vim</code>.
- <code>vim.notify</code>/messages remain available.

A plugin cannot gain OS access by importing a module: modules are JS only, and
the only host bindings are the <code>vim</code> API.

---

## 11. VimL bridge functions

| Function | Purpose |
|---|---|
| <code>js_eval({code} [, {opts}])</code> | Evaluate JS, return converted value. |
| <code>js_call({func}, {args})</code> | Call a JS function handle with a List of args. |
| <code>js_function({name})</code> | Resolve a global/module function to a Funcref. |
| <code>js_version()</code> | Return engine/build version. |
| <code>v:js_version</code> | Same, as a predefined variable. |

These are registered only when the feature is compiled in. They follow Vim
function conventions (argument count errors, return conversion), not JS calling
conventions.

VimL callbacks from JS: a JS function handle is accepted anywhere Vim expects a
Funcref (timers, jobs, autocommands, <code>map()</code>). The interpreter calls
it with converted arguments and converts the return value; a JS throw becomes a
Vim exception.

---

## 12. Testing

- **API contract:** every documented <code>vim.*</code> call has a test for
  success, type errors, Vim errors and sandbox denial.
- **Conversion:** primitives, BigInt bounds, NUL strings, arrays/objects, typed
  arrays, cycles, function identity.
- **GC:** JS function stored in VimL list/dict/register/timer/autocmd; VimL GC
  collects it; handle release; shutdown invalidation. Run under leak detectors.
- **Promises/jobs:** timer resolution, job exit, microtask order, no editor
  reentrancy, shutdown rejection.
- **Modules:** relative/bare resolution, extension precedence, missing module,
  circular import, duplicate load.
- **Limits:** memory limit, stack overflow, interrupt, timeout.
- **Conformance:** selected test262 cases for ES2023 features used by the API;
  document engine gaps rather than claiming full conformance.
- **Performance:** empty call, <code>vim.fn</code> call, list conversion, 10k
  callbacks, startup.

---

## 13. Example plugin

~~~js
// plugin/hello.js
if (!vim.g.loaded_hello) {
  vim.g.loaded_hello = true;

  vim.cmd("command! -nargs=1 Hello echo <q-args>");
  // Prefer vim.api for new code:
  vim.api.createUserCommand("Hello2", {
    nargs: "1",
    callback: (args) => {
      vim.notify("hello, " + args.args);
    },
  });

  vim.api.createAutocmd("BufEnter", {
    pattern: "*.md",
    callback: () => {
      vim.bo.textwidth = 80;
      vim.wo.wrap = true;
    },
  });
}
~~~

The first version uses a Vim command for illustration; new plugins should use
<code>vim.api</code> where it covers the operation, and <code>vim.fn</code> for
compatibility.

---

## 14. What QuickJS is not

- Not a replacement for legacy VimL in the runtime.
- Not Node: no <code>require</code>, npm, <code>process</code>, <code>fs</code>,
  <code>http</code> or native addons.
- Not a second UI: JS cannot draw cells or bypass statusline/tabline/cmdline
  widgets.
- Not a concurrency model: no worker threads, no JS on the I/O reactor.
- Not Vim9: classes/imports/types in Vim9 remain a separate deferred feature.

This boundary is deliberate: one script engine for compatibility (VimL), one
modern extension engine (QuickJS), and no overlap that would make behavior
ambiguous.
