import sys
from abc import ABC, abstractmethod
from typing import Optional, Callable, List

from PySide6.QtCore import Qt, QObject, Signal
from PySide6.QtGui import QKeySequence

class KeyListener(QObject):
    """Base class for global hotkey listeners."""

    # Signal emitted when a key event occurs: (keycode: str, pressed: bool)
    key_event = Signal(tuple)

    def __init__(self):
        super().__init__()
