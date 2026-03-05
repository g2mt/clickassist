import sys
from abc import ABC, abstractmethod
from typing import Optional, Callable, List

from PySide6.QtCore import Qt
from PySide6.QtGui import QKeySequence

class KeyListener:
    """Base class for global hotkey listeners."""

    def __init__(self):
        self._callbacks: List[Callable[[int, bool], None]] = []

    def add_cb(self, callback: Callable[[int, bool], None]) -> None:
        """Add a callback function to be called on key events.

        Args:
            callback: A function that takes (keycode: int, pressed: bool) and returns None.
        """
        if callback not in self._callbacks:
            self._callbacks.append(callback)

    def remove_cb(self, callback: Callable[[int, bool], None]) -> None:
        """Remove a previously added callback."""
        if callback in self._callbacks:
            self._callbacks.remove(callback)

    def _emit(self, data: tuple[int, bool]) -> None:
        """Emit key event to all registered callbacks.

        Args:
            data: A tuple (keycode: int, pressed: bool)
        """
        keycode, pressed = data
        for cb in self._callbacks:
            cb(keycode, pressed)
