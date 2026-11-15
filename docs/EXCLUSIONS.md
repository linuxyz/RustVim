# Scope exclusions and gates

This document records what RVim deliberately does not implement in the current
phase, how the engine behaves when those features are encountered, and what it
would take to add them later.

Scope is a product decision, not an accident. Every exclusion has:
- a catalog consequence (feature absent or marked deferred),
- a runtime consequence (<code>has()</code>/<code>exists()</code> answers),
- an error contract (what the user sees),
- a re-entry criterion.

---

## 1. Summary

| Area | Status | Runtime answer | Re-entry |
|---|---|---|---|
| Vi/Ex compatibility mode | **Excluded** | <code>has('compatible')</code> false; <code>'compatible'</code> cannot be enabled | Not planned; Vim semantics only |
| Vim9 script | **Supported (runtime-grade)** | <code>has('vim9script')</code> true; 19 Tier C checks + compiler/context pass | Remaining limits in section 3 - [VIM9-PLAN.md](VIM9-PLAN.md) |
| GUI | **Interface only** | <code>has('gui')</code> and <code>has('gui_running')</code> false | A new <code>rvim-gui</code> backend |
| Terminal emulator | **Descoped** | <code>has('terminal')</code> false; no <code>:terminal</code>/<code>term_*</code> | Dedicated TERM milestone if re-approved |
| Printing | Deferred (P4 optional) | <code>has('printer')</code> false; <code>:hardcopy</code> errors | MISC-02 |
| RTL/Arabic | Deferred (P4 optional) | <code>has('rightleft')</code> false | MISC-01 |
| Client-server | Deferred (P4 optional) | <code>has('clientserver')</code> false | JOB-08/MISC-03 |
| Images (<code>+image</code>) | Deferred (P4 optional) | <code>has('image')</code> and backend flags false; <code>image_*()</code> absent | MISC-12 |
| NetBeans/workshop | Excluded | <code>has('netbeans_intg')</code> false | Not planned |
| Python/Lua/Perl/Ruby/Tcl/MzScheme | Excluded | corresponding <code>has()</code> false | Not planned; QuickJS is the extension language |
| Neovim API | Excluded | <code>has('nvim')</code> false | Not planned |
| Built-in LSP/tree-sitter | Excluded | not a Vim 9.2 feature | Not planned |

<code>--stdio-channel</code> is the one Vi/Ex-adjacent exception: it implies a
silent <code>-es</code> startup to host a stdio channel, but provides no
interactive Ex mode, Vi defaults or Ex text-entry workflow.

---

## 2. Vi/Ex compatibility is excluded

### What is excluded

- The <code>vi</code>/<code>ex</code> binaries and generic <code>-e</code>,
  <code>-E</code>, <code>-s</code> interactive Ex-mode startup. The silent
  <code>-es</code> startup implied by <code>--stdio-channel</code> is allowed
  only as a channel host mode (JOB-03), not as an Ex compatibility layer.
- Vi-compatible defaults and <code>'compatible'</code> behavior.
- Ex mode as an editing mode: <code>Q</code>, <code>gQ</code>, <code>:visual</code>
  as an Ex-mode toggle, and the "entering Ex mode" UI.
- Ex text-entry commands as a supported editing workflow:
  <code>:append</code>, <code>:insert</code>, <code>:change</code> and
  <code>:open</code> are marked <code>Ex-legacy</code> in the inventory and are
  not in the active catalog.

### What remains in scope

- Vim's Ex **command language**: <code>:</code> commands, ranges, addresses,
  <code>:global</code>, <code>:substitute</code>, <code>:normal</code>, user
  commands, command-line window and command-line completion. This is how Vim
  users and runtime scripts drive the editor; it is not "Vi compatibility".
- Vim defaults and Vim behavior when <code>'compatible'</code> is off.
- <code>--stdio-channel</code> channel-host startup: no UI, startup commands
  only, then a stdio channel; it does not add interactive Ex mode.

### Runtime contract

~~~text
:set compatible?      -> 0
:set compatible       -> E... (Vi-compatible mode is not supported)
:set nocompatible     -> accepted (already the only mode)
has('compatible')     -> 0
exists('+compatible') -> 1 (the option exists for introspection)
~~~

The option exists so scripts that query it do not break. Enabling it fails
loudly; it never silently changes behavior. <code>Q</code>/<code>gQ</code> beep
with a message pointing at this document; they do not enter a half-implemented
Ex mode.

### Why

Supporting Vi/Ex compatibility would double the option/semantic matrix for a
mode almost no target user wants. The budget is better spent on Vim's runtime,
regex, scripting and UI decoupling.

### Re-entry criterion

A future decision to support Vi/Ex would require a separate compatibility
profile, a complete <code>'compatible'</code> option matrix, Ex-mode state and
tests against the <code>ex</code> binary. It is not a small flag.

---

## 3. Vim9 script support and documented limits

Status: runtime-grade Vim9 script is implemented.  The lexer, AST, parser,
evaluator, classes, interfaces, enums, type aliases, imports/exports and the
editor script loader live in <code>crates/rvim-viml/src/vim9/</code> plus the
editor host bridge.  <code>has('vim9script')</code> is 1; the 19 Tier C Vim9
checks and <code>compiler/context.vim</code> pass; the compatibility matrix is
A 14/14, B 48/48, C 789/790 (only the environment-dependent
<code>syntax/glimmer.vim</code> artefact remains); the Vim9 differential corpus
passes 26/26 oracle-checked cases and the workspace has 635 tests.

Documented approximations (deliberately not claimed as full parity):

- No strict Vim9 static type checker.  Type annotations are parsed and used
  for defaults and object members, but type errors are mostly caught at run
  time and diagnostics are less precise than Vim's compiler.
- Legacy <code>:import</code> of a Vim9 file binds a namespace of no-op
  function stubs; calls into compiled Vim9 functions from a legacy script are
  not executed (Vim9-to-Vim9 calls and Vim9 autoload work).
- Vim9 scripts sourced from a legacy script execute against the shared editor
  runtime, but Vim9 callables defined there are not retained for later legacy
  calls.
- vim9-mix files with legacy code before the header are supported by running
  the legacy preamble then the Vim9 part; mixed-mode <code>:def</code> blocks
  inside legacy scripts are parsed and skipped.
- Generic type parameters are accepted/parsed, call-site type arguments are
  ignored at run time, and class generics are accepted more permissively than
  Vim.  Interface conformance is not fully checked at compile time.
- <code>super</code> member access is not implemented (neither the corpus nor
  the pinned runtime files need it).
- Vim's own test_vim9_*.vim suite has not been run in full; hardening is
  tracked in Phase 5 of [VIM9-PLAN.md](VIM9-PLAN.md).

### Runtime impact

The pinned tree contains 63 non-test files whose first line is
<code>vim9script</code> (including <code>vim9script noclear</code>), notably
<code>autoload/dist/*</code>, <code>autoload/context.vim</code>,
<code>:TOhtml</code>, HLSL support and optional help packages. Under the default
policy:

- Vim9-only indent/ftplugin/syntax files are unavailable; the filetype may
  still load its legacy parts.
- <code>:TOhtml</code>, context/dist helpers and optional packages that are
  Vim9-only are unavailable; they are classified as expected gaps, not
  regressions.
- <code>has('vim9script')</code> is false, so well-written runtime files branch
  away; files that assume Vim9 fail and are classified as expected gaps.
- <code>:scriptnames</code> and verbose logs report skipped files.

### Why

Vim9 script is a second language with a compiler, type system and class model.
Implementing it alongside legacy VimL would delay the editor by months and
distract from runtime compatibility. Deferring it is explicit, testable and
reversible.

### Re-entry criterion

A separate VIM9 milestone with its own parser/compiler, type checker, class
model, import system and conformance suite. It must not destabilize legacy
VimL or runtime loading.

---

## 4. GUI is interface-only

### What is deferred

- Any window/toolkit implementation (winit, wgpu, GTK, Cocoa, etc.).
- Fonts, ligatures, pixel scrolling, native menus, native tab bars, native
  scrollbars, IME preedit windows.
- GUI-only options' visual effects (<code>'guifont'</code>,
  <code>'guioptions'</code>, <code>'toolbar'</code>, <code>'mouseshape'</code>).
- <code>gvim</code>, <code>:gui</code>, <code>has('gui_running')</code>.

### What is provided

- <code>UiBackend</code>/<code>UiCapabilities</code> in
  [UI-PROTOCOL.md](UI-PROTOCOL.md).
- Optional GUI extension traits for pixels, IME, menus, scrollbars and
  decorations (design only).
- Frames that are already backend-neutral; a GUI consumes the same cells as
  the TUI.
- GUI-only options are stored, so runtime scripts that set them do not error
  merely because a GUI is absent; <code>has('gui')</code> remains false.

### Runtime contract

~~~text
has('gui')          -> 0
has('gui_running')  -> 0
:gui                -> E... (no GUI in this build)
:set guifont=...    -> accepted and stored; no visual effect
~~~

Menus defined by <code>menu.vim</code> are parsed and can be used through
<code>:emenu</code>; they are not drawn as native menus.

---

## 5. Terminal emulator is descoped

This is a scope reduction from the original plan. The virtual terminal,
<code>:terminal</code>, Terminal-Job mode and <code>term_*</code> API are not
implemented in the current roadmap.

### What is excluded

- VT100/xterm/ECMA-48 parsing and screen/scrollback model.
- <code>:terminal</code>, <code>term_start()</code>, <code>term_sendkeys()</code>,
  <code>term_wait()</code>, <code>term_get*()</code>, <code>term_set*()</code>,
  <code>term_dump*()</code>, <code>terminalprops()</code>.
- Terminal buffers (<code>'buftype=terminal'</code>), Terminal-Job/Normal modes,
  <code>'termwinkey'</code>, <code>'termwinsize'</code>,
  <code>'termwinscroll'</code>, <code>:tmap</code> and terminal mouse/key
  forwarding.
- Terminal session save/restore and terminal-specific autocommands.

### What remains

- **Jobs and channels** remain in scope: <code>job_start()</code>,
  <code>job_stop()</code>, channels, callbacks, timers, <code>:make</code>
  plumbing and <code>:!cmd</code>. A job started with <code>pty</code> still
  gets a PTY; that is process I/O, not terminal emulation.
- **TUI infrastructure** remains: raw mode, alternate screen, resize, mouse,
  bracketed paste, terminfo and terminal input decoding for the editor itself.
- The mechanical inventory still lists terminal commands/functions/options so
  the gap is auditable.

### Runtime contract

~~~text
has('terminal')             -> 0
:terminal                   -> E... (terminal emulator is not built)
term_start()                -> E117: Unknown function
'buftype' = "terminal"      -> E... (terminal buffers are not supported)
TerminalOpen/TerminalWinOpen -> never fired
~~~

Runtime files that check <code>has('terminal')</code> branch away cleanly.
Files that assume it are classified as expected gaps in the compatibility
matrix. They are not patched.

### Why

The terminal emulator is a large, self-contained product (VT parser, scrollback,
PTY lifecycle, modes, colors, mouse, sessions) that does not advance core
editing, VimL, runtime compatibility or the UI contract. Jobs/channels already
cover the automation use cases that do not need an interactive terminal.

### Re-entry criterion

If re-approved, it returns as an isolated <code>rvim-term</code> feature with
its own phase, golden VT-stream tests and no dependency from the editor core.
The descope is recorded in the catalog and roadmap so re-entry is deliberate.

---

## 6. Optional Vim features deferred or excluded

| Feature | Decision | Contract / reason |
|---|---|---|
| Printing, <code>:hardcopy</code> | Deferred (P4) | <code>has('printer')</code> false; command errors. Platform print pipelines are costly. |
| Right-to-left/Arabic | Deferred (P4) | <code>has('rightleft')</code> false; <code>'rightleft'</code> unavailable/off. Requires bidi shaping. |
| Client-server | Deferred (P4) | <code>has('clientserver')</code> false; <code>--servername</code>/<code>remote_*</code> absent. |
| NetBeans/workshop | Excluded | Legacy IDE protocols, no user demand. |
| Sound | Deferred (P4) | <code>has('sound')</code> false; sound functions absent. |
| Images, sixel/kitty, popup images | Deferred (P4) | <code>has('image')</code> and backend flags false; <code>image_*()</code> absent; popup image attributes error clearly. |
| Python/Lua/Perl/Ruby/Tcl/MzScheme | Excluded | QuickJS is the extension language; multiple embedded runtimes would multiply build/ABI risk. |
| Neovim API/UI | Excluded | Not Vim 9.2; we have our own protocol. |
| Built-in LSP/tree-sitter/AI | Excluded | Not Vim 9.2 features. |
| GUI menus/toolbar | Interface only | Parsed for <code>:emenu</code>; no native drawing. |

### Image contract

Images are deferred as MISC-12, not approximated. The default build reports
<code>has('image')</code>, <code>has('image_sixel')</code>,
<code>has('image_kitty')</code> and the GUI image flags as 0;
<code>exists('*image_add')</code> is false; a popup <code>image</code> attribute
is rejected with a clear error. <code>'imageprotocol'</code> is registered as an
unavailable option and setting it errors rather than silently accepting a value
that will never render. A future backend implements the same popup/cell
contract through <code>UiCapabilities.images</code> without changing editor
semantics.

---

## 7. Unsupported-feature policy

1. **Compile-time gates:** a feature absent from the build is absent from the
   registry; <code>exists('*f')</code> is false and <code>has()</code> is false.
2. **Runtime errors:** invoking a known-but-unsupported command/function returns
   a stable <code>E###</code>-style error naming the feature and this document.
   It does not silently no-op.
3. **Runtime files:** the loader classifies failures as pass, expected gap
   (GUI/Vim9/terminal/optional) or regression. Expected gaps are visible in the
   dashboard and release notes.
4. **No fake implementations:** never return empty success for a feature that
   would change buffer state or security. A wrong answer is worse than an error.
5. **Documentation:** every exclusion appears here and in the catalog; every
   deferred item has a re-entry criterion.
6. **Tests:** negative tests assert the error/flag, so an accidental
   implementation cannot silently change the contract.

---

## 8. Changing scope

Scope changes are explicit:

1. Edit [FEATURES.md](FEATURES.md) (add/remove the feature row and dependencies).
2. Run <code>python3 tools/validate_catalog.py</code>.
3. Update this document's status table and runtime contract.
4. Update [ROADMAP.md](ROADMAP.md) and, if needed, the architecture.
5. Add acceptance tests, including the negative case.

The terminal descope and the Vim9 deferral are examples of this process: the
catalog, architecture, runtime policy and tests must agree.
