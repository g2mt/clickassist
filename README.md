# ClickAssist

> [!WARNING]
> Contains mostly AI generated code. This README is also generated, however I have checked the README for accuracy.

Bind mouse positions to keyboard keys and inject them as synthetic touchscreen events on Windows.

Assign keyboard shortcuts to screen positions. Pressing a bound key injects a touch contact at the saved location.

## How it works

- Press any key while in record mode to bind it to the cursor position.
- Bound keys produce real touchscreen events (`InjectTouchInput`) via an isolated child process.
- Toggle an always-on-top overlay showing all bound positions and key labels.
- Hold Ctrl while pressing bound keys to perform touch-drag gestures between positions.
- Minimise the main window to hide it to the system tray; use the tray's Show / Stop / Exit context menu to bring it back or control the app.
- Bindings saved to `Documents/clickassist.json` and restored on restart.

A low-level keyboard hook (`WH_KEYBOARD_LL`) sends JSON commands over stdin to a child worker process that calls `InitializeTouchInjection` and `InjectTouchInput`. The worker runs a 60Hz loop handling multi-touch contacts and interpolated drag moves.

## Usage

- **Record**: Click Record, then press any key to bind it to the mouse position.
- **Show Positions**: Toggle a transparent overlay showing all saved bindings.
- **Reset**: Clear all bindings and hide the overlay.
- **Start**: Activate bindings while keeping the main window visible. Press bound keys to inject touches.
- **Minimise**: Hide the main window to the tray. Click the tray icon or choose Show to bring it back.
- **Stop (Tray)**: Right-click the tray icon and select Stop to return to idle.
- **Esc**: Cancel recording mode.

When started:

- **Ctrl-drag**: While Started, hold Ctrl and press a bound key to begin a touch-drag; press another bound key to drag to that position.

## Build

```
cargo build --release
```

## Requirements

Windows 8 or later (for `InitializeTouchInjection` / `InjectTouchInput`).

## License

MIT No Attribution.

AI-Disclosure: ai-generated.
