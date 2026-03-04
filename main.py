import sys
import os
import json
import subprocess
from abc import ABC, abstractmethod
from typing import Optional

from PySide6.QtWidgets import (
    QApplication, QMainWindow, QToolBar, QWidget,
    QDialog, QVBoxLayout, QLabel, QPushButton, QMessageBox,
    QSystemTrayIcon, QMenu
)
from PySide6.QtGui import QAction, QPainter, QColor, QIcon, QKeySequence
from PySide6.QtCore import Qt, QPoint, QRect, QSize

PLATFORM_WINDOWS: bool = sys.platform == "win32"

if PLATFORM_WINDOWS:
    import win32api
    import win32con
    import ctypes
    from ctypes import wintypes
    import threading

### Abstract base classes ###

class AbstractHotkeyListener(ABC):
    """Abstract base class for global hotkey listeners."""

    @abstractmethod
    def register(self, key_sequence: QKeySequence, callback: callable) -> int:
        """Register a hotkey and return its ID."""
        pass

    @abstractmethod
    def unregister_all(self) -> None:
        """Unregister all hotkeys."""
        pass

    @abstractmethod
    def start(self) -> None:
        """Start listening for hotkeys."""
        pass


class Backend(ABC):
    """Abstract base class for platform-specific backend operations."""

    @abstractmethod
    def click(self, x: int, y: int) -> None:
        """Perform a mouse click at the given coordinates."""
        pass

    @abstractmethod
    def get_cursor_pos(self) -> QPoint:
        """Get the current cursor position."""
        pass

    @abstractmethod
    def create_hotkey_listener(self) -> AbstractHotkeyListener:
        """Create and return a hotkey listener for this platform."""
        pass


### Windows implementation ###

if PLATFORM_WINDOWS:

    class WindowsHotkeyListener(threading.Thread, AbstractHotkeyListener):
        """Registers Win32 global hotkeys and fires callbacks."""

        def __init__(self) -> None:
            super().__init__(daemon=True)
            self._bindings: dict[int, tuple[QKeySequence, callable]] = {}
            self._id_counter: int = 1
            self._hwnd: Optional[int] = None

        def register(self, key_sequence: QKeySequence, callback: callable) -> int:
            hk_id = self._id_counter
            self._id_counter += 1
            self._bindings[hk_id] = (key_sequence, callback)
            return hk_id

        def unregister_all(self) -> None:
            if self._hwnd:
                for hk_id in list(self._bindings.keys()):
                    ctypes.windll.user32.UnregisterHotKey(self._hwnd, hk_id)
            self._bindings.clear()

        def _qs_to_win32(self, qs: QKeySequence):
            key = qs[0].key()
            mods = qs[0].keyboardModifiers()
            win_mod = 0
            if mods & Qt.KeyboardModifier.AltModifier:
                win_mod |= win32con.MOD_ALT
            if mods & Qt.KeyboardModifier.ControlModifier:
                win_mod |= win32con.MOD_CONTROL
            if mods & Qt.KeyboardModifier.ShiftModifier:
                win_mod |= win32con.MOD_SHIFT
            if mods & Qt.KeyboardModifier.MetaModifier:
                win_mod |= win32con.MOD_WIN
            vk = int(key)
            # Map Qt key to VK
            vk_code = ctypes.windll.user32.VkKeyScanW(vk) & 0xFF
            return win_mod, vk_code

        def run(self) -> None:
            import ctypes.wintypes as wt
            # Create a message-only window handle via a dummy approach
            for hk_id, (qs, _cb) in self._bindings.items():
                mod, vk = self._qs_to_win32(qs)
                ctypes.windll.user32.RegisterHotKey(None, hk_id, mod, vk)

            msg = wt.MSG()
            while ctypes.windll.user32.GetMessageW(ctypes.byref(msg), None, 0, 0) != 0:
                if msg.message == win32con.WM_HOTKEY:
                    hk_id = msg.wParam
                    if hk_id in self._bindings:
                        _qs, cb = self._bindings[hk_id]
                        cb()
                ctypes.windll.user32.TranslateMessage(ctypes.byref(msg))
                ctypes.windll.user32.DispatchMessageW(ctypes.byref(msg))

        def start(self) -> None:
            """Start the listener thread."""
            super().start()

    class WindowsBackend(Backend):
        """Windows-specific backend implementation using Win32 API."""

        def click(self, x: int, y: int) -> None:
            win32api.SetCursorPos((x, y))
            win32api.mouse_event(win32con.MOUSEEVENTF_LEFTDOWN, x, y, 0, 0)
            win32api.mouse_event(win32con.MOUSEEVENTF_LEFTUP, x, y, 0, 0)

        def get_cursor_pos(self) -> QPoint:
            pos = win32api.GetCursorPos()
            return QPoint(pos[0], pos[1])

        def create_hotkey_listener(self) -> AbstractHotkeyListener:
            return WindowsHotkeyListener()

else:
    ### Linux/Wayland implementation ###

    try:
        import keyboard as kb_lib
        _KB_AVAILABLE = True
    except ImportError:
        _KB_AVAILABLE = False

    class WaylandHotkeyListener(AbstractHotkeyListener):
        """Linux/Wayland hotkey listener using the keyboard library."""

        def __init__(self) -> None:
            self._hooks: list = []

        def register(self, key_sequence: QKeySequence, callback: callable) -> int:
            if not _KB_AVAILABLE:
                return -1
            hotkey_str = key_sequence.toString().lower().replace("+", "+")
            hook = kb_lib.add_hotkey(hotkey_str, callback)
            self._hooks.append(hook)
            return len(self._hooks) - 1

        def unregister_all(self) -> None:
            if not _KB_AVAILABLE:
                return
            for hook in self._hooks:
                try:
                    kb_lib.remove_hotkey(hook)
                except Exception:
                    pass
            self._hooks.clear()

        def start(self) -> None:
            """Keyboard library works without a thread."""
            pass

    class WaylandBackend(Backend):
        """Linux/Wayland-specific backend implementation using ydotool."""

        def click(self, x: int, y: int) -> None:
            subprocess.Popen(
                ["ydotool", "mousemove", "--absolute", "-x", str(x), "-y", str(y)]
            ).wait()
            subprocess.Popen(["ydotool", "click", "0xC0"]).wait()

        def get_cursor_pos(self) -> QPoint:
            # Use ydotool to get position; fall back to Qt
            try:
                out = subprocess.check_output(["ydotool", "getmouselocation"])
                parts = out.decode().split()
                x = int(parts[0].split(":")[1])
                y = int(parts[1].split(":")[1])
                return QPoint(x, y)
            except Exception:
                return QApplication.primaryScreen().geometry().center()

        def create_hotkey_listener(self) -> AbstractHotkeyListener:
            return WaylandHotkeyListener()


### Keybind capture dialog ###

class KeybindDialog(QDialog):
    """Dialog that waits for the user to press a key and records it."""

    def __init__(self, parent: Optional[QWidget] = None) -> None:
        super().__init__(parent)
        self.setWindowTitle("Press a key")
        self.setModal(True)
        self.key_sequence: Optional[QKeySequence] = None

        layout = QVBoxLayout(self)
        self.label = QLabel("Press any key to bind …", self)
        self.label.setAlignment(Qt.AlignCenter)
        layout.addWidget(self.label)

        cancel_btn = QPushButton("Cancel", self)
        cancel_btn.clicked.connect(self.reject)
        layout.addWidget(cancel_btn)

    def keyPressEvent(self, event) -> None:  # type: ignore[override]
        key = event.key()
        if key in (Qt.Key.Key_unknown, Qt.Key.Key_Control, Qt.Key.Key_Shift,
                   Qt.Key.Key_Alt, Qt.Key.Key_Meta):
            return
        modifiers = event.modifiers()
        self.key_sequence = QKeySequence(int(modifiers) | int(key))
        self.label.setText(f"Bound to: {self.key_sequence.toString()}")
        self.accept()


### Overlay circle window ###

class PositionWindow(QWidget):
    """Frameless window that shows a red circle at a bound mouse position."""

    RADIUS: int = 16

    def __init__(
        self,
        position: QPoint,
        key_sequence: QKeySequence,
        parent: Optional[QWidget] = None,
    ) -> None:
        super().__init__(parent)
        self.position: QPoint = position          # screen position of the click
        self.key_sequence: QKeySequence = key_sequence
        self._drag_offset: QPoint = QPoint()
        self._dragging: bool = False

        diameter = self.RADIUS * 2
        self.setFixedSize(diameter, diameter)
        self.setWindowFlags(
            Qt.WindowType.FramelessWindowHint
            | Qt.WindowType.WindowStaysOnTopHint
            | Qt.WindowType.Tool
        )
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.move(position - QPoint(self.RADIUS, self.RADIUS))
        self.show()

    ### painting ###

    def paintEvent(self, event) -> None:  # type: ignore[override]
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setBrush(QColor(220, 30, 30, 200))
        painter.setPen(QColor(255, 255, 255, 220))
        painter.drawEllipse(0, 0, self.width() - 1, self.height() - 1)

    ### drag support (used in Move mode) ###

    def mousePressEvent(self, event) -> None:  # type: ignore[override]
        if event.button() == Qt.MouseButton.LeftButton:
            self._dragging = True
            self._drag_offset = event.globalPosition().toPoint() - self.frameGeometry().topLeft()
        event.accept()

    def mouseMoveEvent(self, event) -> None:  # type: ignore[override]
        if self._dragging and (event.buttons() & Qt.MouseButton.LeftButton):
            self.move(event.globalPosition().toPoint() - self._drag_offset)
        event.accept()

    def mouseReleaseEvent(self, event) -> None:  # type: ignore[override]
        if event.button() == Qt.MouseButton.LeftButton:
            self._dragging = False
            centre = self.frameGeometry().topLeft() + QPoint(self.RADIUS, self.RADIUS)
            self.position = centre
        event.accept()


### Main window ###

class MainWindow(QMainWindow):
    """Main application window with toolbar."""

    def __init__(self, backend: Backend) -> None:
        super().__init__()
        self.setWindowTitle("Click Assistant")
        self.resize(400, 120)

        self._backend = backend

        # State
        self._recording: bool = False
        self._move_mode: bool = False
        self._delete_mode: bool = False
        self._active: bool = False  # keybinds running / tray mode

        # Bindings: key_sequence string -> PositionWindow
        self._bindings: dict[str, PositionWindow] = {}

        self._hotkey_listener: Optional[AbstractHotkeyListener] = None

        self._build_toolbar()
        self._build_tray()

    ### UI construction ###

    def _build_toolbar(self) -> None:
        toolbar: QToolBar = QToolBar("Main Toolbar", self)
        toolbar.setIconSize(QSize(24, 24))
        self.addToolBar(toolbar)

        # Start
        self._act_start = QAction(
            QIcon.fromTheme("media-playback-start"), "Start"
        )
        self._act_start.setToolTip("Minimise to tray and activate keybinds")
        self._act_start.triggered.connect(self._on_start)
        toolbar.addAction(self._act_start)

        # Record
        self._act_record = QAction(
            QIcon.fromTheme("media-record"), "Record"
        )
        self._act_record.setToolTip("Record a new mouse position keybind")
        self._act_record.setCheckable(True)
        self._act_record.triggered.connect(self._on_record)
        toolbar.addAction(self._act_record)

        # Move
        self._act_move = QAction(
            QIcon.fromTheme("transform-move"), "Move"
        )
        self._act_move.setToolTip("Move a bound position")
        self._act_move.setCheckable(True)
        self._act_move.triggered.connect(self._on_move)
        toolbar.addAction(self._act_move)

        # Delete
        self._act_delete = QAction(
            QIcon.fromTheme("edit-delete"), "Delete"
        )
        self._act_delete.setToolTip("Delete a bound position")
        self._act_delete.setCheckable(True)
        self._act_delete.triggered.connect(self._on_delete)
        toolbar.addAction(self._act_delete)

    def _build_tray(self) -> None:
        self._tray = QSystemTrayIcon(
            QIcon.fromTheme("input-mouse"), self
        )
        tray_menu = QMenu()
        restore_action = QAction("Restore")
        restore_action.triggered.connect(self._restore_from_tray)
        quit_action = QAction("Quit")
        quit_action.triggered.connect(QApplication.quit)
        tray_menu.addAction(restore_action)
        tray_menu.addSeparator()
        tray_menu.addAction(quit_action)
        self._tray.setContextMenu(tray_menu)
        self._tray.activated.connect(self._on_tray_activated)

    ### Toolbar handlers ###

    def _on_start(self) -> None:
        """Minimise to tray and activate keybinds."""
        self._activate_keybinds()
        self._tray.show()
        self.hide()
        # Hide all position windows while active
        for pw in self._bindings.values():
            pw.hide()

    def _on_record(self, checked: bool) -> None:
        self._recording = checked
        if checked:
            self._set_exclusive_mode("record")
            self._show_all_position_windows()
            # Capture current cursor position
            pos: QPoint = self._backend.get_cursor_pos()
            # Ask for keybind
            dlg = KeybindDialog(self)
            if dlg.exec_() == QDialog.Accepted and dlg.key_sequence:
                ks: QKeySequence = dlg.key_sequence
                ks_str: str = ks.toString()
                if ks_str in self._bindings:
                    QMessageBox.warning(
                        self, "Already bound",
                        f"Key '{ks_str}' is already bound. Delete it first."
                    )
                else:
                    pw = PositionWindow(pos, ks)
                    pw.mousePressEvent = self._make_pw_press_handler(pw, pw.mousePressEvent)
                    self._bindings[ks_str] = pw
            self._act_record.setChecked(False)
            self._recording = False
        else:
            self._show_all_position_windows()

    def _on_move(self, checked: bool) -> None:
        self._move_mode = checked
        if checked:
            self._set_exclusive_mode("move")
            self._show_all_position_windows()
            self._set_position_windows_movable(True)
        else:
            self._set_position_windows_movable(False)
            self._act_move.setChecked(False)
            self._move_mode = False

    def _on_delete(self, checked: bool) -> None:
        self._delete_mode = checked
        if checked:
            self._set_exclusive_mode("delete")
            self._show_all_position_windows()
        else:
            self._act_delete.setChecked(False)
            self._delete_mode = False

    ### Mode helpers ###

    def _set_exclusive_mode(self, active: str) -> None:
        """Uncheck all mode buttons except the active one."""
        if active != "record":
            self._act_record.setChecked(False)
            self._recording = False
        if active != "move":
            self._act_move.setChecked(False)
            self._move_mode = False
        if active != "delete":
            self._act_delete.setChecked(False)
            self._delete_mode = False

    def _show_all_position_windows(self) -> None:
        for pw in self._bindings.values():
            pw.show()

    def _set_position_windows_movable(self, movable: bool) -> None:
        """Enable or disable mouse tracking / dragging on position windows."""
        for pw in self._bindings.values():
            pw.setMouseTracking(movable)
            if movable:
                pw.setCursor(Qt.CursorShape.SizeAllCursor)
            else:
                pw.setCursor(Qt.CursorShape.ArrowCursor)

    def _make_pw_press_handler(self, pw: PositionWindow, original_handler):
        """Wrap a PositionWindow's mousePressEvent to support delete mode."""
        def handler(event) -> None:
            if self._delete_mode:
                ks_str = pw.key_sequence.toString()
                reply = QMessageBox.question(
                    self,
                    "Delete binding",
                    f"Delete binding for key '{ks_str}'?",
                    QMessageBox.Yes | QMessageBox.No,
                )
                if reply == QMessageBox.Yes:
                    pw.hide()
                    pw.deleteLater()
                    del self._bindings[ks_str]
                return
            original_handler(event)
        return handler

    ### Keybind activation / deactivation ###

    def _activate_keybinds(self) -> None:
        self._hotkey_listener = self._backend.create_hotkey_listener()
        for ks_str, pw in self._bindings.items():
            ks = QKeySequence(ks_str)
            pos = pw.position

            def make_cb(x: int, y: int):
                def cb():
                    self._backend.click(x, y)
                return cb

            self._hotkey_listener.register(ks, make_cb(pos.x(), pos.y()))

        self._hotkey_listener.start()
        self._active = True

    def _deactivate_keybinds(self) -> None:
        if self._hotkey_listener:
            self._hotkey_listener.unregister_all()
        self._active = False

    ### Tray helpers ###

    def _on_tray_activated(self, reason: QSystemTrayIcon.ActivationReason) -> None:
        if reason == QSystemTrayIcon.ActivationReason.DoubleClick:
            self._restore_from_tray()

    def _restore_from_tray(self) -> None:
        self._deactivate_keybinds()
        self._tray.hide()
        self.show()
        self.raise_()
        self.activateWindow()
        self._show_all_position_windows()

    ### Close event ###

    def closeEvent(self, event) -> None:  # type: ignore[override]
        self._deactivate_keybinds()
        for pw in self._bindings.values():
            pw.close()
        event.accept()


### Entry point ###

def main() -> None:
    app = QApplication(sys.argv)
    app.setQuitOnLastWindowClosed(False)
    
    # Create the appropriate backend for the platform
    if PLATFORM_WINDOWS:
        backend: Backend = WindowsBackend()
    else:
        backend = WaylandBackend()
    
    window = MainWindow(backend)
    window.show()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
