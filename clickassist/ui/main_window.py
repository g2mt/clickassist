import sys
from typing import Optional
from enum import Enum, auto

from PySide6.QtWidgets import (
    QMainWindow, QToolBar, QWidget, QMessageBox,
    QSystemTrayIcon, QMenu, QApplication
)
from PySide6.QtGui import QAction, QIcon, QKeySequence
from PySide6.QtCore import Qt, QPoint, QSize

from clickassist.platform.backend import Backend
from clickassist.ui.keybind_dialog import KeybindDialog
from clickassist.ui.position_window import PositionWindow


class Mode(Enum):
    """Represents the current active mode of the application."""
    ACTIVE = auto()      # Keybinds running / tray mode
    NORMAL = auto()      # No special mode active
    RECORDING = auto()   # Recording a new keybind
    MOVE = auto()        # Moving a bound position
    DELETE = auto()      # Deleting a bound position


class MainWindow(QMainWindow):
    """Main application window with toolbar."""

    def __init__(self, backend: Backend):
        super().__init__()
        self.setWindowTitle("Click Assistant")
        self.resize(400, 120)

        self._backend = backend

        # State
        self._active_mode: Mode = Mode.NORMAL

        # Bindings: key_sequence string -> PositionWindow
        self._bindings: dict[str, PositionWindow] = {}

        self._key_listener: KeyListener = backend.create_key_listener()
        self._record_cb = None

        self._build_toolbar()
        self._build_tray()

    ### Events ###

    def closeEvent(self, event):
        """Handle main window close event."""
        # Quit the application when the main window is closed
        QApplication.quit()
        event.accept()

    ### UI construction ###

    def _build_toolbar(self):
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

    def _build_tray(self):
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

    def _on_start(self):
        """Minimise to tray and activate keybinds."""
        self._set_active_mode(Mode.ACTIVE)

    def _on_record(self, checked: bool):
        if checked:
            self._set_active_mode(Mode.RECORDING)
        else:
            self._set_active_mode(Mode.NORMAL)

    def _on_move(self, checked: bool):
        if checked:
            self._set_active_mode(Mode.MOVE)
        else:
            self._set_active_mode(Mode.NORMAL)

    def _on_delete(self, checked: bool):
        if checked:
            self._set_active_mode(Mode.DELETE)
        else:
            self._set_active_mode(Mode.NORMAL)

    ### Mode helpers ###

    def _set_active_mode(self, active: Mode):
        """Uncheck all mode buttons except the active one and set active mode."""
        if active == self._active_mode:
            return
        self._active_mode = active

        self._act_record.setChecked(False)
        self._act_move.setChecked(False)
        self._act_delete.setChecked(False)

        self._set_position_windows_movable(False)
        if active != Mode.RECORDING:
            self._key_listener.remove_cb(self._record_cb)
            self._record_cb = None

        if active == Mode.ACTIVE:
            self._tray.show()
            self.hide()
            # Hide all position windows while active
            for pw in self._bindings.values():
                pw.hide()

        elif active == Mode.NORMAL:
            self._tray.hide()
            self.show()
            self.raise_()
            self.activateWindow()
            self._show_all_position_windows()

        elif active == Mode.RECORDING:
            assert self._record_cb is None
            self._act_record.setChecked(True)
            self._show_all_position_windows()
            def record_cb():
                # Capture current cursor position
                pos: QPoint = self._backend.get_cursor_pos()
                # Ask for keybind
                dlg = KeybindDialog(self)
                if key := dlg.exec():
                    if key in self._bindings:
                        QMessageBox.warning(
                            self, "Already bound",
                            f"Key '{key}' is already bound. Delete it first."
                        )
                    else:
                        self._bindings[key] = PositionWindow(pos, key)
            self._record_cb = record_cb
            self._key_listener.add_cb(record_cb)

        elif active == Mode.MOVE:
            self._act_move.setChecked(True)
            self._show_all_position_windows()
            self._set_position_windows_movable(True)

        elif active == Mode.DELETE:
            self._act_delete.setChecked(True)
            self._show_all_position_windows()

    def _show_all_position_windows(self):
        for pw in self._bindings.values():
            pw.show()

    def _set_position_windows_movable(self, movable: bool):
        """Enable or disable mouse tracking / dragging on position windows."""
        for pw in self._bindings.values():
            pw.setMouseTracking(movable)
            if movable:
                pw.setCursor(Qt.CursorShape.SizeAllCursor)
            else:
                pw.setCursor(Qt.CursorShape.ArrowCursor)

    ### Tray helpers ###

    def _on_tray_activated(self, reason):
        self._restore_from_tray()

    def _restore_from_tray(self):
        self._set_active_mode(Mode.NORMAL)
