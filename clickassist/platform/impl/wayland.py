import subprocess
from typing import Optional

from PySide6.QtCore import QPoint
from PySide6.QtGui import QKeySequence

from clickassist.platform.hotkey import AbstractHotkeyListener

try:
    import keyboard as kb_lib
    _KB_AVAILABLE = True
except ImportError:
    _KB_AVAILABLE = False


class WaylandHotkeyListener(AbstractHotkeyListener):
    """Linux/Wayland hotkey listener using the keyboard library."""

    def __init__(self):
        self._hooks: list = []

    def register(self, key_sequence: QKeySequence, callback: callable) -> int:
        if not _KB_AVAILABLE:
            return -1
        hotkey_str = key_sequence.toString().lower().replace("+", "+")
        hook = kb_lib.add_hotkey(hotkey_str, callback)
        self._hooks.append(hook)
        return len(self._hooks) - 1

    def unregister_all(self):
        if not _KB_AVAILABLE:
            return
        for hook in self._hooks:
            try:
                kb_lib.remove_hotkey(hook)
            except Exception:
                pass
        self._hooks.clear()

    def start(self):
        """Keyboard library works without a thread."""
        pass


class WaylandBackend:
    """Linux/Wayland-specific backend implementation using ydotool."""

    def click(self, x: int, y: int):
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
            from PySide6.QtWidgets import QApplication
            return QApplication.primaryScreen().geometry().center()

    def create_hotkey_listener(self):
        return WaylandHotkeyListener()
