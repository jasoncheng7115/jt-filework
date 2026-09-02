#include "viewerwindow.h"

#include "matchdelegate.h"
#include "icons.h"
#include "jtfstring.h"

#include <QCheckBox>
#include <QCloseEvent>
#include <QComboBox>
#include <QFontDatabase>
#include <QSlider>
#include <QHBoxLayout>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QAction>
#include <QListView>
#include <QScreen>
#include <QStyle>
#include <QVBoxLayout>
#include <limits>

// ------------------------------------------------------------- ViewerModel

ViewerModel::ViewerModel(JtfApp *app, QObject *parent)
    : QAbstractListModel(parent), m_app(app) {}

int ViewerModel::rowCount(const QModelIndex &parent) const {
    return parent.isValid() ? 0 : m_rows;
}

QVariant ViewerModel::data(const QModelIndex &index, int role) const {
    if (!index.isValid() || role != Qt::DisplayRole) {
        return {};
    }
    // One row, fetched as it is painted. The view only paints what is visible,
    // so this is the whole of the memory cost.
    return jtfText([&](char *buf, int len) {
        return jtf_viewer_row(m_app, static_cast<uint64_t>(index.row()), buf, len);
    });
}

void ViewerModel::reload() {
    beginResetModel();
    const uint64_t rows = jtf_viewer_row_count(m_app);
    // Qt indexes rows with int. A file with more lines than that is beyond
    // what this view can address, and clamping is honest about it rather than
    // wrapping into nonsense.
    m_rows = rows > static_cast<uint64_t>(std::numeric_limits<int>::max())
                 ? std::numeric_limits<int>::max()
                 : static_cast<int>(rows);
    endResetModel();
}

// ------------------------------------------------------------ ViewerWindow

QString ViewerWindow::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

QString ViewerWindow::trKey(const QString &key) const {
    if (key.isEmpty()) {
        return {};
    }
    const QByteArray utf8 = key.toUtf8();
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, utf8.constData(), buf, len); });
}

ViewerWindow::ViewerWindow(JtfApp *app, QWidget *parent)
    : QWidget(parent, Qt::Window), m_app(app) {
    setWindowTitle(tr_("viewer.title"));
    resize(900, 640);

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    auto *bar = new QWidget(this);
    bar->setObjectName(QStringLiteral("JtfViewerBar"));
    auto *barLayout = new QHBoxLayout(bar);
    barLayout->setContentsMargins(8, 6, 8, 6);
    barLayout->setSpacing(8);

    m_encoding = new QComboBox(bar);
    for (int i = 0; i < jtf_encoding_count(); ++i) {
        m_encoding->addItem(trKey(
            jtfText([&](char *buf, int len) { return jtf_encoding_key(i, buf, len); })));
    }
    connect(m_encoding, &QComboBox::currentIndexChanged, this, [this](int index) {
        jtf_viewer_set_encoding(m_app, index);
        m_model->reload();
        updateStatus();
    });
    barLayout->addWidget(m_encoding);

    auto *hex = new QCheckBox(tr_("viewer.hex"), bar);
    connect(hex, &QCheckBox::toggled, this, [this](bool) {
        jtf_viewer_toggle_hex(m_app);
        refresh();
    });
    barLayout->addWidget(hex);

    m_find = new QLineEdit(bar);
    m_find->setPlaceholderText(tr_("viewer.find_placeholder"));
    m_find->setClearButtonEnabled(true);
    // The same magnifier the main window's search box carries. A bare box
    // beside two other controls reads as one more field; the glyph says which
    // of them is the one you type a search into.
    m_findIcon = m_find->addAction(QIcon(), QLineEdit::LeadingPosition);
    connect(m_find, &QLineEdit::returnPressed, this, &ViewerWindow::findNext);
    barLayout->addWidget(m_find, 1);
    layout->addWidget(bar);

    m_model = new ViewerModel(app, this);
    m_view = new QListView(this);
    m_view->setObjectName(QStringLiteral("JtfViewerList"));
    m_view->setModel(m_model);
    m_view->setFrameShape(QFrame::NoFrame);
    m_view->setUniformItemSizes(true); // what keeps the list virtualized
    m_view->setSelectionMode(QAbstractItemView::SingleSelection);
    m_view->setHorizontalScrollBarPolicy(Qt::ScrollBarAsNeeded);
    m_view->setWordWrap(false);
    // A viewer is always monospace: hex needs columns to line up, and so does
    // most of what people open a viewer for.
    m_view->setFont(QFontDatabase::systemFont(QFontDatabase::FixedFont));
    m_matches = new MatchDelegate(this);
    m_view->setItemDelegate(m_matches);
    layout->addWidget(m_view, 1);

    // The keys this window answers to, along its foot, the way the file list
    // has them. A viewer with no visible keys is a window you can only scroll.
    m_hints = new QWidget(this);
    m_hints->setObjectName(QStringLiteral("JtfViewerHints"));
    auto *hintRow = new QHBoxLayout(m_hints);
    hintRow->setContentsMargins(10, 5, 10, 5);
    hintRow->setSpacing(14);
    layout->addWidget(m_hints);

    auto *foot = new QWidget(this);
    foot->setObjectName(QStringLiteral("JtfViewerFoot"));
    auto *footLayout = new QHBoxLayout(foot);
    footLayout->setContentsMargins(8, 3, 8, 3);
    footLayout->setSpacing(8);
    m_status = new QLabel(foot);
    footLayout->addWidget(m_status, 1);

    // Reading a log at the list's font size is not the same as reading a
    // folder at it, so the viewer carries its own.
    auto *smaller = new QLabel(QStringLiteral("A"), foot);
    smaller->setProperty("jtfZoomMark", true);
    m_zoom = new QSlider(Qt::Horizontal, foot);
    m_zoom->setObjectName(QStringLiteral("JtfZoom"));
    m_zoom->setRange(9, 24);
    m_zoom->setFixedWidth(96);
    m_zoom->setValue(m_view->font().pointSize() > 0 ? m_view->font().pointSize() : 12);
    auto *larger = new QLabel(QStringLiteral("A"), foot);
    larger->setProperty("jtfZoomMark", true);
    QFont bigMark = larger->font();
    bigMark.setPointSizeF(bigMark.pointSizeF() * 1.25);
    larger->setFont(bigMark);
    connect(m_zoom, &QSlider::valueChanged, this, [this](int points) {
        QFont font = m_view->font();
        font.setPointSize(points);
        m_view->setFont(font);
    });
    footLayout->addWidget(smaller);
    footLayout->addWidget(m_zoom);
    footLayout->addWidget(larger);
    layout->addWidget(foot);

    refresh();

    // The keyboard belongs in the content. Opening with the focus in the find
    // box means the first arrow key or Page Down goes nowhere, and the first
    // thing anyone does in a viewer is scroll.
    if (m_model->rowCount() > 0) {
        m_view->setCurrentIndex(m_model->index(0, 0));
    }
    m_view->setFocus();
    updateHints();
    m_findIcon->setIcon(
        glyph::make(glyph::Shape::Search, palette().color(QPalette::PlaceholderText)));
}

ViewerWindow::~ViewerWindow() {
    jtf_viewer_close(m_app);
}

void ViewerWindow::refresh() {
    m_model->reload();
    m_encoding->setEnabled(jtf_viewer_is_text(m_app) != 0);
    {
        QSignalBlocker blocker(m_encoding);
        m_encoding->setCurrentIndex(jtf_viewer_encoding(m_app));
    }
    updateStatus();
    fitToContent();
}

// Narrow the window to the width of what is in it.
//
// A hex dump is a fixed width - an offset, sixteen bytes, and sixteen
// characters - and it is nowhere near 900 pixels. The window opened at its
// default and left a third of itself empty, which reads as a layout that has
// gone wrong rather than as content that happens to be narrow.
//
// Only ever narrower. A window that grew to fit would fight anyone who had
// deliberately made it small, and text can be arbitrarily wide, so growing is
// how a viewer ends up wider than the screen.
void ViewerWindow::fitToContent() {
    if (m_model == nullptr || m_view == nullptr || m_model->rowCount() == 0) {
        return;
    }
    // Hex only. Text lines vary, and the longest one in a large file is not
    // worth reading every line to find - nor is it a width anyone wants the
    // window set to. A viewer showing hex is one with no text, which is what
    // `is_text` already answers.
    if (jtf_viewer_is_text(m_app) != 0) {
        return;
    }

    const QFontMetrics metrics(m_view->font());
    // Every hex row is the same width by construction, so one is enough. The
    // first few are sampled anyway in case the last row is short.
    int widest = 0;
    const int sample = qMin(m_model->rowCount(), 8);
    for (int row = 0; row < sample; ++row) {
        const QString line = m_model->index(row, 0).data(Qt::DisplayRole).toString();
        widest = qMax(widest, metrics.horizontalAdvance(line));
    }
    if (widest <= 0) {
        return;
    }

    // The list's own padding, a scrollbar's worth, and the frame.
    const int chrome = m_view->style()->pixelMetric(QStyle::PM_ScrollBarExtent) + 46;
    const int wanted = widest + chrome;
    const int available = screen() != nullptr ? screen()->availableGeometry().width() : width();
    const int target = qBound(420, wanted, available - 40);
    if (target < width()) {
        resize(target, height());
    }
}

void ViewerWindow::updateStatus() {
    char path[4096] = {};
    char kind[64] = {};
    char encoding[64] = {};
    char endings[64] = {};
    uint64_t size = 0;
    jtf_viewer_status(m_app, path, sizeof(path), kind, sizeof(kind), encoding, sizeof(encoding),
                      endings, sizeof(endings), &size);

    QStringList parts;
    parts << trKey(QString::fromUtf8(kind));
    parts << QStringLiteral("%1 bytes").arg(size);
    if (jtf_viewer_is_text(m_app)) {
        parts << trKey(QString::fromUtf8(encoding));
        parts << trKey(QString::fromUtf8(endings));
        parts << jtfFill(tr_("viewer.rows"), "count", QString::number(m_model->rowCount()));
    }
    m_status->setText(parts.join(QStringLiteral("   ")));
    setWindowTitle(QString::fromUtf8(path));
}

void ViewerWindow::findNext() {
    const QString needle = m_find->text();
    if (needle.isEmpty()) {
        m_matches->setNeedle(QString());
        m_view->viewport()->update();
        return;
    }
    const QModelIndex current = m_view->currentIndex();
    const uint64_t from = current.isValid() ? static_cast<uint64_t>(current.row() + 1) : 0;

    const QByteArray utf8 = needle.toUtf8();
    const int64_t found = jtf_viewer_find(m_app, utf8.constData(), from);
    if (found < 0) {
        m_status->setText(tr_("viewer.not_found"));
        m_matches->setNeedle(QString());
        m_view->viewport()->update();
        return;
    }
    const QModelIndex index = m_model->index(static_cast<int>(found), 0);
    m_view->setCurrentIndex(index);
    m_view->scrollTo(index, QAbstractItemView::PositionAtCenter);
    // The line is long and the match is a few characters in it. Lighting the
    // whole row says which line; it does not say where to look.
    m_matches->setNeedle(needle);
    m_view->viewport()->update();
    m_view->setFocus();
    updateStatus();
}

void ViewerWindow::keyPressEvent(QKeyEvent *event) {
    // Escape closes, which is what every viewer does and what people press.
    if (event->key() == Qt::Key_Escape) {
        close();
        return;
    }
    // Single keys, as everywhere else in this program: CV.HLP gives H for the
    // hex view, and a viewer whose only key is Escape makes the rest of the
    // window a mouse target.
    if (event->modifiers() == Qt::NoModifier && !m_find->hasFocus()) {
        switch (event->key()) {
        case Qt::Key_H:
            jtf_viewer_toggle_hex(m_app);
            refresh();
            return;
        case Qt::Key_Slash:
            m_find->setFocus();
            m_find->selectAll();
            return;
        case Qt::Key_N:
        case Qt::Key_F3:
            findNext();
            return;
        default:
            break;
        }
    }
    QWidget::keyPressEvent(event);
}

void ViewerWindow::updateHints() {
    if (m_hints == nullptr) {
        return;
    }
    // Built from the same catalogue as everything else, so it follows the
    // language; the keys are this window's own and are not in the keymap,
    // which is why they are named here rather than looked up.
    struct Hint {
        const char *key;
        const char *label;
    };
    static const Hint kHints[] = {
        {"H", "viewer.hint.hex"},   {"/", "viewer.hint.find"},
        {"N", "viewer.hint.next"},  {"Esc", "viewer.hint.close"},
    };
    auto *row = qobject_cast<QHBoxLayout *>(m_hints->layout());
    if (row == nullptr) {
        return;
    }
    while (QLayoutItem *old = row->takeAt(0)) {
        delete old->widget();
        delete old;
    }

    // A keycap is fixed-width, so the chips are too: in proportional type
    // `/` and `Esc` make boxes of wildly different heights and weights, and
    // the row stops reading as a row of keys.
    QFont keyFont = QFontDatabase::systemFont(QFontDatabase::FixedFont);
    keyFont.setPointSizeF(font().pointSizeF());
    keyFont.setBold(true);

    for (const Hint &hint : kHints) {
        auto *chip = new QWidget(m_hints);
        auto *pair = new QHBoxLayout(chip);
        pair->setContentsMargins(0, 0, 0, 0);
        pair->setSpacing(5);
        auto *key = new QLabel(QLatin1String(hint.key), chip);
        key->setProperty("jtfHintKey", true);
        key->setFont(keyFont);
        auto *text = new QLabel(tr_(hint.label), chip);
        text->setProperty("jtfHintLabel", true);
        pair->addWidget(key);
        pair->addWidget(text);
        row->addWidget(chip);
    }
    row->addStretch(1);
}

void ViewerWindow::applyTheme(const QColor &mark, const QColor &text) {
    if (m_matches != nullptr) {
        m_matches->setHighlight(mark, text);
        m_view->viewport()->update();
    }
}

void ViewerWindow::changeEvent(QEvent *event) {
    QWidget::changeEvent(event);
    if (event->type() == QEvent::PaletteChange && m_findIcon != nullptr) {
        m_findIcon->setIcon(
            glyph::make(glyph::Shape::Search, palette().color(QPalette::PlaceholderText)));
    }
}

void ViewerWindow::closeEvent(QCloseEvent *event) {
    // The destructor releases the session. WA_DeleteOnClose already schedules
    // the deletion, so calling deleteLater here as well was a second one.
    QWidget::closeEvent(event);
    event->accept();
}
