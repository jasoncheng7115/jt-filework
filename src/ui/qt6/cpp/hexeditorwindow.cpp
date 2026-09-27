#include "hexeditorwindow.h"

#include "icons.h"
#include "jtfstring.h"
#include "theme.h"

#include <QAction>
#include <QApplication>
#include <QCheckBox>
#include <QClipboard>
#include <QCloseEvent>
#include <QComboBox>
#include <QFileInfo>
#include <QFontDatabase>
#include <QHBoxLayout>
#include <QInputDialog>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QMessageBox>
#include <QPainter>
#include <QPushButton>
#include <QScrollBar>
#include <QSignalBlocker>
#include <QToolButton>
#include <QVBoxLayout>

#include <QLocale>
#include <QMouseEvent>

#include <algorithm>
#include <functional>
#include <limits>
#include <vector>

namespace {

/// Whether a byte is drawn as itself in the text column. Printable ASCII only:
/// anything else as a character is a guess about an encoding the bytes may not
/// be in, and a guess drawn in a column of facts reads as one.
bool printable(uint8_t byte) { return byte >= 0x20 && byte < 0x7f; }

QString hexText(uint64_t value, int digits) {
    return QStringLiteral("%1").arg(value, digits, 16, QLatin1Char('0')).toUpper();
}

} // namespace

// -------------------------------------------------------------------- HexView

HexView::HexView(JtfApp *app, QWidget *parent) : QAbstractScrollArea(parent), m_app(app) {
    // Monospace, because a hex dump is a grid and only a fixed-width face
    // keeps its columns under each other.
    setFont(QFontDatabase::systemFont(QFontDatabase::FixedFont));
    setFocusPolicy(Qt::StrongFocus);
    setFrameShape(QFrame::NoFrame);
    viewport()->setCursor(Qt::IBeamCursor);
    m_rowBytes = jtf_hex_row_bytes();
    const QFontMetrics metrics(font());
    m_charWidth = metrics.horizontalAdvance(QLatin1Char('0'));
    m_lineHeight = metrics.height() + 4;
    verticalScrollBar()->setSingleStep(1);
}

void HexView::setColours(const QColor &text, const QColor &dim, const QColor &changed,
                         const QColor &selection, const QColor &cursor, const QColor &base) {
    m_text = text;
    m_dim = dim;
    m_changed = changed;
    m_selection = selection;
    m_cursor = cursor;
    m_base = base;
    viewport()->update();
}

int HexView::offsetDigits() const {
    // Eight digits reach four gigabytes; a file past that needs sixteen, and
    // an offset with its top half cut off is a different offset.
    return jtf_hex_len(m_app) > 0xffffffffULL ? 16 : 8;
}

int HexView::hexStart() const { return m_charWidth + (offsetDigits() + 2) * m_charWidth; }

int HexView::byteX(int index) const {
    // A gap after the eighth byte, so the eye can count to sixteen in halves.
    return hexStart() + (index * 3 + (index >= m_rowBytes / 2 ? 1 : 0)) * m_charWidth;
}

int HexView::textStart() const { return hexStart() + (m_rowBytes * 3 + 2) * m_charWidth; }

QSize HexView::sizeHint() const {
    const int width = textStart() + (m_rowBytes + 1) * m_charWidth +
                      verticalScrollBar()->sizeHint().width();
    return {width, m_lineHeight * 26};
}

int HexView::visibleRows() const { return std::max(1, viewport()->height() / m_lineHeight); }

void HexView::updateScrollRange() {
    const uint64_t rows = jtf_hex_row_count(m_app);
    // A scroll bar counts in int. Past that the last rows are out of reach of
    // the bar - sixteen bytes a row puts the limit at thirty-two gigabytes -
    // and the cursor keys and Go To still reach them.
    const uint64_t limit = static_cast<uint64_t>(std::numeric_limits<int>::max());
    const int total = static_cast<int>(std::min(rows, limit));
    verticalScrollBar()->setRange(0, std::max(0, total - visibleRows()));
    verticalScrollBar()->setPageStep(visibleRows());
}

void HexView::refresh() {
    updateScrollRange();
    viewport()->update();
}

void HexView::resizeEvent(QResizeEvent *event) {
    QAbstractScrollArea::resizeEvent(event);
    updateScrollRange();
}

void HexView::focusInEvent(QFocusEvent *event) {
    QAbstractScrollArea::focusInEvent(event);
    viewport()->update();
}

void HexView::focusOutEvent(QFocusEvent *event) {
    QAbstractScrollArea::focusOutEvent(event);
    viewport()->update();
}

void HexView::ensureCursorVisible() {
    const int row = static_cast<int>(std::min<uint64_t>(
        jtf_hex_cursor(m_app) / static_cast<uint64_t>(m_rowBytes),
        static_cast<uint64_t>(std::numeric_limits<int>::max())));
    QScrollBar *bar = verticalScrollBar();
    if (row < bar->value()) {
        bar->setValue(row);
    } else if (row >= bar->value() + visibleRows()) {
        bar->setValue(row - visibleRows() + 1);
    }
}

void HexView::paintEvent(QPaintEvent *) {
    QPainter painter(viewport());
    painter.fillRect(viewport()->rect(), m_base);
    painter.setFont(font());

    const uint64_t rows = jtf_hex_row_count(m_app);
    const uint64_t len = jtf_hex_len(m_app);
    const uint64_t cursor = jtf_hex_cursor(m_app);
    uint64_t selStart = 0;
    uint64_t selEnd = 0;
    const bool selected = jtf_hex_selection(m_app, &selStart, &selEnd) != 0;
    const int column = jtf_hex_column(m_app);
    const int pending = jtf_hex_pending_nibble(m_app);
    const int digits = offsetDigits();
    const int ascent = QFontMetrics(font()).ascent() + 2;
    const bool focused = hasFocus();

    std::vector<uint8_t> values(static_cast<size_t>(m_rowBytes));
    std::vector<uint8_t> modified(static_cast<size_t>(m_rowBytes));

    const uint64_t first = static_cast<uint64_t>(verticalScrollBar()->value());
    const int count = visibleRows() + 1;
    for (int i = 0; i < count; ++i) {
        const uint64_t row = first + static_cast<uint64_t>(i);
        if (row >= rows) {
            break;
        }
        const int y = i * m_lineHeight;
        const uint64_t base = row * static_cast<uint64_t>(m_rowBytes);
        const int n = std::max(0, jtf_hex_row(m_app, row, values.data(), modified.data(),
                                              m_rowBytes));

        painter.setPen(m_dim);
        painter.drawText(m_charWidth, y + ascent, hexText(base, digits));

        for (int b = 0; b < n; ++b) {
            const uint64_t at = base + static_cast<uint64_t>(b);
            const QRect hexCell(byteX(b), y, m_charWidth * 2, m_lineHeight);
            const QRect textCell(textStart() + b * m_charWidth, y, m_charWidth, m_lineHeight);
            if (selected && at >= selStart && at < selEnd) {
                painter.fillRect(hexCell.adjusted(-m_charWidth / 2, 0, m_charWidth / 2, 0),
                                 m_selection);
                painter.fillRect(textCell, m_selection);
            }
            // A changed byte in the mark colour, in both columns: the one
            // question an editor has to answer at a glance is what is about to
            // be written.
            const QColor ink = modified[static_cast<size_t>(b)] != 0 ? m_changed : m_text;
            painter.setPen(ink);
            painter.drawText(hexCell.x(), y + ascent,
                             hexText(values[static_cast<size_t>(b)], 2));
            const uint8_t value = values[static_cast<size_t>(b)];
            painter.setPen(printable(value) ? ink : m_dim);
            painter.drawText(textCell.x(), y + ascent,
                             printable(value) ? QString(QLatin1Char(static_cast<char>(value)))
                                              : QStringLiteral("."));
        }

        // The cursor, which may stand one past the last byte - where an insert
        // appends - and so is drawn whether or not there is a byte under it.
        if (cursor >= base && cursor < base + static_cast<uint64_t>(m_rowBytes) &&
            cursor <= len) {
            const int b = static_cast<int>(cursor - base);
            const QRect hexCell(byteX(b) - 1, y + 1, m_charWidth * 2 + 2, m_lineHeight - 2);
            const QRect textCell(textStart() + b * m_charWidth - 1, y + 1, m_charWidth + 2,
                                 m_lineHeight - 2);
            if (pending >= 0 && column == 0) {
                // Half a byte typed: the first digit in the cell it will land
                // in, and a mark where the second goes.
                painter.fillRect(hexCell, m_base);
                painter.setPen(m_changed);
                painter.drawText(byteX(b), y + ascent,
                                 hexText(static_cast<uint64_t>(pending), 1) +
                                     QLatin1Char('_'));
            }
            QColor strong = m_cursor;
            if (!focused) {
                strong.setAlphaF(0.55);
            }
            QColor faint = m_cursor;
            faint.setAlphaF(0.35);
            painter.setPen(QPen(column == 0 ? strong : faint, column == 0 ? 2 : 1));
            painter.drawRect(hexCell);
            painter.setPen(QPen(column == 1 ? strong : faint, column == 1 ? 2 : 1));
            painter.drawRect(textCell);
        }
    }
}

bool HexView::offsetAt(const QPoint &point, uint64_t *offset, int *column) const {
    const uint64_t row = static_cast<uint64_t>(verticalScrollBar()->value()) +
                         static_cast<uint64_t>(std::max(0, point.y()) / m_lineHeight);
    int index = -1;
    int where = 0;
    if (point.x() >= textStart() - m_charWidth) {
        where = 1;
        index = (point.x() - textStart()) / m_charWidth;
    } else {
        for (int b = 0; b < m_rowBytes; ++b) {
            if (point.x() < byteX(b) + m_charWidth * 5 / 2) {
                index = b;
                break;
            }
        }
        if (index < 0) {
            index = m_rowBytes - 1;
        }
    }
    index = std::clamp(index, 0, m_rowBytes - 1);
    const uint64_t at = row * static_cast<uint64_t>(m_rowBytes) + static_cast<uint64_t>(index);
    *offset = std::min(at, jtf_hex_len(m_app));
    *column = where;
    return true;
}

void HexView::moveTo(uint64_t offset, bool extend) {
    jtf_hex_move_to(m_app, std::min(offset, jtf_hex_len(m_app)), extend ? 1 : 0);
    ensureCursorVisible();
    viewport()->update();
    emit changed();
}

void HexView::moveBy(int64_t delta, bool extend) {
    const uint64_t cursor = jtf_hex_cursor(m_app);
    uint64_t target = cursor;
    if (delta < 0) {
        const auto back = static_cast<uint64_t>(-delta);
        target = back > cursor ? 0 : cursor - back;
    } else {
        target = cursor + static_cast<uint64_t>(delta);
    }
    moveTo(target, extend);
}

void HexView::mousePressEvent(QMouseEvent *event) {
    if (event->button() != Qt::LeftButton) {
        QAbstractScrollArea::mousePressEvent(event);
        return;
    }
    uint64_t offset = 0;
    int column = 0;
    if (offsetAt(event->position().toPoint(), &offset, &column)) {
        jtf_hex_set_column(m_app, column);
        moveTo(offset, event->modifiers().testFlag(Qt::ShiftModifier));
    }
}

void HexView::mouseMoveEvent(QMouseEvent *event) {
    if (!event->buttons().testFlag(Qt::LeftButton)) {
        return;
    }
    uint64_t offset = 0;
    int column = 0;
    if (offsetAt(event->position().toPoint(), &offset, &column)) {
        moveTo(offset, true);
    }
}

void HexView::keyPressEvent(QKeyEvent *event) {
    const bool extend = event->modifiers().testFlag(Qt::ShiftModifier);
    const bool chord = (event->modifiers() & (Qt::ControlModifier | Qt::MetaModifier)) != 0;
    const int64_t row = m_rowBytes;
    const int64_t page = row * visibleRows();
    const uint64_t cursor = jtf_hex_cursor(m_app);
    const uint64_t len = jtf_hex_len(m_app);

    switch (event->key()) {
    case Qt::Key_Left:
        moveBy(-1, extend);
        return;
    case Qt::Key_Right:
        moveBy(1, extend);
        return;
    case Qt::Key_Up:
        moveBy(-row, extend);
        return;
    case Qt::Key_Down:
        moveBy(row, extend);
        return;
    case Qt::Key_PageUp:
        moveBy(-page, extend);
        return;
    case Qt::Key_PageDown:
        moveBy(page, extend);
        return;
    case Qt::Key_Home:
        moveTo(chord ? 0 : cursor - cursor % static_cast<uint64_t>(row), extend);
        return;
    case Qt::Key_End: {
        const uint64_t rowEnd = cursor - cursor % static_cast<uint64_t>(row) +
                                static_cast<uint64_t>(row) - 1;
        moveTo(chord ? len : std::min(rowEnd, len), extend);
        return;
    }
    case Qt::Key_Tab:
    case Qt::Key_Backtab:
        // Between the two columns: which one typing goes into.
        jtf_hex_set_column(m_app, jtf_hex_column(m_app) == 0 ? 1 : 0);
        viewport()->update();
        emit changed();
        return;
    case Qt::Key_Insert: {
        // Overwrite and insert, the way every editor with an Insert key
        // swaps them. Not out of read-only: that is a choice made on purpose,
        // in the mode box, not by a key that is easy to brush.
        const int mode = jtf_hex_mode(m_app);
        if (mode != 0) {
            jtf_hex_set_mode(m_app, mode == 1 ? 2 : 1);
            viewport()->update();
            emit changed();
        }
        return;
    }
    case Qt::Key_Backspace:
    case Qt::Key_Delete:
        if (jtf_hex_delete(m_app, event->key() == Qt::Key_Delete ? 1 : 0) < 0) {
            emit failed();
        }
        refresh();
        ensureCursorVisible();
        emit changed();
        return;
    case Qt::Key_Escape:
        // Collapse the selection to the cursor.
        moveTo(cursor, false);
        return;
    default:
        break;
    }

    const QString text = event->text();
    if (chord || text.isEmpty() || !text.at(0).isPrint()) {
        QAbstractScrollArea::keyPressEvent(event);
        return;
    }
    if (jtf_hex_mode(m_app) == 0) {
        // Typing into a read-only file is the one mistake this window exists
        // to make harmless. Said, not ignored, so the person knows where the
        // switch is.
        emit readOnlyTyped();
        return;
    }
    const bool hexColumn = jtf_hex_column(m_app) == 0;
    for (const QChar ch : text) {
        int result = 0;
        if (hexColumn) {
            result = jtf_hex_type_hex_digit(m_app, ch.unicode());
        } else if (ch.unicode() < 0x100) {
            // One byte per character, Latin-1: the text column is bytes, and a
            // character that needs more than one byte is a paste, not a key.
            result = jtf_hex_type_byte(m_app, static_cast<uint8_t>(ch.unicode()));
        }
        if (result < 0) {
            emit failed();
            break;
        }
    }
    refresh();
    ensureCursorVisible();
    emit changed();
}

// ------------------------------------------------------------ HexEditorWindow

QString HexEditorWindow::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

HexEditorWindow::HexEditorWindow(JtfApp *app, QWidget *parent)
    : QWidget(parent, Qt::Window), m_app(app) {
    const bool dark = palette().color(QPalette::Window).lightness() < 128;
    const Theme theme = Theme::fromApp(m_app, dark);
    m_errorColour = theme.error;
    m_dimColour = theme.textSecondary;

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    // ------------------------------------------------------------ the bar
    auto *bar = new QWidget(this);
    bar->setObjectName(QStringLiteral("JtfViewerBar"));
    auto *rows = new QVBoxLayout(bar);
    rows->setContentsMargins(8, 6, 8, 6);
    rows->setSpacing(6);
    auto *top = new QHBoxLayout;
    top->setSpacing(8);
    auto *second = new QHBoxLayout;
    second->setSpacing(8);
    rows->addLayout(top);
    rows->addLayout(second);

    // The mode first and always visible: it is what decides whether a
    // keystroke changes the file.
    m_mode = new QComboBox(bar);
    m_mode->addItem(tr_("hex.mode.readonly"));
    m_mode->addItem(tr_("hex.mode.overwrite"));
    m_mode->addItem(tr_("hex.mode.insert"));
    m_mode->setToolTip(tr_("hex.mode.tooltip"));
    connect(m_mode, &QComboBox::currentIndexChanged, this, [this](int mode) { setMode(mode); });
    top->addWidget(m_mode);

    m_find = new QLineEdit(bar);
    m_find->setPlaceholderText(tr_("hex.find.placeholder"));
    m_find->setClearButtonEnabled(true);
    connect(m_find, &QLineEdit::returnPressed, this, [this] { find(true); });
    top->addWidget(m_find, 1);

    m_kind = new QComboBox(bar);
    for (const char *key : {"hex.kind.hex", "hex.kind.utf8", "hex.kind.latin1",
                            "hex.kind.utf16le", "hex.kind.utf16be", "hex.kind.integer"}) {
        m_kind->addItem(tr_(key));
    }
    top->addWidget(m_kind);

    m_width = new QComboBox(bar);
    for (const int bits : {8, 16, 32, 64}) {
        m_width->addItem(jtfFill(tr_("hex.kind.bits"), "bits", QString::number(bits)), bits / 8);
    }
    m_width->setCurrentIndex(2);
    top->addWidget(m_width);
    m_bigEndian = new QCheckBox(tr_("hex.kind.big_endian"), bar);
    top->addWidget(m_bigEndian);
    const auto syncKind = [this] {
        // Width and byte order mean something only for an integer.
        const bool integer = m_kind->currentIndex() == 5;
        m_width->setVisible(integer);
        m_bigEndian->setVisible(integer);
    };
    connect(m_kind, &QComboBox::currentIndexChanged, this, syncKind);
    syncKind();

    const QColor ink = palette().color(QPalette::WindowText);
    auto *previous = new QToolButton(bar);
    previous->setIcon(glyph::forCommand(QStringLiteral("nav.up"), ink));
    previous->setToolTip(tr_("hex.find.previous"));
    previous->setAutoRaise(true);
    connect(previous, &QToolButton::clicked, this, [this] { find(false); });
    top->addWidget(previous);
    auto *next = new QToolButton(bar);
    next->setIcon(glyph::make(glyph::Shape::ArrowDown, ink));
    next->setToolTip(tr_("hex.find.next"));
    next->setAutoRaise(true);
    connect(next, &QToolButton::clicked, this, [this] { find(true); });
    top->addWidget(next);

    m_replace = new QLineEdit(bar);
    m_replace->setPlaceholderText(tr_("hex.replace.placeholder"));
    m_replace->setClearButtonEnabled(true);
    second->addWidget(m_replace, 1);
    m_replaceOne = new QPushButton(tr_("hex.replace.one"), bar);
    connect(m_replaceOne, &QPushButton::clicked, this, [this] { replace(false); });
    second->addWidget(m_replaceOne);
    m_replaceAll = new QPushButton(tr_("hex.replace.all"), bar);
    connect(m_replaceAll, &QPushButton::clicked, this, [this] { replace(true); });
    second->addWidget(m_replaceAll);
    layout->addWidget(bar);

    // ------------------------------------------------------------ the grid
    m_view = new HexView(app, this);
    m_view->setColours(theme.textPrimary, theme.textSecondary, theme.mark,
                       theme.selectionInactive, theme.focusRing, theme.pane);
    m_view->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(m_view, &QWidget::customContextMenuRequested, this,
            [this](const QPoint &at) { showContextMenu(m_view->mapToGlobal(at)); });
    connect(m_view, &HexView::changed, this, [this] { updateStatus(); });
    connect(m_view, &HexView::failed, this, [this] { showError(); });
    connect(m_view, &HexView::readOnlyTyped, this,
            [this] { showMessage(tr_("hex.readonly_typed"), true); });
    layout->addWidget(m_view, 1);

    // ------------------------------------------------------------ the foot
    auto *hints = new QLabel(this);
    hints->setObjectName(QStringLiteral("JtfViewerHints"));
    hints->setTextFormat(Qt::PlainText);
    hints->setContentsMargins(10, 5, 10, 5);
    hints->setText(jtfFill(
        jtfFill(tr_("hex.hints"), "save",
                QKeySequence(QKeySequence::Save).toString(QKeySequence::NativeText)),
        "goto", QKeySequence(Qt::CTRL | Qt::Key_L).toString(QKeySequence::NativeText)));
    layout->addWidget(hints);

    auto *foot = new QWidget(this);
    foot->setObjectName(QStringLiteral("JtfViewerFoot"));
    auto *footRow = new QHBoxLayout(foot);
    footRow->setContentsMargins(10, 4, 10, 4);
    footRow->setSpacing(16);
    m_message = new QLabel(foot);
    m_message->setTextFormat(Qt::PlainText);
    footRow->addWidget(m_message, 1);
    m_changes = new QLabel(foot);
    m_changes->setTextFormat(Qt::PlainText);
    footRow->addWidget(m_changes);
    m_position = new QLabel(foot);
    m_position->setTextFormat(Qt::PlainText);
    footRow->addWidget(m_position);
    layout->addWidget(foot);

    // ------------------------------------------------------------ actions
    // On the window, so they work wherever the focus is - except in the two
    // text fields, which keep the editing chords for their own text.
    const auto action = [this](const QKeySequence &keys, std::function<void()> run) {
        auto *a = new QAction(this);
        a->setShortcut(keys);
        a->setShortcutContext(Qt::WindowShortcut);
        connect(a, &QAction::triggered, this, std::move(run));
        addAction(a);
        return a;
    };
    action(QKeySequence::Save, [this] { save(); });
    action(QKeySequence::Undo, [this] {
        jtf_hex_undo(m_app);
        m_view->refresh();
        m_view->ensureCursorVisible();
        updateStatus();
    });
    action(QKeySequence::Redo, [this] {
        jtf_hex_redo(m_app);
        m_view->refresh();
        m_view->ensureCursorVisible();
        updateStatus();
    });
    action(QKeySequence::Find, [this] {
        m_find->setFocus();
        m_find->selectAll();
    });
    action(QKeySequence::FindNext, [this] { find(true); });
    action(QKeySequence::FindPrevious, [this] { find(false); });
    // Go to an offset. Cmd/Ctrl-G is Find Next on macOS, so the chord is the
    // one Hex Fiend uses for the same thing: L, for location.
    action(QKeySequence(Qt::CTRL | Qt::Key_L), [this] { gotoOffset(); });
    action(QKeySequence::Copy, [this] { copyAs(jtf_hex_column(m_app) == 0 ? 2 : 0); });
    action(QKeySequence::Paste, [this] { paste(); });
    action(QKeySequence::SelectAll, [this] {
        jtf_hex_select_all(m_app);
        m_view->refresh();
        updateStatus();
    });
    action(QKeySequence::Close, [this] { close(); });

    resize(m_view->sizeHint().width() + 24, 640);
}

// Not closed here: a destructor runs after the event that deleted the window,
// by which time the next file may already be open in the bridge, and closing
// it then would close that one. `closeEvent` closes the session instead.
HexEditorWindow::~HexEditorWindow() = default;

void HexEditorWindow::load() {
    m_path = jtfText([&](char *buf, int len) { return jtf_hex_path(m_app, buf, len); });
    // Checked at the start rather than discovered at save: an editor that lets
    // someone make twenty changes to a file it cannot write has wasted them.
    m_writable = QFileInfo(m_path).isWritable();
    {
        const QSignalBlocker blocker(m_mode);
        m_mode->setCurrentIndex(0);
    }
    m_mode->setEnabled(m_writable);
    m_replace->setEnabled(m_writable);
    m_replaceOne->setEnabled(m_writable);
    m_replaceAll->setEnabled(m_writable);
    showMessage(m_writable ? QString() : tr_("hex.not_writable"), !m_writable);
    m_view->refresh();
    m_view->verticalScrollBar()->setValue(0);
    m_view->setFocus();
    updateStatus();
}

void HexEditorWindow::showMessage(const QString &text, bool error) {
    m_message->setText(text);
    m_message->setStyleSheet(
        QStringLiteral("color: %1;").arg((error ? m_errorColour : m_dimColour).name()));
}

void HexEditorWindow::showError() {
    const QString text =
        jtfText([&](char *buf, int len) { return jtf_hex_take_error(m_app, buf, len); });
    if (!text.isEmpty()) {
        showMessage(text, true);
    }
}

void HexEditorWindow::updateTitle() {
    uint64_t changed = 0;
    const bool modified = jtf_hex_summary(m_app, &changed, nullptr, nullptr) != 0;
    // A dot for unsaved changes, where every editor puts one.
    setWindowTitle(QStringLiteral("%1%2 — %3")
                       .arg(QFileInfo(m_path).fileName(),
                            modified ? QStringLiteral(" •") : QString(), tr_("hex.title")));
    setWindowModified(modified);
}

void HexEditorWindow::updateStatus() {
    {
        const QSignalBlocker blocker(m_mode);
        m_mode->setCurrentIndex(jtf_hex_mode(m_app));
    }
    const uint64_t cursor = jtf_hex_cursor(m_app);
    const QLocale locale;
    QString position = jtfFill(
        jtfFill(tr_("hex.status.offset"), "hex", QStringLiteral("0x") + hexText(cursor, 1)),
        "decimal", locale.toString(static_cast<qulonglong>(cursor)));
    uint64_t start = 0;
    uint64_t end = 0;
    if (jtf_hex_selection(m_app, &start, &end) != 0) {
        position = jtfFill(tr_("hex.status.selection"), "count",
                           locale.toString(static_cast<qulonglong>(end - start))) +
                   QStringLiteral("   ") + position;
    }
    m_position->setText(position);

    uint64_t changed = 0;
    uint64_t before = 0;
    uint64_t after = 0;
    const bool modified = jtf_hex_summary(m_app, &changed, &before, &after) != 0;
    QString changes;
    if (modified) {
        changes = jtfFill(tr_("hex.status.changed"), "count",
                          locale.toString(static_cast<qulonglong>(changed)));
        if (before != after) {
            changes += QStringLiteral("   ") +
                       jtfFill(jtfFill(tr_("hex.status.resized"), "before",
                                       locale.toString(static_cast<qulonglong>(before))),
                               "after", locale.toString(static_cast<qulonglong>(after)));
        }
    } else {
        changes = jtfFill(tr_("hex.status.size"), "size",
                          locale.toString(static_cast<qulonglong>(after)));
    }
    m_changes->setText(changes);
    updateTitle();
}

void HexEditorWindow::setMode(int mode) {
    if (!m_writable && mode != 0) {
        const QSignalBlocker blocker(m_mode);
        m_mode->setCurrentIndex(0);
        showMessage(tr_("hex.not_writable"), true);
        return;
    }
    jtf_hex_set_mode(m_app, mode);
    showMessage(QString(), false);
    m_view->refresh();
    m_view->setFocus();
    updateStatus();
}

int HexEditorWindow::kind() const { return m_kind->currentIndex(); }

int HexEditorWindow::width() const { return m_width->currentData().toInt(); }

bool HexEditorWindow::littleEndian() const { return !m_bigEndian->isChecked(); }

void HexEditorWindow::find(bool forward) {
    const QByteArray text = m_find->text().toUtf8();
    if (text.isEmpty()) {
        m_find->setFocus();
        return;
    }
    if (jtf_hex_find(m_app, text.constData(), kind(), width(), littleEndian() ? 1 : 0,
                     forward ? 1 : 0) != 0) {
        showMessage(QString(), false);
    } else {
        const QString error =
            jtfText([&](char *buf, int len) { return jtf_hex_take_error(m_app, buf, len); });
        showMessage(error.isEmpty() ? tr_("hex.not_found") : error, true);
    }
    m_view->refresh();
    m_view->ensureCursorVisible();
    updateStatus();
}

void HexEditorWindow::replace(bool all) {
    const QByteArray find = m_find->text().toUtf8();
    const QByteArray with = m_replace->text().toUtf8();
    if (find.isEmpty()) {
        m_find->setFocus();
        return;
    }
    if (all) {
        const uint64_t count = jtf_hex_replace_all(m_app, find.constData(), with.constData(),
                                                   kind(), width(), littleEndian() ? 1 : 0);
        const QString error =
            jtfText([&](char *buf, int len) { return jtf_hex_take_error(m_app, buf, len); });
        if (!error.isEmpty()) {
            showMessage(error, true);
        } else {
            showMessage(jtfFill(tr_("hex.replaced"), "count",
                                QLocale().toString(static_cast<qulonglong>(count))),
                        false);
        }
    } else if (jtf_hex_replace(m_app, find.constData(), with.constData(), kind(), width(),
                               littleEndian() ? 1 : 0) == 0) {
        const QString error =
            jtfText([&](char *buf, int len) { return jtf_hex_take_error(m_app, buf, len); });
        showMessage(error.isEmpty() ? tr_("hex.not_found") : error, !error.isEmpty());
    } else {
        showMessage(QString(), false);
    }
    m_view->refresh();
    m_view->ensureCursorVisible();
    updateStatus();
}

void HexEditorWindow::gotoOffset() {
    bool accepted = false;
    const QString text = QInputDialog::getText(this, tr_("hex.goto.title"),
                                               tr_("hex.goto.prompt"), QLineEdit::Normal,
                                               QString(), &accepted);
    if (!accepted || text.trimmed().isEmpty()) {
        return;
    }
    const QByteArray utf8 = text.trimmed().toUtf8();
    if (jtf_hex_goto(m_app, utf8.constData(), 0) == 0) {
        showError();
    } else {
        showMessage(QString(), false);
    }
    m_view->refresh();
    m_view->ensureCursorVisible();
    updateStatus();
}

bool HexEditorWindow::save() {
    uint64_t changed = 0;
    uint64_t before = 0;
    uint64_t after = 0;
    if (jtf_hex_summary(m_app, &changed, &before, &after) == 0) {
        return true; // nothing to write
    }
    // What is about to happen, before it does: how many bytes, and whether the
    // file changes length - which moves everything after the change, and for a
    // file with offsets inside it is the difference between an edit and a
    // broken file.
    const QLocale locale;
    QString detail = jtfFill(tr_("hex.save.detail"), "count",
                             locale.toString(static_cast<qulonglong>(changed)));
    if (before != after) {
        detail += QStringLiteral("\n\n") +
                  jtfFill(jtfFill(tr_("hex.save.resized"), "before",
                                  locale.toString(static_cast<qulonglong>(before))),
                          "after", locale.toString(static_cast<qulonglong>(after)));
    }
    QMessageBox box(this);
    box.setIcon(QMessageBox::Question);
    box.setWindowTitle(tr_("hex.save.title"));
    box.setText(jtfFill(tr_("hex.save.question"), "name", QFileInfo(m_path).fileName()));
    box.setInformativeText(detail);
    QPushButton *write = box.addButton(tr_("hex.save.confirm"), QMessageBox::AcceptRole);
    QPushButton *cancel = box.addButton(tr_("hex.save.cancel"), QMessageBox::RejectRole);
    box.setDefaultButton(write);
    box.setEscapeButton(cancel);
    box.exec();
    if (box.clickedButton() != write) {
        return false;
    }
    if (jtf_hex_save(m_app) == 0) {
        showError();
        return false;
    }
    showMessage(tr_("hex.saved"), false);
    m_view->refresh();
    updateStatus();
    emit saved(m_path);
    return true;
}

void HexEditorWindow::copyAs(int format) {
    uint64_t start = 0;
    uint64_t end = 0;
    if (jtf_hex_selection(m_app, &start, &end) == 0) {
        showMessage(tr_("hex.copy.nothing"), false);
        return;
    }
    const QString text =
        jtfText([&](char *buf, int len) { return jtf_hex_copy_as(m_app, format, buf, len); });
    if (text.isEmpty()) {
        showError();
        return;
    }
    QApplication::clipboard()->setText(text);
    showMessage(jtfFill(tr_("hex.copied"), "count",
                        QLocale().toString(static_cast<qulonglong>(end - start))),
                false);
}

void HexEditorWindow::paste() {
    const QByteArray text = QApplication::clipboard()->text().toUtf8();
    if (text.isEmpty()) {
        return;
    }
    if (jtf_hex_paste(m_app, text.constData()) == 0) {
        showError();
    } else {
        // Said, because a paste is a guess about what the text is: hex digits,
        // an array literal, Base64 or plain text, and the guess should be
        // visible in case it was the wrong one.
        const QString key = jtfText(
            [&](char *buf, int len) { return jtf_hex_take_paste_kind(m_app, buf, len); });
        const QByteArray keyUtf8 = key.toUtf8();
        showMessage(key.isEmpty() ? QString() : tr_(keyUtf8.constData()), false);
    }
    m_view->refresh();
    m_view->ensureCursorVisible();
    updateStatus();
}

void HexEditorWindow::showContextMenu(const QPoint &global) {
    QMenu menu(this);
    QMenu *copy = menu.addMenu(tr_("hex.copy.as"));
    const char *const formats[] = {"hex.copy.raw",        "hex.copy.hex_string",
                                   "hex.copy.hex_spaced", "hex.copy.c_array",
                                   "hex.copy.rust_array", "hex.copy.python_bytes",
                                   "hex.copy.base64"};
    for (int i = 0; i < 7; ++i) {
        copy->addAction(tr_(formats[i]), this, [this, i] { copyAs(i); });
    }
    copy->setEnabled(jtf_hex_selection(m_app, nullptr, nullptr) != 0);
    QAction *pasteAction = menu.addAction(tr_("hex.paste"), this, [this] { paste(); });
    pasteAction->setEnabled(jtf_hex_mode(m_app) != 0);
    menu.addSeparator();
    QAction *undo = menu.addAction(tr_("hex.undo"), this, [this] {
        jtf_hex_undo(m_app);
        m_view->refresh();
        updateStatus();
    });
    undo->setEnabled(jtf_hex_can_undo(m_app) != 0);
    QAction *redo = menu.addAction(tr_("hex.redo"), this, [this] {
        jtf_hex_redo(m_app);
        m_view->refresh();
        updateStatus();
    });
    redo->setEnabled(jtf_hex_can_redo(m_app) != 0);
    menu.addSeparator();
    menu.addAction(tr_("hex.goto.title"), this, [this] { gotoOffset(); });
    menu.addAction(tr_("hex.select_all"), this, [this] {
        jtf_hex_select_all(m_app);
        m_view->refresh();
        updateStatus();
    });
    menu.exec(global);
}

void HexEditorWindow::closeEvent(QCloseEvent *event) {
    if (!release()) {
        event->ignore();
        return;
    }
    jtf_hex_close(m_app);
    event->accept();
}

bool HexEditorWindow::release() {
    uint64_t changed = 0;
    if (jtf_hex_summary(m_app, &changed, nullptr, nullptr) == 0) {
        return true;
    }
    // Unsaved changes are asked about, never dropped: closing the window is
    // not the same decision as throwing the edits away.
    QMessageBox box(this);
    box.setIcon(QMessageBox::Warning);
    box.setWindowTitle(tr_("hex.unsaved.title"));
    box.setText(jtfFill(tr_("hex.unsaved.question"), "name", QFileInfo(m_path).fileName()));
    box.setInformativeText(jtfFill(tr_("hex.unsaved.detail"), "count",
                                   QLocale().toString(static_cast<qulonglong>(changed))));
    QPushButton *saveButton = box.addButton(tr_("hex.unsaved.save"), QMessageBox::AcceptRole);
    QPushButton *discard =
        box.addButton(tr_("hex.unsaved.discard"), QMessageBox::DestructiveRole);
    QPushButton *cancel = box.addButton(tr_("hex.save.cancel"), QMessageBox::RejectRole);
    box.setDefaultButton(saveButton);
    box.setEscapeButton(cancel);
    box.exec();
    if (box.clickedButton() == discard) {
        return true;
    }
    return box.clickedButton() == saveButton && save();
}
