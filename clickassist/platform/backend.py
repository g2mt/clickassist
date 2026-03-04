"""Platform-specific backend implementations."""

import sys
from abc import ABC, abstractmethod

from PySide6.QtCore import QPoint

from clickassist.platform.impl import OSBackend

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
