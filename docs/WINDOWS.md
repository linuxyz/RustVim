# Windows, tabs and the presentation layer

**Status:** design baseline. Windows live with the UI model, not in the buffer
model and not in the backend. This document explains where the line is and how
layout, viewports and rendering work.

---

## 1. Why windows belong to the UI layer

A buffer is text + metadata. It has no cursor, no scroll position, no folds and
no screen rectangle. A Vim window is a viewport: the same buffer can be visible
in two windows with independent cursors and folds.

Therefore:

| Concept | Owner | Depends on |
|---|---|---|
| Buffer text, marks, undo, options | editor core | no UI |
| Window (viewport, cursor, folds, w: vars) | <code>rvim-view</code> | buffer IDs, cell geometry |
| Tab page (window tree, current window) | <code>rvim-view</code> | windows |
| Layout and compositing | <code>rvim-view</code> | cells, not backends |
| Pixels, glyph rasterization, native menus | backend | frame only |

This satisfies the requirement that window implementation is combined with the
UI layer while keeping the backend replaceable. The window model is UI-level,
but it is not TUI-level: it produces cells, not ANSI bytes.

---

## 2. Data model

~~~rust
pub struct ViewState {
    pub windows: Arena<Window>,
    pub tabs: Arena<TabPage>,
    pub current_tab: TabId,
    pub layout: LayoutTree,
    pub compositor: Compositor,
    pub popups: PopupStack,
    pub messages: MessageArea,
    pub cmdline: CmdlineWidget,
}

pub struct Window {
    pub id: WindowId,
    pub buffer: BufferId,
    pub viewport: Viewport,
    pub cursor: BufPos,
    pub folds: FoldState,
    pub options: WindowOptions,
    pub vars: VimDict,
    pub statusline: StatuslineCache,
    pub syntax: SyntaxViewCache,
    pub dirty: DirtyFlags,
}

pub struct Viewport {
    pub topline: u32,        // first buffer line, 1-based
    pub topfill: u16,        // filler lines above topline (diff)
    pub leftcol: u32,        // first screen column for nowrap
    pub skipcol: u32,        // skipped screen columns for wrapped smoothscroll
    pub curswant: u32,       // desired column for vertical motion
    pub width: u16,
    pub height: u16,
}

pub struct TabPage {
    pub id: TabId,
    pub windows: LayoutTree,
    pub current: WindowId,
    pub vars: VimDict,
    pub options: TabOptions,
    pub cwd: Option<PathBuf>,
}
~~~

<code>WindowId</code>, <code>TabId</code> and <code>BufferId</code> are
generational IDs. The editor owns <code>ViewState</code>; the buffer model never
holds window references.

---

## 3. Layout tree

Vim's layout is a tree of splits. We use an explicit frame tree:

~~~rust
pub enum LayoutNode {
    Leaf(WindowId),
    Split {
        dir: SplitDir,          // Horizontal (stacked) | Vertical (side by side)
        children: Vec<LayoutNode>,
        sizes: Vec<SizeWeight>, // fixed cells or weights
    },
}

pub enum SplitDir { Horizontal, Vertical }
~~~

### 3.1 Geometry computation

Input: screen rectangle minus tabline, tabpanel, cmdline and message rows.
Output: a <code>Rect</code> per leaf, plus separator, statusline and tabpanel
rectangles. Statusline height is a per-window input, not a constant.

Rules:

- Separators occupy one cell column/row between siblings.
- Each window has a minimum width/height from
  <code>'winminwidth'</code>/<code>'winminheight'</code>.
- Fixed-size windows (<code>'winfixheight'</code>/<code>'winfixwidth'</code>)
  keep their size; remaining space is distributed.
- <code>'equalalways'</code>/<code>'eadirection'</code> rebalance on
  open/close/resize unless fixed.
- A new split copies the current window's size and halves the available space,
  honoring <code>'splitbelow'</code>/<code>'splitright'</code>.
- The tree is normalized after close: a split with one child collapses; sizes
  are clamped; zero-size windows are impossible.
- Terminal resize is just a new screen rectangle; the same algorithm runs and
  must be idempotent.

Layout is deterministic and side-effect free. Tests feed sequences of
open/close/resize and compare rectangles.

### 3.2 Statusline placement

Each window normally owns a statusline at its bottom (except when
<code>'laststatus'</code> is 0 and only one window). The statusline is part of
the window's allocated rectangle, not an overlay. Its height is 1..N, derived
from <code>'statuslineopt'</code> (<code>maxheight</code>, <code>fixedheight</code>)
and <code>%@</code> newlines; text height is reduced by the actual height. A
global statusline (<code>'laststatus'</code>=3) follows the same height rules at
the bottom of the screen.

The tabline (when shown) is a full-width row at the top. The tabpanel, when
<code>'showtabpanel'</code> is on, is a vertical sidebar on the left or right;
its width comes from <code>'tabpanelopt'</code> and it reserves columns from the
window area. Cmdline and messages occupy rows at the bottom. These rows are
owned by the presentation layer and computed before window layout.

---

## 4. Viewport: buffer coordinates to screen rows

A window renders a slice of the buffer. The mapping is nontrivial because of
wrapping, folds, diff fillers and smooth scrolling.

### 4.1 Line mapping

~~~text
screen row 0..height-1
  -> optional filler lines (diff)
  -> displayed line at topline
  -> physical lines, with folds collapsed
  -> wrapped screen lines within a physical line
  -> skipcol for smoothscroll
~~~

- <code>topline</code> is a buffer line number.
- A fold start line may hide N lines; the fold text occupies one screen row.
- <code>'wrap'</code> splits a long line into screen rows; <code>skipcol</code>
  skips part of the first wrapped row when <code>'smoothscroll'</code> is on.
- <code>'number'</code>, <code>'signcolumn'</code>, <code>'foldcolumn'</code>
  and <code>'colorcolumn'</code> reduce the text width; they do not shift the
  buffer column mapping.
- <code>'linebreak'</code>/<code>'breakindent'</code> affect where a wrapped row
  breaks and how continuation rows are indented.

The viewport caches a <code>ScreenMap</code>: for each screen row, the buffer
line and byte range it displays, plus flags (fold, filler, wrapped, truncated).
The cache is invalidated by <code>changedtick</code>, option changes, resize and
fold changes.

### 4.2 Cursor

The cursor is stored as a buffer position (<code>lnum</code>, byte
<code>col</code>). <code>curswant</code> remembers the desired screen column for
vertical motion across short lines and tabs. Rendering converts it to a screen
cell using the screen map; if the cursor would be off-screen, scrolling adjusts
<code>topline</code>/<code>skipcol</code> first.

Cursor visibility rules:

- <code>'scrolloff'</code>/<code>'sidescrolloff'</code> keep context around the
  cursor.
- The cursor is never hidden by a fold; opening the fold or moving to its start
  is required.
- In a wrapped line, the cursor may be on a continuation row; <code>gj/gk</code>
  move by screen row, <code>j/k</code> by buffer line.
- Virtual text can shift the cursor; the mapping belongs to the screen map.

### 4.3 Scrolling

<code>CTRL-E/Y</code>, <code>CTRL-D/U</code>, <code>CTRL-F/B</code>,
<code>zz/zt/zb</code>, <code>H/M/L</code> and mouse wheel all mutate
<code>topline</code>/<code>skipcol</code> and then clamp the cursor. The scroll
logic is shared; commands differ only in how many screen rows they request and
where the cursor ends up.

<code>'scrollbind'</code>/<code>'cursorbind'</code> synchronize scroll/cursor
across windows of the same buffer (or compatible buffers); the sync is applied
after the initiating window moves, with loop guards.

---

## 5. Rendering a window

Order within a window rectangle:

1. **Background** for the text area and gutters.
2. **Filler lines** (diff) and fold rows.
3. **Text**: for each screen row, apply:
   - buffer text and tabs,
   - <code>'list'</code>/<code>'listchars'</code>,
   - conceal,
   - syntax highlight spans,
   - search matches,
   - text properties and virtual text,
   - selection/visual highlight,
   - cursorline/column.
4. **Gutters**: number/relativenumber, sign column, fold column, colorcolumn.
5. **Separators** and **statusline**.
6. **Window-local overlays** (preview, popup anchored to this window).

The compositor then applies tabline, global statusline, cmdline, messages,
popup menu and floating windows in z-order.

### 5.1 Highlight priority

Vim has precise priority rules. The renderer resolves, from lowest to highest:

- normal text,
- syntax groups,
- <code>CursorLine</code>/<code>ColorColumn</code>,
- search/incsearch,
- visual/select,
- text properties,
- signs/fold text,
- cursor (drawn by backend, not composited into cells),
- popup menu selection.

Exact precedence is tested against Vim with <code>synID()</code> traces and
golden frames. A wrong priority is a compatibility bug, not a cosmetic detail.

### 5.2 Wide characters and tabs

The renderer expands tabs according to <code>'tabstop'</code>/<code>'vartabstop'</code>
and display width. A wide character occupies two cells; the screen map records
the byte range for both. A cursor on the trailing cell is normalized to the
leading cell. A line truncated at the right edge never leaves half a wide
character in the last cell; it is replaced by the truncation marker.

---

## 6. Window lifecycle

### 6.1 Splitting

<code>:split</code>/<code>:vsplit</code>/<code>:new</code>/<code>:vnew</code>:

- create a window with the same buffer (or a new empty buffer for <code>:new</code>),
- copy cursor, viewport, folds and window-local options,
- halve the parent's space, honoring <code>'splitbelow'</code>/<code>'splitright'</code>,
- make the new window current,
- fire <code>WinNew</code>, <code>WinEnter</code>/<code>WinLeave</code>.

### 6.2 Closing

- <code>:close</code>/<code>:quit</code>/<code>CTRL-W c/q</code>; refuse if it is
  the last window (unless quitting).
- If the buffer is modified and would be hidden, follow <code>'hidden'</code>,
  <code>'bufhidden'</code> and confirm/error rules.
- <code>'winfixbuf'</code>: commands that would change the window's buffer are
  rejected with the documented message.
- Fire <code>WinClosed</code> before the window is removed, then
  <code>WinEnter</code> for the next window.
- Collapse the layout tree and rebalance.

### 6.3 Tabs

A tab page is an independent window tree. <code>:tabnew</code> creates a tab
with one window; <code>:tabclose</code> closes the tab (and its windows);
<code>:tabonly</code> closes others. Closing the last window in a tab closes the
tab. Tab-local variables/options and the tab's working directory follow Vim.

---

## 7. Statusline, tabline and tabpanel integration

The window owns a <code>StatuslineCache</code>:

- evaluate <code>'statusline'</code> for this window (with
  <code>g:statusline_winid</code>),
- expand items and highlights,
- lay out left/center/right segments in the window width,
- register click regions for statusline items.

The tabline widget does the same at screen level, using tab metadata. The
tabpanel is a vertical variant: it lays out <code>'tabpanel'</code> in a column
strip, tracks its own scroll offset and emits tab-selection, scrollbar and
<code>%[Func]</code> regions. None of these widgets knows the backend. The
compositor calls them in the same pass as buffer text, so a headless frame
contains statusline/tabline/tabpanel cells exactly like the TUI would draw them.

---

## 8. Mouse and hit regions

Screen and window regions:

- separator drag: resize the split tree,
- statusline: focus window, click items, `%[Func]` callbacks,
  double-click equalize,
- tabline/tabpanel: select/close/reorder (reorder deferred), scrollbar drag,
- sign/fold columns: jump/expand,
- text area: set cursor, start visual selection, drag.

The backend sends coordinates; the core hit-tests the last frame. During a
drag, the core captures the mouse until release, so leaving the window does not
cancel the operation. Resizing a split updates the layout immediately and
re-renders; the buffer does not change.

---

## 9. Borrowing and mutation during render

Rendering takes <code>&mut ViewState</code> and <code>RenderContext&lt;'_&gt;</code>
(immutable model). It may update caches inside <code>ViewState</code>, but it
must not execute user scripts or mutate buffers. Statusline/foldtext
expressions run in the preceding evaluation phase, producing plain segments.

This separation prevents:

- reentrant buffer mutation while a window is being painted,
- aliasing <code>&mut Window</code> with <code>&Buffer</code>,
- hidden autocommands during render,
- nondeterministic frames in tests.

If a future feature needs a callback during render, it is modeled as a
pre-render query with an explicit cache.

---

## 10. Scripting APIs

Window/tab functions are implemented as builtins over <code>ViewState</code>:

- <code>winnr()</code>, <code>win_getid()</code>, <code>win_id2win()</code>,
  <code>win_findbuf()</code>, <code>win_gotoid()</code>, <code>winbufnr()</code>,
  <code>winheight()</code>, <code>winwidth()</code>, <code>wincol()</code>,
  <code>winline()</code>,
- <code>getwininfo()</code>, <code>gettabinfo()</code>,
  <code>winlayout()</code>, <code>win_execute()</code>,
  <code>winsaveview()</code>/<code>winrestview()</code>,
  <code>winrestcmd()</code>,
- <code>tabpagenr()</code>, <code>tabpagebuflist()</code>,
  <code>tabpagewinnr()</code>, <code>gettabwinvar()</code>,
  <code>settabwinvar()</code>.

Return values and failure modes follow Vim (0/ -1/ empty list), not Rust
<code>Option</code> leakage. Invalid IDs are errors or documented sentinels.

---

## 11. Future GUI mapping

The same <code>LayoutTree</code> produces cell rectangles. A GUI backend can:

- map cell rectangles to pixels via <code>PixelMetrics</code>,
- draw native separators/statusline/tabline or use the cells,
- add native scrollbars using viewport metadata,
- implement tab drag by translating pixels to tab hit regions,
- render fonts/ligatures while preserving cell boundaries.

None of this changes the layout model. The GUI is a renderer with richer output,
not a second window manager.

---

## 12. Testing

- **Geometry:** table-driven split/close/resize sequences; assert exact
  rectangles and min sizes.
- **Screen map:** wrapping/folds/smoothscroll/skipcol cases with buffer positions
  and expected screen rows.
- **Golden frames:** multi-window layouts, statuslines, tabline, separators,
  wide chars, cursor.
- **Resize storms:** random resize sequences; no panic, no zero-size window, no
  lost text.
- **Mouse:** hit regions after resize/scroll; drag capture; double-click.
- **Differential:** <code>winlayout()</code> and <code>win_screenpos()</code>
  against Vim for the same command sequence.

A layout bug is user-visible and should be caught by geometry tests before a
frame snapshot.
