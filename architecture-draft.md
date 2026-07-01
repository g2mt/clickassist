# ClickAssist — Architecture Draft

## 1. Overview

**ClickAssist** is a Windows utility written in **Rust** using the **Win32
API** (via the official `windows` crate, no heavy frameworks). It lets a user
bind mouse positions to keyboard keys and then, in *start mode*, simulate
touchscreen touch events (press, release, move/gesture) by holding those keys.

Design goals:

- **Minimal dependencies** — use the official **`windows`** crate for Win32
  bindings; nothing else beyond an optional tiny JSON helper for the config.
- **Small footprint** — single executable, lives in the taskbar/notification
  area, low CPU when idle.
- **Responsive input** — key-to-touch latency must be low, so use low-level
  input hooks / raw input rather than polling.

---

## 2. Dependencies

Keep the dependency list as short as possible:

| Purpose | Choice | Notes |
|---|---|---|
| Win32 FFI bindings | **`windows`** | The official Microsoft crate. Enable only the feature modules actually used (`Win32_UI_WindowsAndMessaging`, `Win32_UI_Input_KeyboardAndMouse`, `Win32_UI_Input_Pointer`, `Win32_UI_Shell`, `Win32_UI_Controls`, `Win32_Graphics_Gdi`, `Win32_Foundation`, `Win32_System_LibraryLoader`, `Win32_UI_HiDpi`). |
| Config (de)serialization | `serde` + `serde_json` (optional) | For reading/writing `Documents/clickassist.json`. If we want to avoid even this, hand-roll a tiny JSON writer/reader in `config.rs`. |

Everything else (event loop, tray icon, window, input injection, overlay) is
built on Win32 primitives from the `windows` crate.

### Key Win32 surfaces used

- **Window / message loop**: `CreateWindowExW`, `RegisterClassW`,
  `GetMessageW`, `DispatchMessageW`, `DefWindowProcW`.
- **Toolbar / buttons**: common controls (`BUTTON` class), or a custom-drawn
  toolbar. Icons via `LoadImageW` / `ImageList`.
- **Tray (status) icon**: `Shell_NotifyIconW` (`NIM_ADD`, `NIM_MODIFY`,
  `NIM_DELETE`), with a custom `WM_APP` callback message.
- **Keyboard capture**: `SetWindowsHookExW(WH_KEYBOARD_LL, ...)` low-level
  keyboard hook, or `RegisterRawInputDevices` + `WM_INPUT`.
- **Touch injection**: `InitializeTouchInjection` + `InjectSyntheticPointerInput`
  (`POINTER_TOUCH_INFO`) — the modern touch simulation API.
- **Tooltips**: `TOOLTIPS_CLASS` common control or a custom top-most layered
  window.
- **Overlay (positions)**: transparent, click-through, top-most layered window
  (`WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST`) drawn with GDI
  (`TextOutW`, `Ellipse`).
- **Config path**: `SHGetKnownFolderPath(FOLDERID_Documents, ...)` to locate
  the user's Documents folder.

---

## 3. High-Level Component Diagram

```
+-----------------------------------------------------------+
|                        ClickAssist                        |
|                                                           |
|  +-------------+     +--------------------------------+   |
|  | Tray Icon   |     |  Main Window                   |   |
|  | (Shell_     |<--->|  + Toolbar                     |   |
|  |  NotifyIcon)|     |  [Record][Show Positions][Start]|  |
|  +-------------+     +--------------------------------+   |
|         |                     |                           |
|         v                     v                           |
|  +----------------------------------------------------+   |
|  |              App State / Controller                |   |
|  |   mode: Idle | Recording | Started                 |   |
|  |   bindings: Map<VirtualKey, (x, y)>                 |   |
|  |   active_touches: Map<VirtualKey, TouchPointer>    |   |
|  +----------------------------------------------------+   |
|      |            |               |            |          |
|      v            v               v            v          |
|  +--------+  +-----------+  +-----------+  +-----------+   |
|  | Input  |  | Touch     |  | Overlay   |  | Config    |   |
|  | Hook   |  | Injection |  | (dots +   |  | (JSON in  |   |
|  |        |  | Engine    |  |  labels)  |  | Documents)|   |
|  +--------+  +-----------+  +-----------+  +-----------+   |
+-----------------------------------------------------------+
```

---

## 4. Modules (Rust source layout)

Each module lists an implementation **difficulty** and the key
**functions/structs** expected.

### `src/main.rs` — **[EASY]**
Entry point: DPI awareness, register class, build window, install hook, run the
message loop, clean up on exit.
```rust
fn main() -> windows::core::Result<()>;
fn run_message_loop() -> i32;
```

### `src/app.rs` — **[HARD]**
Central controller: owns `AppState`, dispatches key/tray/toolbar events, drives
mode transitions.
```rust
enum Mode { Idle, Recording, Started }

struct AppState {
    mode: Mode,
    bindings: HashMap<u32 /*VK*/, POINT>,
    active: HashMap<u32 /*VK*/, ActiveTouch>,
    next_pointer_id: u32,
    ctrl_down: bool,
    gesture_anchor: Option<u32>,
    overlay_visible: bool,
}

struct ActiveTouch { pointer_id: u32, current_pos: POINT }

impl AppState {
    fn on_toolbar_command(&mut self, id: u16);
    fn on_key_event(&mut self, vk: u32, down: bool) -> bool; // returns "swallow?"
    fn enter_recording(&mut self);
    fn enter_started(&mut self);
    fn stop(&mut self);
}
```

### `src/window.rs` — **[EASY]**
Main window creation and `WndProc` message routing.
```rust
fn create_main_window(hinstance: HINSTANCE) -> HWND;
unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT;
```

### `src/toolbar.rs` — **[EASY]**
Toolbar with three buttons (Record / Show Positions / Start), icon + text.
```rust
const ID_RECORD: u16;
const ID_SHOW_POSITIONS: u16;
const ID_START: u16;

fn create_toolbar(parent: HWND, hinstance: HINSTANCE) -> HWND;
fn set_button_icons(toolbar: HWND); // TB_SETIMAGELIST / BS_ICON
```

### `src/tray.rs` — **[EASY]**
Notification-area icon, callback message, context menu.
```rust
const WM_TRAY: u32 = WM_APP + 1;

fn add_tray_icon(hwnd: HWND) -> NOTIFYICONDATAW;
fn remove_tray_icon(data: &NOTIFYICONDATAW);
fn show_tray_menu(hwnd: HWND);
```

### `src/tooltip.rs` — **[EASY]**
Record-mode instruction tooltip.
```rust
fn show_instruction_tooltip(anchor: HWND, text: &str);
fn hide_tooltip();
```

### `src/hook.rs` — **[HARD]**
Low-level keyboard hook install/uninstall + callback; auto-repeat filtering;
modifier tracking; marshalling to the UI thread.
```rust
fn install_keyboard_hook() -> HHOOK;
fn uninstall_keyboard_hook(hook: HHOOK);
unsafe extern "system" fn keyboard_proc(code: i32, w: WPARAM, l: LPARAM) -> LRESULT;
```

### `src/touch.rs` — **[HARD]**
Touch injection engine: press, release, multi-touch, interpolated gesture move.
```rust
fn init_touch_injection(max_contacts: u32);
fn make_touch_info(pointer_id: u32, pos: POINT, flags: POINTER_FLAGS) -> POINTER_TOUCH_INFO;
fn touch_down(pointer_id: u32, pos: POINT);
fn touch_up(pointer_id: u32, pos: POINT);
fn touch_move(pointer_id: u32, from: POINT, to: POINT, steps: u32);
```

### `src/overlay.rs` — **[HARD]**
Transparent click-through top-most window that renders each bound key as a dot
with a centered label above it.
```rust
fn create_overlay_window(hinstance: HINSTANCE) -> HWND;
fn show_overlay(overlay: HWND, bindings: &HashMap<u32, POINT>);
fn hide_overlay(overlay: HWND);
unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT;
fn paint_bindings(hdc: HDC, bindings: &HashMap<u32, POINT>); // Ellipse + centered TextOutW
```

### `src/bindings.rs` — **[EASY]**
Key -> position map operations and virtual-key <-> label helpers.
```rust
struct Binding { vk: u32, x: i32, y: i32 }
fn vk_to_label(vk: u32) -> String;
fn upsert(bindings: &mut HashMap<u32, POINT>, vk: u32, pos: POINT);
```

### `src/config.rs` — **[EASY]**
Persist/load bindings to `Documents/clickassist.json`.
```rust
struct Config { bindings: Vec<Binding> }
fn config_path() -> PathBuf;                 // SHGetKnownFolderPath(FOLDERID_Documents)
fn load() -> Config;
fn save(cfg: &Config) -> std::io::Result<()>;
```

### `src/win.rs` — **[EASY]**
Thin Win32 helpers (wide strings, error checks, DPI).
```rust
fn wide(s: &str) -> Vec<u16>;
fn set_dpi_awareness();
```

---

## 5. Application State Machine

```
        +---------+   Record btn    +-----------+
        |  Idle   | --------------> | Recording |
        |         | <-------------- |           |
        +---------+  key bound /    +-----------+
             |        Esc/cancel
             | Start btn
             v
        +-----------+
        |  Started  |
        |  (window  |
        |   hidden) |
        +-----------+
             |
             | tray click / stop
             v
           Idle
```

The **Show Positions** button toggles an overlay in any mode; it does not
change `Mode`, only `overlay_visible`.

### `Mode` enum

```rust
enum Mode {
    Idle,       // main window visible, nothing intercepted
    Recording,  // waiting for a key press to bind current cursor pos
    Started,    // window hidden, keys injected as touch events
}
```

### Shared state

```rust
struct AppState {
    mode: Mode,
    bindings: HashMap<u32 /*VK*/, POINT /*screen x,y*/>,
    active: HashMap<u32 /*VK*/, ActiveTouch>,
    next_pointer_id: u32,
    ctrl_down: bool,
    gesture_anchor: Option<u32>, // the key currently "held" for a gesture
    overlay_visible: bool,
}

struct ActiveTouch {
    pointer_id: u32,
    current_pos: POINT,
}
```

`AppState` is owned by the message loop thread. The low-level keyboard hook
callback runs on the same thread that installed it (the hook fires in the
context of the thread's message loop), so state access can be single-threaded.
If a hook runs in another context, marshal events to the UI thread via
`PostMessageW(WM_APP_KEY_EVENT, ...)`.

---

## 6. Main Window & Toolbar

- Created on startup, shown immediately.
- Contains a **single toolbar** with three buttons:
  - **Record** — enters `Recording` mode; shows a tooltip:
    *"Move cursor and press any key to bind the mouse position to that key."*
  - **Show Positions** — toggles the overlay that draws every bound key as a
    dot with its label centered horizontally above it (see §8).
  - **Start** — enters `Started` mode; **hides** the main window
    (`ShowWindow(hwnd, SW_HIDE)`).
- Buttons show an **icon + text** if icons are available
  (`TB_SETIMAGELIST` / `BS_ICON`), otherwise text-only fallback.
- Closing the window (`WM_CLOSE`) hides it to tray instead of exiting
  (real exit only from the tray menu).

---

## 7. Tray (Status) Icon

- Added via `Shell_NotifyIconW(NIM_ADD)` at startup with a `uCallbackMessage`
  (e.g. `WM_APP + 1`).
- Left-click on the icon **restores/shows** the main window
  (`ShowWindow(SW_SHOW)` + `SetForegroundWindow`).
- Right-click shows a small context menu (`TrackPopupMenu`): *Show*, *Stop*,
  *Exit*.
- Removed via `NIM_DELETE` on exit.

---

## 8. Position Overlay ("Show Positions")

Purpose: let the user visually verify where each bound key maps on screen.

- A dedicated **transparent, click-through, top-most** window covers the full
  virtual desktop:
  `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW`.
- For each binding `(vk, POINT{x, y})`:
  - Draw a small filled **dot** (e.g. `Ellipse`) centered at `(x, y)`.
  - Draw the key **label** (from `vk_to_label`) **above** the dot, **centered
    horizontally** on `x`:
    - Measure text with `GetTextExtentPoint32W`.
    - `text_x = x - text_width / 2`; `text_y = y - dot_radius - text_height - pad`.
    - Render with `SetTextAlign(hdc, TA_LEFT)` + `TextOutW`, or
      `TA_CENTER` and pass `x` directly.
- Toggled by the **Show Positions** button (`overlay_visible = !overlay_visible`)
  and repainted (`InvalidateRect`) whenever bindings change.
- Because the window is `WS_EX_TRANSPARENT`, it never steals input; recording
  and injection continue to work while it is shown.

```rust
fn paint_bindings(hdc: HDC, bindings: &HashMap<u32, POINT>) {
    for (vk, p) in bindings {
        draw_dot(hdc, p.x, p.y, DOT_RADIUS);
        let label = vk_to_label(*vk);
        draw_centered_label(hdc, p.x, p.y - DOT_RADIUS - LABEL_GAP, &label);
    }
}
```

---

## 9. Record Mode

1. User clicks **Record** → `mode = Recording`.
2. Show tooltip near the toolbar/cursor with the instruction text.
3. Low-level keyboard hook is active; on the next **key down**:
   - Capture cursor position with `GetCursorPos` → `POINT`.
   - Store `bindings.insert(vk, pos)`.
   - Persist to `Documents/clickassist.json` (see §13).
   - If the overlay is visible, `InvalidateRect` to redraw the new dot/label.
   - (Optional) allow binding multiple keys in a row until the user clicks
     Record again / presses `Esc` to finish.
4. Return to `Idle` (or stay in Recording for multi-bind), hide tooltip.

The bound key event is **swallowed** (hook returns non-zero) while recording so
it does not leak to other apps.

---

## 10. Start Mode & Touch Injection

### Initialization

- Call `InitializeTouchInjection(max_count, TOUCH_FEEDBACK_DEFAULT)` once,
  where `max_count` is >= number of simultaneously supported touches.

### Per-key behavior (simple press/release)

- **Key down** (bound key, not already active):
  - Allocate a `pointer_id`.
  - Build `POINTER_TOUCH_INFO` with `POINTER_FLAG_DOWN | INRANGE | INCONTACT`
    at the bound position.
  - `InjectSyntheticPointerInput(...)`.
  - Track in `active`.
  - Swallow the physical key so the target app only sees touch.
- **Key up** (bound key, active):
  - Send the pointer with `POINTER_FLAG_UP`.
  - Remove from `active`, free `pointer_id`.

Multiple keys held simultaneously = multiple concurrent touch pointers
(multi-touch), each with its own `pointer_id`.

### Gestures (finger drag)

Trigger: **Ctrl held** changes interpretation of subsequent key presses into a
"move the existing touch" gesture.

Sequence:

1. User holds **Ctrl**.
2. User presses **Key 1** (first key after Ctrl is held): a touch press is
   injected at `bindings[Key1]`; this pointer becomes the *gesture anchor*.
3. User presses **Key 2**: the anchor pointer is **moved** from
   `bindings[Key1]` to `bindings[Key2]`:
   - Inject a sequence of `POINTER_FLAG_UPDATE | INCONTACT` events
     interpolating positions from pos1 → pos2 (a few intermediate steps for a
     smooth swipe).
   - The pointer now logically rests at Key2's position; further Key-N presses
     continue chaining the drag (Key2 → Key3, ...).
4. Releasing Ctrl (or the anchor key) finalizes: inject `POINTER_FLAG_UP` and
   clear the gesture anchor.

```rust
enum KeyIntent {
    TouchTap(u32),        // vk -> down/up per physical key
    GestureAnchor(u32),   // first key while Ctrl held
    GestureMove(u32),     // subsequent keys: drag anchor to this key's pos
}
```

Interpolation helper produces N steps between two `POINT`s and injects one
`UPDATE` per step (optionally with a small `Sleep`/timer to look natural).

---

## 11. Keyboard Hook Strategy

- Use `SetWindowsHookExW(WH_KEYBOARD_LL, callback, hmod, 0)`.
- In the callback, inspect `WM_KEYDOWN / WM_KEYUP` and the virtual key code.
- Decide based on `mode`:
  - `Idle`: pass through (`CallNextHookEx`).
  - `Recording`: capture + bind, swallow.
  - `Started`: if key is bound, translate to touch action + swallow;
    otherwise pass through.
- Track modifier state (`Ctrl`) here or via `GetAsyncKeyState`.
- Guard against auto-repeat: only act on the *first* down transition
  (track pressed set), ignore repeated `WM_KEYDOWN`.

Because `WH_KEYBOARD_LL` callbacks must return quickly, heavy work (injection
sequences for gestures) can be posted to the UI thread via `PostMessageW`
to avoid blocking the hook chain.

---

## 12. Threading Model

- **Single UI thread** owns the window, message loop, tray, and `AppState`.
- The low-level keyboard hook must be installed from a thread with a message
  loop — use the UI thread.
- Gesture interpolation that needs timed steps uses a **Win32 timer**
  (`SetTimer` / `WM_TIMER`) rather than a background thread, keeping state
  single-threaded and lock-free.

---

## 13. Data / Persistence

- Config file path: **`Documents/clickassist.json`**, resolved with
  `SHGetKnownFolderPath(FOLDERID_Documents, ...)`.
- Format (JSON):
  ```json
  {
    "bindings": [
      { "vk": 65, "x": 1200, "y": 640 },
      { "vk": 66, "x": 300,  "y": 200 }
    ]
  }
  ```
- Loaded once at startup; saved whenever bindings change (after a bind in
  Record mode, or on exit).
- Serialization via `serde_json` (optional). To stay fully dependency-free, a
  hand-rolled writer/reader in `config.rs` can emit/parse this small schema.

---

## 14. Lifecycle Summary

1. **Startup**: set DPI awareness, register window class, load config from
   `Documents/clickassist.json`, create main window (visible), create toolbar +
   buttons, create overlay window (hidden), add tray icon,
   `InitializeTouchInjection`, install keyboard hook, enter message loop.
2. **Record**: bind keys to cursor positions; persist to config.
3. **Show Positions**: toggle overlay of dots + centered labels.
4. **Start**: hide window; keys drive touch injection (taps, multi-touch,
   gestures).
5. **Restore**: tray click shows window again (mode → Idle or paused).
6. **Exit**: save config, `NIM_DELETE`, `UnhookWindowsHookEx`, destroy windows,
   `PostQuitMessage`.

---

## 15. Open Questions / Risks

- **Touch injection availability**: `InjectSyntheticPointerInput` requires
  Windows 8+; verify target OS. Legacy fallback would be `SendInput` mouse
  events (no true multi-touch).
- **Elevation**: injecting input into elevated apps requires ClickAssist to run
  elevated (UIPI). Manifest with appropriate `requestedExecutionLevel`.
- **Coordinate space**: touch injection and the overlay use screen pixels;
  handle multi-monitor (virtual desktop bounds) and DPI
  (`SetProcessDpiAwarenessContext`).
- **Gesture timing**: number of interpolation steps and delay per step needs
  tuning for the target application's gesture recognizer.
- **Key swallowing**: ensure bound keys don't leak to the foreground app in
  Started mode while non-bound keys pass through normally.
- **Overlay redraw**: keep the click-through overlay in sync with binding
  changes and monitor topology changes (`WM_DISPLAYCHANGE`).
