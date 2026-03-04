"""Frameless window that shows a red circle at a bound mouse position."""

from typing import Optional

from PySide6.QtWidgets import QWidget
from PySide6.QtGui import QPainter, QColor
from PySide6.QtCore import Qt, QPoint, QRect


class PositionWindow(QWidget):
    """Frameless window that shows a red circle at a bound mouse position."""

    RADIUS: int = 16

    def __init__(
        self,
        position: QPoint,
        key_sequence: QKeySequence,
        parent: Optional[QWidget] = None,
    ):
        super().__init__(parent)
        self.position: QPoint = position          # screen position of the click
        self.key_sequence = key_sequence
        self._drag_offset: QPoint = QPoint()
        self._dragging: bool = False

        diameter = self.RADIUS * 2
        self.setFixedSize(diameter, diameter)
        self.setWindowFlags(
            Qt.WindowType.FramelessWindowHint
            | Qt.WindowType.WindowStaysOnTopHint
            | Qt.WindowType.Tool
        )
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.move(position - QPoint(self.RADIUS, self.RADIUS))
        self.show()

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
            centre = self.frameGeometry().topLeft() + QPoint(self.RADIUS, self.RADIUS)
            self.position = centre
        event.accept()
