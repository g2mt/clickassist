import sys
from typing import Optional

from PySide6.QtWidgets import (
    QMainWindow, QToolBar, QWidget, QMessageBox,
    QSystemTrayIcon, QMenu, QApplication
)
from PySide6.QtGui import QAction, QIcon, QKeySequence
from PySide6.QtCore import Qt, QPoint, QSize

from clickassist.platform.backend import Backend
from clickassist.ui.keybind_dialog import KeybindDialog
from clickassist.ui.position_window import PositionFrame, PositionWindow
from .mode import Mode


class MainWindow(QMainWindow):
    """Main application window with toolbar."""

    def __init__(self, backend: Backend):
        super().__init__()
        self.setWindowTitle("Click Assistant")
        self.resize(400, 30)

        self.backend = backend

        # State
        self._active_mode: Mode = Mode.NORMAL

        # Bindings: key_sequence string -> PositionFrame
        self._bindings: dict[str, PositionFrame] = {}
        # Single overlay window to contain all position frames
        self.position_window: PositionWindow = PositionWindow(self)

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
        toolbar.setToolButtonStyle(Qt.ToolButtonStyle.ToolButtonTextBesideIcon)
        self.setCentralWidget(toolbar)

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

        self.position_window.set_position_frames_movable(False)
        if active != Mode.RECORDING:
            if self._record_cb is not None:
                self._key_listener.key_event.disconnect(self._record_cb)
                self._record_cb = None
        if active != Mode.ACTIVE:
            try:
                self._key_listener.key_event.disconnect(self._handle_key_event)
            except:
                pass
            self.position_window.show()

        if active == Mode.ACTIVE:
            self._tray.show()
            self.hide()
            # Hide all position windows while active
            for pw in self._bindings.values():
                pw.hide()
            self._key_listener.key_event.connect(self._handle_key_event)
            self.position_window.hide()

        elif active == Mode.NORMAL:
            self._tray.hide()
            self.show()
            self.raise_()
            self.activateWindow()

        elif active == Mode.RECORDING:
            assert self._record_cb is None
            self._act_record.setChecked(True)
            def record_cb(data: tuple[str, bool]):
                self._key_listener.key_event.disconnect(self._record_cb)
                self._record_cb = None

                try:
                    pos: QPoint = self.backend.get_cursor_pos()
                except Exception as e:
                    QMessageBox.critical(
                        self, "Error getting cursor position",
                        str(e)
                    )
                    return
                dlg = KeybindDialog(data[0])
                def on_accept():
                    key = dlg.key
                    if key in self._bindings:
                        QMessageBox.warning(
                            self, "Already bound",
                            f"Key '{key}' is already bound. Delete it first."
                        )
                    else:
                        position_frame = PositionFrame(pos, key, self)
                        self._bindings[key] = position_frame
                    self._set_active_mode(Mode.NORMAL)
                dlg.accepted.connect(on_accept)
                dlg.exec()
            self._record_cb = record_cb
            self._key_listener.key_event.connect(record_cb)

        elif active == Mode.MOVE:
            self._act_move.setChecked(True)
            self.position_window.set_position_frames_movable(True)

        elif active == Mode.DELETE:
            self._act_delete.setChecked(True)


    ### Key event handler ###

    def _handle_key_event(self, data: tuple[str, bool]):
        """Handle key events when in ACTIVE mode."""
        if self._active_mode != Mode.ACTIVE:
            return
        
        key, pressed = data
        if position_frame := self._bindings.get(key):
            # Get the center position of the frame
            center_pos = position_frame.centerPosition()
            if pressed:
                # Mouse down when key is pressed
                self.backend.mouse_down(center_pos.x(), center_pos.y())
            else:
                # Mouse up when key is released
                self.backend.mouse_up(center_pos.x(), center_pos.y())

    ### Tray helpers ###

    def _on_tray_activated(self, reason):
        self._restore_from_tray()

    def _restore_from_tray(self):
        self._set_active_mode(Mode.NORMAL)
