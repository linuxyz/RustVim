# UI protocol and backend contract

**Status:** design baseline. This is the contract between the editor core and
any backend (TUI now, GUI later, headless for tests).

The protocol is intentionally small: the core sends complete frames of cells;
the backend sends input events. The backend has no knowledge of Vim commands,
buffers, options, statusline syntax or highlight groups.

---

## 1. Contract rules

1. **The core owns semantics.** It decides every visible cell, cursor position,
   highlight, click region and frame timing. The backend only rasterizes.
2. **The backend owns the physical device.** It reads bytes/events, negotiates
   capabilities and writes glyphs/colors. It does not interpret Vim keys beyond
   decoding terminal sequences into logical keys.
3. **Frames are immutable snapshots.** A backend must not mutate a
   <code>Frame</code>; it may copy what it needs. The core may reuse buffers
   after <code>present</code> returns.
4. **Coordinates are cells, zero-based.** A frame is exactly
   <code>size.rows x size.cols</code>. The base layer covers the whole frame.
5. **Text is a grapheme cluster per cell.** The core resolves width, combining
   marks and continuation cells. The backend must not reflow or reshape cell
   boundaries.
6. **No Vim semantics in the protocol.** No <code>Normal</code> highlight group,
   no <code>%f</code> statusline item, no <code>:set</code> option. Only cells,
   attributes, cursor and events.
7. **Backends are replaceable.** TUI, headless and GUI consume the same frame
   type; a backend swap must not change editor behavior or tests.
8. **Protocol changes are versioned.** Additive fields are allowed; semantic
   changes bump <code>UI_PROTOCOL_VERSION</code> and update all backends.

---

## 2. Geometry and cells

~~~rust
pub const UI_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size { pub rows: u16, pub cols: u16 }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect { pub row: u16, pub col: u16, pub rows: u16, pub cols: u16 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AttrId(pub u32);

#[derive(Clone, Debug)]
pub struct Cell {
    /// One grapheme cluster. Empty for a wide-char continuation cell.
    pub text: String,
    /// 1 or 2 for a leading cell; 0 for continuation.
    pub width: u8,
    pub attrs: AttrId,
}

impl Cell {
    pub const BLANK: Cell = Cell { text: String::new(), width: 1, attrs: AttrId(0) };
    pub const WIDE_CONT: Cell = Cell { text: String::new(), width: 0, attrs: AttrId(0) };
}
~~~

A double-width character occupies two cells: the leading cell has
<code>width = 2</code> and the cluster text; the following cell has
<code>width = 0</code> and empty text. A backend that cannot render double-width
characters must still consume both cells, not shift the rest of the line. The
core never emits a zero-width cell without a preceding wide cell in the same
row.

A grid is row-major:

~~~rust
pub struct Grid {
    pub size: Size,
    pub cells: Vec<Cell>,          // len == rows * cols
    pub attrs: Vec<Attr>,          // frame-local palette
}

impl Grid {
    pub fn idx(&self, row: u16, col: u16) -> usize {
        row as usize * self.size.cols as usize + col as usize
    }
    pub fn cell(&self, row: u16, col: u16) -> &Cell { ... }
}
~~~

The attribute palette is frame-local and deduplicated by the core. Attribute
IDs are valid only for the frame that contains them.

---

## 3. Attributes and color

~~~rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Default,
    Indexed(u8),        // terminal palette 0-255
    Rgb(u8, u8, u8),
}

bitflags::bitflags! {
    pub struct AttrFlags: u32 {
        const BOLD          = 1 << 0;
        const ITALIC        = 1 << 1;
        const UNDERLINE     = 1 << 2;
        const UNDERCURL     = 1 << 3;
        const UNDERDOUBLE   = 1 << 4;
        const UNDERDOTTED   = 1 << 5;
        const UNDERDASHED   = 1 << 6;
        const STRIKETHROUGH = 1 << 7;
        const REVERSE       = 1 << 8;
        const BLINK         = 1 << 9;
        const CONCEAL       = 1 << 10;
        const DIM           = 1 << 11;
        const NOCOMBINE     = 1 << 12;
    }
}

pub struct Attr {
    pub fg: Color,
    pub bg: Color,
    pub sp: Color,          // underline/curl color
    pub flags: AttrFlags,
}
~~~

The core resolves Vim highlight groups to <code>Attr</code> values according to
backend capabilities:

- <code>Mono</code>: attributes only, no color.
- <code>Ansi16</code>: map to the 16-color palette, preserving bold/reverse.
- <code>Ansi256</code>: nearest palette index for RGB sources.
- <code>TrueColor</code>: RGB values pass through; <code>'termguicolors'</code>
  controls whether Vim semantics choose GUI colors or cterm colors.

Color mapping is deterministic and cached by <code>(highlight group, capability,
background, termguicolors)</code>. The backend receives final colors; it does not
know <code>ctermfg</code> versus <code>guifg</code>.

---

## 4. Frames and layers

~~~rust
pub struct Frame {
    pub seq: u64,
    pub size: Size,
    pub layers: Vec<Layer>,
    pub cursor: Option<Cursor>,
    pub mouse_shape: MouseShape,
    pub title: Option<String>,
    pub bell: bool,
    pub clipboard: Option<ClipboardRequest>,
    pub mode: UiMode,             // informational: normal/insert/visual/...
}

pub struct Layer {
    pub id: GridId,
    pub rect: Rect,               // position in the frame
    pub z: i16,                   // lower is behind; base is 0
    pub kind: LayerKind,
    pub grid: Grid,
}

pub enum LayerKind {
    Base,                         // exactly one, covers the frame
    Popup,                        // popup menu, hover, completion
    Overlay,                      // future GUI effects, not required by TUI
}
~~~

The core always emits exactly one base layer. Popups and the completion menu
may be separate layers so a GUI can animate or decorate them; the TUI flattens
layers in z-order. A backend may ignore layer metadata and composite naively,
but must preserve z-order and clipping.

Frame sequence numbers increase monotonically. A backend may drop intermediate
frames if it cannot keep up, but must present the newest frame and never a
frame older than one already presented on the same connection. The core does
not wait for acknowledgements.

---

## 5. Cursor, mouse and metadata

~~~rust
pub struct Cursor {
    pub grid: GridId,
    pub row: u16,
    pub col: u16,
    pub shape: CursorShape,
    pub blink: bool,
    pub color: Color,             // Default or explicit
}

pub enum CursorShape {
    Block,
    Vertical { thickness: u8 },
    Horizontal { thickness: u8 },
    Underline { thickness: u8 },
}

pub enum MouseShape {
    Default,
    Arrow,
    IBeam,
    Crosshair,
    Hand,
    ResizeHorizontal,
    ResizeVertical,
    ResizeUpLeftDownRight,
    ResizeUpRightDownLeft,
    NotAllowed,
}
~~~

The cursor is separate from cells. The backend must not draw the cursor into the
cell grid; it draws it at the requested cell and restores the cell when the
cursor moves. <code>blink = false</code> means "steady"; the backend may blink
when the protocol does not require steady (the core sets this from Vim mode and
options).

<code>UiMode</code> is informational for GUI cursor/menu policy and for tests.
The backend must not derive editor behavior from it.

---

## 6. Backend trait

~~~rust
pub trait UiBackend {
    fn capabilities(&self) -> UiCapabilities;

    fn init(&mut self) -> Result<(), UiError>;
    fn present(&mut self, frame: &Frame) -> Result<(), UiError>;
    fn poll_event(&mut self, timeout: Duration) -> Result<Option<UiEvent>, UiError>;
    fn resize(&mut self, size: Size) -> Result<(), UiError>;
    fn set_title(&mut self, title: &str) -> Result<(), UiError>;
    fn clipboard(&mut self, request: ClipboardRequest) -> Result<(), UiError>;
    fn suspend(&mut self) -> Result<(), UiError>;
    fn resume(&mut self) -> Result<(), UiError>;
    fn shutdown(&mut self) -> Result<(), UiError>;
}
~~~

Rules:

- <code>present</code> must not block on the user or call back into the editor.
- <code>poll_event</code> waits up to <code>timeout</code>; <code>None</code>
  means timeout, not EOF. EOF is <code>UiEvent::Quit(QuitReason::Eof)</code>.
- <code>resize</code> is called by the core after a
  <code>UiEvent::Resize</code>, so a backend can update its internal size before
  the next frame.
- <code>suspend</code>/<code>resume</code> restore the terminal around shell
  commands and job control. They must be idempotent.
- <code>shutdown</code> is called on normal exit and on fatal error; it must
  restore the terminal even if <code>init</code> partially failed.
- All methods return <code>UiError</code>; a backend error is fatal to the UI
  session, not a Vim error. The editor reports it and exits non-zero.

The editor side wraps a backend:

~~~rust
pub struct UiBridge<B: UiBackend> {
    backend: B,
    last_size: Size,
    pending_clipboard: HashMap<RequestId, ClipboardKind>,
}

impl<B: UiBackend> UiBridge<B> {
    pub fn pump(&mut self, timeout: Duration) -> Result<Vec<UiEvent>, UiError>;
    pub fn present(&mut self, frame: &Frame) -> Result<(), UiError>;
    pub fn resize(&mut self, size: Size) -> Result<(), UiError>;
}
~~~

The bridge is the only place that knows both editor and backend. It lives in
<code>rvim-cli</code>, not in <code>rvim-editor</code>.

---

## 7. Events

~~~rust
pub enum UiEvent {
    Key(KeyEvent),
    Text(String),                 // committed text, e.g. IME
    Paste(String),                // bracketed paste payload
    Mouse(MouseEvent),
    Resize(Size),
    Focus(bool),
    TermResponse(TermResponse),
    Clipboard(ClipboardResult),
    Ime(ImeEvent),                // GUI preedit/commit
    Wake,                         // internal wakeup, no user action
    Quit(QuitReason),
}

pub struct KeyEvent {
    pub key: Key,
    pub mods: Mods,
    pub kind: KeyKind,
    pub text: Option<Box<str>>,   // printable text when key is Char
}

pub enum KeyKind { Press, Repeat, Release }

bitflags::bitflags! {
    pub struct Mods: u8 {
        const SHIFT = 1 << 0;
        const CTRL  = 1 << 1;
        const ALT   = 1 << 2;
        const SUPER = 1 << 3;
        const HYPER = 1 << 4;
        const META  = 1 << 5;
    }
}

pub enum Key {
    Char(char),
    F(u8),
    Enter, Esc, Tab, Backspace, Insert, Delete,
    Home, End, PageUp, PageDown,
    Up, Down, Left, Right,
    Mouse,                        // only if the backend cannot decode
}
~~~

<code>Key::Char</code> plus modifiers covers <code>&lt;C-a&gt;</code>,
<code>&lt;M-x&gt;</code>, <code>&lt;S-Tab&gt;</code> and similar. The core maps
these to Vim key notation and mapping semantics. <code>KeyKind::Release</code>
is ignored by default; the core may use it for kitty-protocol disambiguation.
<code>Text</code> is for committed characters that are not a single key event
(IME, compose); the core inserts them as text, not as mappings.

### 7.1 Mouse

~~~rust
pub struct MouseEvent {
    pub row: u16,
    pub col: u16,
    pub kind: MouseKind,          // Press, Release, Drag, Move, Wheel
    pub button: MouseButton,      // Left, Middle, Right, WheelUp, WheelDown, None
    pub mods: Mods,
}

pub enum MouseKind { Press, Release, Drag, Move, Wheel }
pub enum MouseButton { Left, Middle, Right, WheelUp, WheelDown, None }
~~~

The backend decodes the terminal mouse protocol (SGR, urxvt, X10) into these
semantic events. It does not know about windows, tabs or folds. The core hit-
tests its last frame's regions and applies <code>'mouse'</code>,
<code>'mousemodel'</code>, <code>'mousetime'</code> and multi-click rules.

### 7.2 Terminal responses

<code>TermResponse</code> carries device attributes, cursor-position reports,
background-color reports and keyboard-protocol acknowledgements. These are not
keys; the core may consume them for <code>v:termresponse</code>,
<code>'background'</code> detection or capability updates.

### 7.3 Clipboard

~~~rust
pub enum ClipboardRequest {
    Get { kind: ClipboardKind, request: RequestId },
    Set { kind: ClipboardKind, data: String, request: RequestId },
}

pub enum ClipboardKind { Unnamed, Selection, Plus }

pub struct ClipboardResult {
    pub request: RequestId,
    pub data: Option<String>,
    pub error: Option<String>,
}
~~~

The core owns <code>'clipboard'</code>, register semantics and text selection.
The backend only moves bytes to/from the OS. Reads may be synchronous at the
provider level; the protocol allows a result event for backends that need async
IPC. A clipboard failure becomes a Vim message, not a UI crash.

---

## 8. Capabilities

~~~rust
pub struct UiCapabilities {
    pub color: ColorDepth,
    pub unicode: bool,
    pub mouse: MouseCaps,
    pub bracket_paste: bool,
    pub focus_events: bool,
    pub kitty_keyboard: bool,
    pub modify_other_keys: bool,
    pub title: bool,
    pub clipboard: ClipboardCaps,
    pub ime: bool,
    pub pixels: Option<PixelMetrics>,   // GUI only
    pub images: bool,                   // reserved; false until MISC-12
    pub sync_output: bool,              // DEC 2026 synchronized output
    pub in_band_resize: bool,           // resize reports inside the byte stream
}

pub enum ColorDepth { Mono, Ansi16, Ansi256, TrueColor }

pub struct PixelMetrics {
    pub cell_width: f32,
    pub cell_height: f32,
    pub dpi: f32,
}

pub struct MouseCaps {
    pub click: bool,
    pub drag: bool,
    pub move: bool,
    pub wheel: bool,
    pub sgr: bool,
    pub urxvt: bool,
}
~~~

Negotiation happens once at <code>init</code>, then on terminal response changes
(for example, <code>t_Co</code> or <code>v:termresponse</code>). The core
recomputes highlight attributes and re-renders when capabilities change.
<code>sync_output</code> means the backend can bracket a frame with the
terminal's synchronized-update sequence (<code>'termsync'</code>);
<code>in_band_resize</code> means resize reports arrive in the input stream
(<code>'termresize'</code>) rather than only through <code>SIGWINCH</code>. A
capability downgrade must never lose text; it only changes color/decoration.

---

## 9. Input decoding responsibilities

| Layer | Responsibility |
|---|---|
| TTY backend | Read bytes; decode terminfo/function keys, mouse protocols, bracketed paste, focus, kitty/modifyOtherKeys; handle ESC ambiguity; emit <code>UiEvent</code>. |
| Core input layer | Convert <code>KeyEvent</code> to Vim key notation; apply <code>'langmap'</code>, mappings, <code>'timeout'</code>/<code>'ttimeout'</code>, counts and mode semantics. |
| Mode machines | Interpret keys as motions/operators/text/commands. |

### 9.1 ESC and Alt ambiguity

Terminals encode <code>Alt-x</code> as <code>ESC x</code>, which is also
<code>Esc</code> followed by <code>x</code>. The backend resolves this with a
short delay (default 25 ms, configurable through <code>UiConfig</code> and
aligned with <code>'ttimeoutlen'</code>):

- If no byte follows <code>ESC</code> within the delay, emit
  <code>Key::Esc</code>.
- If the following bytes form a known escape sequence, decode it as a
  function/arrow/mouse key.
- If a printable byte follows within the delay and no sequence matches, emit
  <code>Key::Char(c)</code> with <code>ALT</code>.
- If the user typed <code>Esc</code> then a key quickly, the core mapping engine
  can still disambiguate with <code>'timeoutlen'</code>; the backend delay only
  resolves terminal-level ambiguity.

The exact delay is a TUI concern; the GUI has explicit Alt events. The core's
mapping timeouts remain authoritative for Vim behavior.

### 9.2 Paste

Bracketed paste is delivered as one <code>Paste(String)</code>. The core:
- bypasses mappings and abbreviations,
- inserts text literally (normalizing line endings as Vim does),
- respects <code>'paste'</code> and undo blocking,
- fires the relevant TextChanged events once, not per character.

If the terminal lacks bracketed paste, the TUI falls back to a heuristic only
when the user explicitly enables it; no silent guessing.

### 9.3 Unknown sequences

Unknown CSI/OSC sequences are logged at debug level and dropped. They must not
be inserted as text. A malformed UTF-8 byte is surfaced as a replacement
character/byte sequence according to <code>'encoding'</code> policy; the
backend does not panic.

---

## 10. Render loops

### 10.1 TUI

~~~text
loop {
    deadline = editor.next_deadline();
    if let Some(event) = backend.poll_event(deadline)? {
        editor.handle_event(event);
        while let Some(event) = backend.poll_event(Duration::ZERO)? {
            editor.handle_event(event);
        }
    }
    editor.on_tick(now);                 // timers, job events, redraw
    if editor.needs_redraw() {
        let frame = editor.render();
        backend.present(&frame)?;
    }
}
~~~

The TUI's wait watches stdin and a wakeup pipe used by the job reactor. The
editor's deadline is the minimum of the next timer, redraw deadline and input
timeout. A wakeup event is internal and produces no Vim key.

When <code>'termsync'</code> is set and <code>sync_output</code> is advertised,
the TUI brackets <code>present</code> with the terminal's begin/end
synchronized-update sequences. With <code>'termresize'</code> set to
<code>inband</code>, resize reports are decoded from the input stream; either
path produces the same <code>UiEvent::Resize</code>.

<code>redraw_listener_add()</code> callbacks wrap this pipeline:
<code>on_start</code> runs before layout and <code>on_end</code> after
<code>present</code>. They run on the main thread, are not backend hooks, and
cannot add a listener or force a nested redraw from inside a callback.

### 10.2 GUI (future)

A GUI event loop forwards key/mouse/IME/resize events and calls the same
<code>handle_event</code>/<code>on_tick</code>/<code>render</code> API. It may
render frames on the main thread or hand them to a render thread, but editor
state stays on the main thread. Pixel-level effects are metadata; they never
change cell contents.

### 10.3 Headless

The headless backend renders on demand and records frames. It can inject
<code>UiEvent</code>s directly; there is no real-time waiting. This is the
primary CI backend.

---

## 11. Statusline, tabline, tabpanel and cmdline widgets

These widgets live in <code>rvim-view</code> and emit cell segments. They are
not special-cased by backends.

~~~rust
pub struct Segment {
    pub text: String,
    pub attrs: Attr,
    pub click: Option<ActionId>,
}

pub trait StatuslineHost {
    fn eval_statusline(&mut self, win: WindowId) -> Result<Vec<Segment>, VimError>;
    fn window_width(&self, win: WindowId) -> u16;
    fn mode_text(&self) -> &str;
    fn ruler_text(&self, win: WindowId) -> String;
}

pub trait Widget {
    fn layout(&mut self, area: Rect, host: &mut dyn StatuslineHost);
    fn height(&self) -> u16;             // statusline may use several rows
    fn segments(&self) -> &[Segment];
    fn hit_regions(&self) -> &[Region];
}
~~~

- The statusline widget evaluates <code>'statusline'</code> (with
  <code>g:statusline_winid</code>) in a separate evaluation phase, then lays
  out segments. Truncation and <code>%=</code>/<code>%&lt;</code> are computed
  in cells, not bytes. With <code>'statuslineopt'</code> and <code>%@</code> it
  reports 1..N rows; the compositor reserves that height before placing the
  window below it.
- <code>%[FuncName]</code> regions (closed by <code>%[]</code>) become click
  actions; the callback receives <code>minwid</code>, click count, button,
  modifiers, window ID and area. The widget records the action, the core invokes
  VimL after hit-testing.
- The tabline widget does the same for <code>'tabline'</code>, with
  <code>%1T</code> click regions. The tabpanel is the vertical sidebar variant:
  it renders <code>'tabpanel'</code> into a column strip, owns its scroll
  offset and emits tab-selection, scrollbar and <code>%[Func]</code> regions.
- The cmdline widget renders text and the completion popup; the core owns the
  editing/history/completion state machine.

Because these widgets only use <code>Segment</code>/<code>GridWriter</code>,
they compile and unit-test without a terminal. The TUI backend sees the same
cells as any other frame. Statusline height and tabpanel width are layout
inputs; neither widget knows the backend's row size.

---

## 12. Hit regions and actions

The core records regions while rendering:

~~~rust
pub struct Region {
    pub rect: Rect,
    pub action: ActionId,
    pub cursor: MouseShape,
}

pub enum ActionId {
    FocusWindow(WindowId),
    ResizeWindow { win: WindowId, dir: SplitDir },
    SelectTab(TabId),
    CloseTab(TabId),
    StatuslineItem { win: WindowId, item: u16 },
    StatuslineCallback { callback: CallbackId, minwid: u16, area: ClickArea },
    TabpanelScroll { delta: i16 },
    Sign { buf: BufferId, line: u32 },
    Fold { win: WindowId, line: u32 },
    PopupItem { popup: PopupId, index: u32 },
    CmdlineCompletion { index: u32 },
}

pub enum ClickArea { Statusline, Tabline, Tabpanel }
pub struct CallbackId(pub u32); // registry handle; never a raw Funcref
~~~

The backend sends mouse coordinates only. The core hit-tests the last frame's
regions (adjusted for resize) and dispatches the semantic action. This is what
lets statusline/tabline/tabpanel clicks and callback dispatch work identically
in TUI and GUI without either backend knowing what a tab or window is.

---

## 13. Text, width and the cursor

- The core converts buffer text to display cells using:
  tab expansion, <code>'listchars'</code>, conceal, folds, wrapping,
  <code>setcellwidths()</code>, combining marks, emoji/wide width, virtual text.
- A grapheme cluster is emitted in one cell with its width; combining marks are
  attached, never emitted as standalone cells.
- The cursor is placed on the leading cell of a wide character. A backend must
  render the cursor across the full width when the shape is a block.
- GUI font shaping may produce different pixel advances, but the cell grid is
  authoritative for cursor/selection/hit regions. A GUI that cannot fit a glyph
  in its cell must clip/scale, not change the grid.
- <code>screenchar()</code>/<code>screenstring()</code> read back the frame's
  cell text, not terminal state.

---

## 14. GUI extension contract (design only)

A future GUI backend may implement additional traits:

~~~rust
pub trait GuiExt {
    fn set_window_geometry(&mut self, windows: &[WindowGeometry]);
    fn set_ime_cursor(&mut self, pos: PixelPos);
    fn show_ime_preedit(&mut self, text: &str, cursor: usize);
    fn set_scrollbars(&mut self, bars: &[ScrollbarState]);
    fn show_context_menu(&mut self, menu: MenuModel);
    fn set_tab_drag(&mut self, enabled: bool);
}
~~~

- Fonts/ligatures: the GUI receives cell clusters and attributes; it may shape
  text but must keep cell mapping.
- Menus: the core evaluates <code>:menu</code> definitions into a
  <code>MenuModel</code>; the GUI decides native vs custom rendering.
- Scrollbars: the core provides viewport metadata; the GUI may draw native
  scrollbars. Mouse actions map back to scroll commands.
- IME: preedit is a GUI overlay; committed text arrives as
  <code>UiEvent::Text</code>.
- Window decorations: the core owns separators/statusline cells; a GUI may add
  native borders outside the cell rect without changing geometry.

None of this is implemented in the current phase. <code>has('gui')</code> is
false, <code>has('gui_running')</code> is false, and no GUI crate is linked.

---

## 15. Protocol testing

- **Golden frames:** the headless backend serializes <code>Frame</code> to JSON
  (size, layers, cells, attrs, cursor, regions). Snapshots are reviewed like
  source.
- **Backend conformance:** a fake backend replays event sequences and asserts
  the frames the editor produces; a TUI-specific test asserts ANSI bytes for a
  small frame.
- **Resize:** every frame is checked for out-of-bounds/continuation-cell
  consistency.
- **Color:** the same logical frame is rendered at Mono/16/256/TrueColor and
  checked for attribute invariants.
- **Fuzz:** random cell sizes, wide chars, combining marks and overlay rects
  must not panic.

The headless backend is the reference implementation of the protocol. If a TUI
or GUI disagrees with it, the backend is wrong unless the protocol says
otherwise.
