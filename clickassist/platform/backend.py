"""Platform-specific backend implementations."""

import sys
import subprocess
from abc import ABC, abstractmethod
from typing import Optional

from PySide6.QtCore import QPoint

from clickassist.utils.constants import PLATFORM_WINDOWS

if PLATFORM_WINDOWS:
    import win32api
    import win32con


class Backend(ABC):
    """Abstract base class for platform-specific backend operations."""

    @abstractmethod
    def click(self, x: int, y: int):
        """Perform a mouse click at the given coordinates."""
        pass

    @abstractmethod
    def get_cursor_pos(self) -> QPoint:
        """Get the current cursor position."""
        pass

    @abstractmethod
    def create_hotkey_listener(self):
        """Create and return a hotkey listener for this platform."""
        pass


if PLATFORM_WINDOWS:
    class WindowsBackend(Backend):
        """Windows-specific backend implementation using Win32 API."""

        def click(self, x: int, y: int):
            win32api.SetCursorPos((x, y))
            win32api.mouse_event(win32con.MOUSEEVENTF_LEFTDOWN, x, y, 0, 0)
            win32api.mouse_event(win32con.MOUSEEVENTF_LEFTUP, x, y, 0, 0)

        def get_cursor_pos(self) -> QPoint:
            pos = win32api.GetCursorPos()
            return QPoint(pos[0], pos[1])

        def create_hotkey_listener(self):
            from clickassist.platform.hotkey import WindowsHotkeyListener
            return WindowsHotkeyListener()

else:
    class WaylandBackend(Backend):
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
            from clickassist.platform.hotkey import WaylandHotkeyListener
            return WaylandHotkeyListener()
