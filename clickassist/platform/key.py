import sys
from abc import ABC, abstractmethod
from typing import Optional

from PySide6.QtCore import Qt
from PySide6.QtGui import QKeySequence

class AbstractKeyListener(ABC):
    """Abstract base class for global hotkey listeners."""

    @abstractmethod
    def next_key(self):
        """
        next_key() yields tuples of (key_name: str, pressed: bool).
        """
        pass
