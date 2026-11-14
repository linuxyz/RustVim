# Vim runtime compatibility

**Target:** the unmodified Vim 9.2.1338 <code>runtime/</code> tree.  
**Principle:** compatibility is measured by executing Vim's own runtime data, not
by reimplementing a lookalike set of plugins.

---

## 1. What "runtime compatibility" means

RVim must load and execute the files shipped in Vim's runtime:

| Directory | What it contains | Why it matters |
|---|---|---|
| <code>autoload/</code> | lazily loaded script libraries (netrw, matchparen helpers, archive plugins, ...) | plugins call these functions by name |
| <code>colors/</code> | colorschemes | <code>:colorscheme</code> must produce Vim-compatible highlights |
| <code>compiler/</code> | compiler definitions | <code>:compiler</code> and <code>:make</code> |
| <code>doc/</code> | help text and <code>tags</code> | <code>:help</code> is a core feature |
| <code>ftplugin/</code> | filetype-specific settings/mappings | daily editing of real languages |
| <code>indent/</code> | indentation expressions | <code>=</code>, autoindent, <code>indentexpr</code> |
| <code>keymap/</code> | keyboard layouts | <code>:loadkeymap</code>, <code>'keymap'</code> |
| <code>lang/</code> | menu/message translations | <code>:language</code> |
| <code>plugin/</code> | always-loaded plugins (matchparen, netrw, archives, ...) | startup behavior |
| <code>syntax/</code> | syntax definitions | highlighting for real languages |
| <code>pack/</code> | optional packages (matchit, netrw, editorconfig, ...) | <code>:packadd</code> |
| <code>spell/</code> | compiled dictionaries | <code>'spell'</code> |
| <code>import/</code> | Vim9 imports | deferred with Vim9 |

The pinned Vim 9.2.1338 tree contains 790 syntax files, 445 ftplugins, 191
indent files, 137 compiler definitions, 12,156 help tags and 63 non-test
first-line <code>vim9script</code> files. Test fixtures under
<code>testdir/</code> are excluded; they are not loaded by the editor.

---

## 2. Pinning and integrity

- The runtime is vendored as <code>runtime/</code> (preferred) or fetched by a
  reproducible script from tag <code>v9.2.1338</code>. It is never edited.
- A repository-level [runtime.lock](../runtime.lock) records the tag, source
  URL and archive SHA-256
  (<code>eeb4370a8db32041af1c3b05159bdeeea6b781f5a32d0f1f68bd0c51d45880f3</code>).
  Upstream has no <code>runtime/VIM_VERSION</code>; never add metadata files to
  the runtime tree.
- A manifest of SHA-256 hashes is generated for every runtime file. CI verifies
  it before tests.
- Updates are explicit: update version + manifest, run the inventory diff, run
  the compatibility matrix, then commit. No silent runtime drift.
- Licensing: Vim runtime files retain the Vim license and copyright notices;
  <code>COPYING</code>/<code>runtime/doc/uganda.txt</code> are preserved. The
  engine's own license is separate and must be compatible with redistribution
  of the runtime. This is reviewed before the first release.

---

## 3. Path semantics

RVim must reproduce Vim's <code>'runtimepath'</code> semantics:

- user config directory(s), system vimfiles, <code>$VIMRUNTIME</code>, and the
  corresponding <code>after/</code> directories, in Vim's documented order;
- platform-specific defaults (Unix/macOS/Windows) and <code>$VIM</code> /
  <code>$VIMRUNTIME</code> overrides;
- <code>:set runtimepath+=...</code> and comma escaping;
- <code>:runtime</code> sources the first match; <code>:runtime!</code> sources
  all matches in path order;
- <code>after/</code> entries are loaded after the main entries, not in the
  middle;
- every loaded script gets a script ID for <code>:scriptnames</code>,
  <code>&lt;SID&gt;</code> and error attribution.

The exact default list is copied from Vim 9.2 documentation and covered by a
table-driven test. We do not "simplify" it.

---

## 4. Startup and load order

The runtime observes this order (details in
[ARCHITECTURE.md](ARCHITECTURE.md#18-startup-sequence)):

1. <code>--cmd</code> commands (before vimrc).
2. vimrc (<code>~/.vimrc</code>, <code>$XDG_CONFIG_HOME/vim/vimrc</code> where
   Vim defines it, <code>$VIM/_vimrc</code> on Windows), unless
   <code>-u NONE</code>/<code>--clean</code>.
3. <code>plugin/**/*.vim</code> from <code>'runtimepath'</code>, in order,
   excluding <code>after/</code> until the second pass.
4. <code>pack/*/start/*/plugin/**/*.vim</code>.
5. <code>after/plugin/**/*.vim</code>.
6. <code>-c</code>/<code>+cmd</code>/<code>-S</code> commands, file arguments.
7. <code>VimEnter</code>, then the event loop.

<code>--noplugin</code> skips steps 3-5. <code>:packadd</code> loads an optional
package on demand, including its <code>plugin/</code> and <code>after/plugin/</code>
parts. <code>:packloadall</code> loads all start packages.

A plugin error during startup is reported with file/line; it does not silently
disable the rest of startup unless Vim would abort.

---

## 5. Filetype pipeline

~~~text
BufNewFile/BufRead
  -> filetype.vim rules (:setf / :setfiletype)
  -> scripts.vim content sniffing (when enabled)
  -> FileType event
  -> ftplugin/<ft>.vim, ftplugin/<ft>_*.vim
  -> indent/<ft>.vim
  -> Syntax event
  -> syntax/<ft>.vim
~~~

Requirements:

- <code>:setf</code> sets the filetype only if empty; <code>:setfiletype</code>
  forcibly sets it; <code>:filetype on/off/detect/plugin/indent</code>.
- <code>ftplugin.vim</code>/<code>indent.vim</code> loader scripts (the ones in
  the runtime root) must work: they implement the <code>b:did_ftplugin</code>
  and <code>b:did_indent</code> conventions.
- <code>b:undo_ftplugin</code>/<code>b:undo_indent</code> strings are executed on
  filetype change; they are part of compatibility, not optional cleanup.
- <code>after/ftplugin</code> and <code>after/indent</code> run after the main
  files.
- <code>FileType</code> autocommands and <code>:setfiletype</code> ordering follow
  Vim, including the "only once" guard.
- Syntax loading is separate from ftplugin/indent; <code>:syntax on</code>
  enables the <code>Syntax</code> event. A filetype can have syntax without an
  ftplugin.

---

## 6. Plugins, packages and autoload

- <code>plugin/**/*.vim</code> order follows <code>'runtimepath'</code>, with
  <code>after/</code> last.
- <code>pack/*/start/*</code> loads at startup; <code>pack/*/opt/*</code> only
  with <code>:packadd</code> (and <code>:packadd!</code> for after-only).
- <code>'packpath'</code> defaults and package directory conventions follow Vim.
- Autoload functions (<code>foo#bar#baz()</code>) source
  <code>autoload/foo/bar.vim</code> on first call and cache it by script ID.
  Load errors name the autoload file, not the caller.
- <code>:scriptnames</code> lists every sourced script, including runtime files,
  in load order.
- <code>:finish</code>, <code>:scriptencoding</code> and
  <code>:scriptversion</code> behave as Vim's legacy script loader expects.

---

## 7. Help and tags

- <code>doc/tags</code> is a tab-separated tag/file/search-command index. It is
  parsed once and cached; duplicate tags resolve like Vim.
- <code>:help</code> creates a help buffer with <code>'buftype=help'</code>,
  <code>'iskeyword'</code> including <code>-</code>, tag navigation,
  <code>CTRL-]</code>/<code>CTRL-t</code>/<code>CTRL-o</code> and
  <code>:helpclose</code>.
- <code>:helptags</code> regenerates tags from <code>doc/*.txt</code> using
  Vim's tag syntax (<code>*tag*</code>, duplicates, errors).
- <code>:helpgrep</code>/<code>:lhelpgrep</code> produce quickfix lists.
- <code>'helplang'</code> selects translations; missing translations fall back
  to English.
- A tag that points at a missing file/line is reported as a runtime error, not
  a crash. The inventory's 12,156 tags are a coverage test.

---

## 8. Vim9 runtime policy

The pinned tag's runtime contains 63 non-test Vim9 script files, including
<code>autoload/dist/*</code>, <code>autoload/context.vim</code>,
<code>:TOhtml</code>, HLSL support and optional packages. Because Vim9 is
deferred:

- The loader reads the first line. If it is <code>vim9script</code> or
  <code>vim9script noclear</code>, the file is skipped.
- Default: one warning per skipped file at verbose level, recorded in
  <code>:scriptnames</code> metadata and the compatibility matrix. Sourcing
  continues.
- Strict mode (<code>--vim9=error</code>): the source command fails with a
  stable error, useful for compatibility auditing.
- <code>has('vim9script')</code> is false, so runtime files that check it can
  branch. Files that do not are expected gaps.
- A Vim9 file is never partially parsed by the legacy engine; that would produce
  wrong behavior and confusing errors.
- The skip list is part of the release report. If a tier requires a Vim9 file,
  the tier is marked incomplete rather than patching the runtime.

---

## 9. Terminal descope and runtime behavior

The terminal emulator is descoped (see [EXCLUSIONS.md](EXCLUSIONS.md)):

- <code>has('terminal')</code> is false; <code>:terminal</code> and
  <code>term_*</code> are absent.
- Runtime files that gate on <code>has('terminal')</code> branch away cleanly.
- Files that assume terminal support are classified as expected gaps, not
  regressions. Terminal-specific ftplugins/syntax files are not loaded unless
  the user opens a terminal buffer (which is unsupported).
- Jobs and channels remain; <code>job_start(..., {'pty': 1})</code> still works.
  The absence of an emulator does not remove process I/O.
- The inventory keeps terminal commands/functions/options visible so the gap is
  measurable.

---

## 10. Compatibility tiers

Tiers are release gates, not marketing labels. A tier is complete only when its
matrix has no unexpected errors and the listed feature IDs pass.

The matrix is generated by `tools/runtime_compat.py`; see
[COMPATIBILITY.md](COMPATIBILITY.md) for the current snapshot, report schema
and the explicit gap list.  CI runs Tier A (gated), Tier B and Tier C (report
only) in `.github/workflows/runtime-compat.yml`.

### Tier A — core runtime

Goal: a user can edit real files with Vim's default look and feel.

- <code>defaults.vim</code>, <code>vimrc_example.vim</code>
- <code>filetype.vim</code>, <code>scripts.vim</code>
- <code>ftplugin.vim</code>, <code>indent.vim</code>, <code>ftoff.vim</code>,
  <code>indoff.vim</code>
- ftplugin/indent/syntax for the top ~30 languages
- all <code>colors/*.vim</code>
- common <code>compiler/*.vim</code>
- <code>plugin/matchparen.vim</code>, <code>plugin/getscriptPlugin.vim</code>,
  <code>plugin/rrhelper.vim</code> (if no hard GUI dependency)
- <code>doc/tags</code> and <code>:help</code>
- no Vim9/terminal/GUI errors classified as unexpected

### Tier B — common plugins and languages

- <code>plugin/netrwPlugin.vim</code> core, <code>pack/dist/opt/netrw</code>
- <code>pack/dist/opt/matchit</code>
- archive plugins (<code>tar</code>, <code>zip</code>, <code>gzip</code>,
  <code>vimball</code>)
- <code>plugin/spellfile.vim</code>
- ftplugin/indent/syntax for the long tail
- <code>:compiler</code> corpus
- <code>after/</code> conventions from popular plugins

### Tier C — everything in the pinned runtime

All files that are not explicitly excluded (GUI-only, Vim9-only, terminal-only,
optional scripting interfaces). Measured per file; known gaps listed.

---

## 11. Smoke harness

The harness is a first-class deliverable, not an ad-hoc script.

### 11.1 Plugin/autoload sourcing

~~~text
for each runtime .vim file:
  run a fresh rvim --headless --clean process
  set v:errmsg = ""
  source the file (or invoke its documented entry point for autoload)
  capture v:errmsg, messages, exit code
  classify:
    pass
    expected-gap: gui | vim9 | terminal | optional-feature
    fail
  write JSON: path, sha256, kind, status, error, stack
~~~

Sourcing every autoload file blindly is not meaningful (many require arguments);
for autoload, the harness calls known entry points from runtime plugins and
Vim's own tests where available.

### 11.2 ftplugin/indent

~~~text
create sample files per filetype
:filetype plugin indent on
:set filetype=<ft>
assert b:did_ftplugin, b:undo_ftplugin (if defined)
assert no error
run gg=G on indent samples; compare text with Vim
switch filetype; assert b:undo_* restores options
~~~

### 11.3 Syntax

~~~text
:filetype plugin indent on
:syntax on
open a sample file
assert no error while loading syntax/<ft>.vim
sample known positions -> synID()/synIDattr() traces
compare with Vim's traces (normalized group names)
run :syntime on a large sample; enforce budget
~~~

### 11.4 Help

~~~text
parse doc/tags
for each tag:
  resolve file/search command
  :help <tag> in headless mode
  assert the tag is found and the file exists
~~~

### 11.5 Colors, compilers, packages

- <code>:colorscheme</code> each shipped scheme; parse <code>:hi</code> output
  and compare selected groups.
- <code>:compiler</code> each definition; assert <code>b:current_compiler</code>
  and no errors.
- <code>:packadd</code> each optional package; assert load and no unexpected
  error.

### 11.6 Vim9/terminal/GUI classification

The harness has explicit classifiers, so a failure caused by a descoped feature
cannot be mistaken for a core regression. Classifiers are reviewed when scope
changes.

---

## 12. Compatibility matrix schema

~~~json
{
  "runtime": "9.2.1338",
  "rvim": "0.1.0-dev",
  "generated": "2026-11-13T00:00:00Z",
  "files": [
    {
      "path": "syntax/rust.vim",
      "sha256": "...",
      "kind": "syntax",
      "status": "pass",
      "classification": null,
      "error": null,
      "duration_ms": 12
    },
    {
      "path": "indent/vim.vim",
      "sha256": "...",
      "kind": "indent",
      "status": "expected-gap",
      "classification": "vim9",
      "error": "E...: vim9script skipped",
      "duration_ms": 1
    }
  ],
  "summary": {
    "pass": 0,
    "expected_gap": 0,
    "fail": 0
  }
}
~~~

The matrix is committed per release and diffed in CI. A new failure blocks the
release unless it is a documented expected gap with a catalog ID.

---

## 13. Release gates

- **Tier A:** 100% pass, no unexpected errors; help tags resolve; Tier A syntax
  corpus loads; startup under budget.
- **Tier B:** no regressions; at least the documented pass threshold; all
  failures classified.
- **Tier C:** measured; known gaps listed; no crash.
- **Vim9/terminal/GUI gaps:** visible counts, no silent failures.
- **Diff:** runtime manifest unchanged, inventory diff reviewed, no new
  unclassified failure.

---

## 14. Adding or updating a runtime tier

1. Update the pinned runtime and manifest.
2. Regenerate inventories (<code>tools/gen_inventory.py</code>).
3. Review new/changed files; classify new features.
4. Update the catalog if a new runtime file requires new behavior.
5. Run the smoke harness and the oracle tests.
6. Publish the matrix diff and known gaps.

The runtime is large, but the process is mechanical. That is the point: no
hand-waving about "most plugins work".

---

## 15. Known risks

- **Runtime assumptions:** some files assume GUI/Vim9/terminal; honest
  <code>has()</code> flags turn them into branches, not crashes.
- **VimL edge cases:** netrw and matchit exercise obscure legacy behavior;
  they are Tier B, not Tier A.
- **Syntax performance:** large files + complex syntax can exceed the redraw
  budget; incremental sync and <code>:syntime</code> guard it.
- **Help encoding:** shipped docs may contain legacy encodings; the help loader
  must convert or display deterministically.
- **License drift:** runtime updates can change notices; the manifest and
  release checklist include a license review.

---

## 16. Why this is worth the cost

A Vim clone that cannot load Vim's runtime is a different editor with Vim
keybindings. Runtime compatibility gives us:

- a huge, free regression suite for VimL, regex, syntax, ftplugin and options;
- real-world filetype coverage without maintaining hundreds of plugins;
- a clear definition of done (measured files, not opinions);
- a migration path for users: their existing <code>~/.vim</code> and packages
  keep working, except for the documented exclusions.
