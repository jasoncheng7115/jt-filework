#include "viewerwindow.h"
#include "jtfstring.h"

#include <QCheckBox>
#include <QCloseEvent>
#include <QComboBox>
#include <QFontDatabase>
#include <QHBoxLayout>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QListView>
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
    auto *barLayout = new QHBoxLayout(bar);
    barLayout->setContentsMargins(6, 4, 6, 4);

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
    connect(m_find, &QLineEdit::returnPressed, this, &ViewerWindow::findNext);
    barLayout->addWidget(m_find, 1);
    layout->addWidget(bar);

    m_model = new ViewerModel(app, this);
    m_view = new QListView(this);
    m_view->setModel(m_model);
    m_view->setUniformItemSizes(true); // what keeps the list virtualized
    m_view->setSelectionMode(QAbstractItemView::SingleSelection);
    m_view->setHorizontalScrollBarPolicy(Qt::ScrollBarAsNeeded);
    m_view->setWordWrap(false);
    // A viewer is always monospace: hex needs columns to line up, and so does
    // most of what people open a viewer for.
    m_view->setFont(QFontDatabase::systemFont(QFontDatabase::FixedFont));
    layout->addWidget(m_view, 1);

    m_status = new QLabel(this);
    m_status->setContentsMargins(6, 3, 6, 3);
    layout->addWidget(m_status);

    refresh();
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
        return;
    }
    const QModelIndex current = m_view->currentIndex();
    const uint64_t from = current.isValid() ? static_cast<uint64_t>(current.row() + 1) : 0;

    const QByteArray utf8 = needle.toUtf8();
    const int64_t found = jtf_viewer_find(m_app, utf8.constData(), from);
    if (found < 0) {
        m_status->setText(tr_("viewer.not_found"));
        return;
    }
    const QModelIndex index = m_model->index(static_cast<int>(found), 0);
    m_view->setCurrentIndex(index);
    m_view->scrollTo(index, QAbstractItemView::PositionAtCenter);
    updateStatus();
}

void ViewerWindow::keyPressEvent(QKeyEvent *event) {
    // Escape closes, which is what every viewer does and what people press.
    if (event->key() == Qt::Key_Escape) {
        close();
        return;
    }
    QWidget::keyPressEvent(event);
}

void ViewerWindow::closeEvent(QCloseEvent *event) {
    // The destructor releases the session. WA_DeleteOnClose already schedules
    // the deletion, so calling deleteLater here as well was a second one.
    QWidget::closeEvent(event);
    event->accept();
}
