import sys
from typing import Optional

from PySide6.QtWidgets import (
    QMainWindow, QToolBar, QWidget, QMessageBox,
    QSystemTrayIcon, QMenu, QApplication
)
from PySide6.QtGui import QAction, QIcon, QKeySequence
from PySide6.QtCore import Qt, QPoint, QSize

from clickassist.platform.backend import AbstractBackend
from clickassist.platform.hotkey import AbstractHotkeyListener
from clickassist.ui.keybind_dialog import KeybindDialog
from clickassist.ui.position_window import PositionWindow


class MainWindow(QMainWindow):
    """Main application window with toolbar."""

    def __init__(self, backend: AbstractBackend):
        super().__init__()
        self.setWindowTitle("Click Assistant")
        self.resize(400, 120)

        self._backend = backend

        # State
        self._recording: bool = False
        self._move_mode: bool = False
        self._delete_mode: bool = False
        self._active: bool = False  # keybinds running / tray mode

        # Bindings: key_sequence string -> PositionWindow
        self._bindings: dict[str, PositionWindow] = {}

        self._hotkey_listener: Optional[AbstractHotkeyListener] = None

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
        self._activate_keybinds()
        self._tray.show()
        self.hide()
        # Hide all position windows while active
        for pw in self._bindings.values():
            pw.hide()

    def _on_record(self, checked: bool):
        self._recording = checked
        if checked:
            self._set_exclusive_mode("record")
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
            self._act_record.setChecked(False)
            self._recording = False
        else:
            self._show_all_position_windows()

    def _on_move(self, checked: bool):
        self._move_mode = checked
        if checked:
            self._set_exclusive_mode("move")
            self._show_all_position_windows()
            self._set_position_windows_movable(True)
        else:
            self._set_position_windows_movable(False)
            self._act_move.setChecked(False)
            self._move_mode = False

    def _on_delete(self, checked: bool):
        self._delete_mode = checked
        if checked:
            self._set_exclusive_mode("delete")
            self._show_all_position_windows()
        else:
            self._act_delete.setChecked(False)
            self._delete_mode = False

    ### Mode helpers ###

    def _set_exclusive_mode(self, active: str):
        """Uncheck all mode buttons except the active one."""
        if active != "record":
            self._act_record.setChecked(False)
            self._recording = False
        if active != "move":
            self._act_move.setChecked(False)
            self._move_mode = False
        if active != "delete":
            self._act_delete.setChecked(False)
            self._delete_mode = False

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
            if self._delete_mode:
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

    ### Keybind activation / deactivation ###

    def _activate_keybinds(self):
        self._hotkey_listener = self._backend.create_hotkey_listener()
        for ks_str, pw in self._bindings.items():
            ks = QKeySequence(ks_str)
            pos = pw.position

            def make_cb(x: int, y: int):
                def cb():
                    self._backend.click(x, y)
                return cb

            self._hotkey_listener.register(ks, make_cb(pos.x(), pos.y()))

        self._hotkey_listener.start()
        self._active = True

    def _deactivate_keybinds(self):
        if self._hotkey_listener:
            self._hotkey_listener.unregister_all()
        self._active = False

    ### Tray helpers ###

    def _on_tray_activated(self, reason):
        if reason == QSystemTrayIcon.ActivationReason.DoubleClick:
            self._restore_from_tray()

    def _restore_from_tray(self):
        self._deactivate_keybinds()
        self._tray.hide()
        self.show()
        self.raise_()
        self.activateWindow()
        self._show_all_position_windows()

    ### Close event ###

    def closeEvent(self, event):
        self._deactivate_keybinds()
        for pw in self._bindings.values():
            pw.close()
        event.accept()
