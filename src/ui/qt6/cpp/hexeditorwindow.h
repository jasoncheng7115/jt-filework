// The hex editor: a file edited as bytes.
//
// The read-only hex dump lives in the viewer. This is the other thing - a
// window that can change a file and write it back - and it is separate for the
// reason AGENTS.md 14 separates preview from viewer: looking and changing are
// different jobs, and a view that could change the file by a stray keystroke
// would make every look a risk. It opens read-only for the same reason.
//
// Everything about bytes, offsets, undo and search is decided on the Rust side
// (src/app/src/hexedit.rs over jtf-hexedit). This file draws what it
// is told and forwards keystrokes.
#pragma once

#include "bridge.h"

#include <QAbstractScrollArea>
#include <QColor>
#include <QWidget>

class QCheckBox;
class QComboBox;
class QLabel;
class QLineEdit;
class QPushButton;

/// The byte grid: offsets, sixteen bytes a row in hex, and the same bytes as
/// text. Painted directly rather than through a model, because every row is a
/// question to the Rust side and a model would only add a copy of the answer.
class HexView : public QAbstractScrollArea {
    Q_OBJECT

public:
    explicit HexView(JtfApp *app, QWidget *parent = nullptr);

    void setColours(const QColor &text, const QColor &dim, const QColor &changed,
                    const QColor &selection, const QColor &cursor, const QColor &base);
    /// Re-read the file's length and repaint: after an edit, an undo, a find.
    void refresh();
    /// Scroll so the cursor's row is on screen.
    void ensureCursorVisible();

    QSize sizeHint() const override;

signals:
    /// The cursor, the selection or the contents changed.
    void changed();
    /// Something went wrong that the window should say.
    void failed();
    /// A key was typed while the file is open read-only.
    void readOnlyTyped();

protected:
    void paintEvent(QPaintEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void resizeEvent(QResizeEvent *event) override;
    void focusInEvent(QFocusEvent *event) override;
    void focusOutEvent(QFocusEvent *event) override;

private:
    /// Where a point on the viewport lands: the byte, and which column.
    bool offsetAt(const QPoint &point, uint64_t *offset, int *column) const;
    void moveBy(int64_t delta, bool extend);
    void moveTo(uint64_t offset, bool extend);
    void updateScrollRange();
    int visibleRows() const;
    int offsetDigits() const;
    int hexStart() const;
    int textStart() const;
    int byteX(int index) const;

    JtfApp *m_app;
    int m_rowBytes = 16;
    int m_charWidth = 8;
    int m_lineHeight = 16;
    QColor m_text, m_dim, m_changed, m_selection, m_cursor, m_base;
};

/// The window around the grid: the mode, find and replace, and a status line
/// that says where the cursor is and what saving would change.
class HexEditorWindow : public QWidget {
    Q_OBJECT

public:
    HexEditorWindow(JtfApp *app, QWidget *parent = nullptr);
    ~HexEditorWindow() override;

    /// Show the file the bridge has just opened.
    void load();
    /// Let go of the file, asking about unsaved changes first. False means the
    /// person chose to keep it - Cancel - and nothing was released.
    bool release();

signals:
    /// The file was written, so the panes can show its new size and date.
    void saved(const QString &path);

protected:
    void closeEvent(class QCloseEvent *event) override;

private:
    QString tr_(const char *key) const;
    void updateStatus();
    void updateTitle();
    void showError();
    void showMessage(const QString &text, bool error);
    void find(bool forward);
    void replace(bool all);
    void gotoOffset();
    bool save();
    void copyAs(int format);
    void paste();
    void setMode(int mode);
    /// Search kind, integer width and byte order as the bridge numbers them.
    int kind() const;
    int width() const;
    bool littleEndian() const;
    void showContextMenu(const QPoint &global);

    JtfApp *m_app;
    HexView *m_view = nullptr;
    QComboBox *m_mode = nullptr;
    QLineEdit *m_find = nullptr;
    QComboBox *m_kind = nullptr;
    QComboBox *m_width = nullptr;
    QCheckBox *m_bigEndian = nullptr;
    QLineEdit *m_replace = nullptr;
    QPushButton *m_replaceOne = nullptr;
    QPushButton *m_replaceAll = nullptr;
    QLabel *m_message = nullptr;
    QLabel *m_position = nullptr;
    QLabel *m_changes = nullptr;
    QString m_path;
    bool m_writable = true;
    QColor m_errorColour;
    QColor m_dimColour;
};
