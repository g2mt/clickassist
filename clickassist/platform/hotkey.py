"""Platform-specific hotkey listener implementations."""

import sys
from abc import ABC, abstractmethod
from typing import Optional

from PySide6.QtCore import Qt
from PySide6.QtGui import QKeySequence

class AbstractHotkeyListener(ABC):
    """Abstract base class for global hotkey listeners."""

    @abstractmethod
    def register(self, key_sequence: QKeySequence, callback: callable) -> int:
        """Register a hotkey and return its ID."""
        pass

    @abstractmethod
    def unregister_all(self):
        """Unregister all hotkeys."""
        pass

    @abstractmethod
    def start(self):
        """Start listening for hotkeys."""
        pass
