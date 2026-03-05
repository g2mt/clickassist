import sys
from abc import ABC, abstractmethod

from clickassist.platform.key import KeyListener

from PySide6.QtCore import QPoint

class Backend(ABC):
    """Abstract base class for platform-specific backend operations."""

    @abstractmethod
    def mouse_down(self, x: int, y: int):
        """Perform a mouse down at the given coordinates."""
        pass

    @abstractmethod
    def mouse_up(self, x: int, y: int):
        """Perform a mouse up at the given coordinates."""
        pass

    @abstractmethod
    def get_cursor_pos(self) -> QPoint:
        """Get the current cursor position."""
        pass

    @abstractmethod
    def create_key_listener(self) -> KeyListener:
        """Create and return a hotkey listener for this platform."""
        pass
