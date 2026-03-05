import sys
from typing import Optional
from enum import Enum, auto

from PySide6.QtWidgets import (
    QMainWindow, QToolBar, QWidget, QMessageBox,
    QSystemTrayIcon, QMenu, QApplication
)
from PySide6.QtGui import QAction, QIcon, QKeySequence
from PySide6.QtCore import Qt, QPoint, QSize

from clickassist.platform.backend import AbstractBackend
from clickassist.ui.keybind_dialog import KeybindDialog
from clickassist.ui.position_window import PositionWindow


class Mode(Enum):
    """Represents the current active mode of the application."""
    NORMAL = auto()      # No special mode active
    RECORDING = auto()   # Recording a new keybind
    MOVE = auto()        # Moving a bound position
    DELETE = auto()      # Deleting a bound position
    ACTIVE = auto()      # Keybinds running / tray mode


class MainWindow(QMainWindow):
    """Main application window with toolbar."""

    def __init__(self, backend: AbstractBackend):
        super().__init__()
        self.setWindowTitle("Click Assistant")
        self.resize(400, 120)

        self._backend = backend

        # State
        self._active_mode: Mode = Mode.NORMAL

        # Bindings: key_sequence string -> PositionWindow
        self._bindings: dict[str, PositionWindow] = {}

        self._key_listener: Optional[AbstractKeyListener] = None

        self._build_toolbar()
        self._build_tray()

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
        self._tray.show()
        self.hide()
        # Hide all position windows while active
        for pw in self._bindings.values():
            pw.hide()

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
        self._active_mode = active
        self._act_record.setChecked(False)
        self._act_move.setChecked(False)
        self._act_delete.setChecked(False)

        if active == Mode.RECORDING:
            self._act_record.setChecked(True)
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

    def _make_pw_press_handler(self, pw: PositionWindow, original_handler):
        """Wrap a PositionWindow's mousePressEvent to support delete mode."""
        def handler(event):
            if self._active_mode == Mode.DELETE:
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

    ### Tray helpers ###

    def _on_tray_activated(self, reason):
        if reason == QSystemTrayIcon.ActivationReason.DoubleClick:
            self._restore_from_tray()

    def _restore_from_tray(self):
        self._set_active_mode(Mode.NORMAL)
        self._tray.hide()
        self.show()
        self.raise_()
        self.activateWindow()
        self._show_all_position_windows()
