from typing import Optional, TYPE_CHECKING

from PySide6.QtWidgets import QWidget, QMessageBox, QApplication
from PySide6.QtGui import QPainter, QColor
from PySide6.QtCore import Qt, QPoint, QRect

from .mode import Mode

if TYPE_CHECKING:
    from clickassist.ui.main_window import MainWindow


class PositionWindow(QWidget):
    """Full screen overlay window that contains all PositionFrame widgets."""

    def __init__(self, main_window: "MainWindow"):
        super().__init__()
        self.main_window = main_window
        self.setWindowFlags(
            Qt.WindowType.FramelessWindowHint
            | Qt.WindowType.WindowStaysOnTopHint
        )
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        screen = QApplication.primaryScreen()
        self.setGeometry(screen.geometry())
        self.hide()

    def paintEvent(self, event):
        """Paint a semi-transparent black background."""
        painter = QPainter(self)
        painter.setBrush(QColor(0, 0, 0, 76))  # 76 is approximately 0.3 * 255
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawRect(self.rect())

    def showEvent(self, event):
        """Resize to cover all screens when shown."""
        # Get the combined geometry of all screens
        screens = QApplication.screens()
        if screens:
            # We'll set the window to cover the primary screen for simplicity
            # In a more robust implementation, we could cover all screens
            screen_geometry = screens[0].geometry()
            self.setGeometry(screen_geometry)
        super().showEvent(event)

    def set_position_frames_movable(self, movable: bool):
        """Enable or disable mouse tracking / dragging on all position windows."""
        for child in self.children():
            if isinstance(child, PositionFrame):
                child.setMouseTracking(movable)
                if movable:
                    child.setCursor(Qt.CursorShape.SizeAllCursor)
                else:
                    child.setCursor(Qt.CursorShape.ArrowCursor)


class PositionFrame(QWidget):
    """Frame that shows a red circle at a bound mouse position."""

    RADIUS: int = 16

    def __init__(
        self,
        position: QPoint,
        key: str,
        main_window: "MainWindow",
    ):
        super().__init__(main_window.position_window)
        self.main_window = main_window
        self.key = key
        self._drag_offset: QPoint = QPoint()
        self._dragging: bool = False

        diameter = self.RADIUS * 2
        self.setFixedSize(diameter, diameter)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        adjusted_pos = position - QPoint(self.RADIUS, self.RADIUS)
        self.move(adjusted_pos)
        self.show()

    def centerPosition(self):
        return self.pos() + QPoint(self.RADIUS, self.RADIUS)

    ### painting ###

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setBrush(QColor(220, 30, 30, 200))
        painter.setPen(QColor(255, 255, 255, 220))
        painter.drawEllipse(0, 0, self.width() - 1, self.height() - 1)

    ### drag support (used in Move mode) ###

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            if self.main_window._active_mode == Mode.DELETE:
                reply = QMessageBox.question(
                    self,
                    "Delete binding",
                    f"Delete binding for key '{self.key}'?",
                    QMessageBox.Yes | QMessageBox.No,
                )
                if reply == QMessageBox.Yes:
                    self.hide()
                    self.deleteLater()
                    # Remove from bindings
                    if self.key in self.main_window._bindings:
                        del self.main_window._bindings[self.key]
                return
            self._dragging = True
            self._drag_offset = event.globalPosition().toPoint() - self.frameGeometry().topLeft()
        event.accept()

    def mouseMoveEvent(self, event):
        if self._dragging and (event.buttons() & Qt.MouseButton.LeftButton):
            self.move(event.globalPosition().toPoint() - self._drag_offset)
        event.accept()

    def mouseReleaseEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self._dragging = False
        event.accept()
