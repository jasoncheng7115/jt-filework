#include "inspector.h"

#include "bridge.h"
#include "icons.h"
#include "theme.h"
#include "jtfstring.h"

#include <QDateTime>
#include <QFileInfo>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QImageReader>
#include <QLabel>
#include <QLocale>
#include <QMimeDatabase>
#include <QPushButton>
#include <QPlainTextEdit>
#include <QScrollArea>
#include <QVBoxLayout>

namespace {
// A thumbnail is decoded at this size at most. Reading a 100-megapixel image
// to fill a 220-pixel box is how a file manager freezes on a folder of
// photographs (docs/SECURITY.md 13: bound what untrusted input can allocate).
constexpr int kPreviewEdge = 220;

// How many lines of a text file the preview reads.
//
// A preview is a glance, not the viewer: reading a 4 GB log into a widget to
// show its first screenful is the allocation bound docs/SECURITY.md 13 is
// about, and the viewer window (Enter) is one keystroke away for the rest.
constexpr int kPreviewLines = 400;

QString humanSize(qint64 bytes) {
    static const char *const units[] = {"B", "KB", "MB", "GB", "TB", "PB"};
    double value = static_cast<double>(bytes);
    int unit = 0;
    while (value >= 1024.0 && unit < 5) {
        value /= 1024.0;
        ++unit;
    }
    return unit == 0
               ? QStringLiteral("%1 B").arg(bytes)
               : QStringLiteral("%1 %2 (%3 B)")
                     .arg(value, 0, 'f', 1)
                     .arg(QLatin1String(units[unit]), QLocale::system().toString(bytes));
}
} // namespace

Inspector::Inspector(JtfApp *app, QWidget *parent) : QWidget(parent), m_app(app) {
    setObjectName(QStringLiteral("JtfInspector"));
    setMinimumWidth(200);

    auto *outer = new QVBoxLayout(this);
    outer->setContentsMargins(0, 0, 0, 0);
    outer->setSpacing(0);

    auto *header = new QWidget(this);
    header->setObjectName(QStringLiteral("JtfInspectorHeader"));
    auto *headerRow = new QHBoxLayout(header);
    headerRow->setContentsMargins(10, 6, 6, 6);
    m_name = new QLabel(header);
    m_name->setObjectName(QStringLiteral("JtfInspectorName"));
    // A long file name elides rather than widening the panel: the panel's
    // width belongs to the user, who set it by dragging.
    m_name->setTextInteractionFlags(Qt::TextSelectableByMouse);
    m_name->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
    m_close = new QPushButton(header);
    m_close->setFlat(true);
    // Tinted by applyTheme; icons are theme output, not fixed assets.
    m_close->setFixedSize(22, 22);
    connect(m_close, &QPushButton::clicked, this, &Inspector::closeRequested);
    headerRow->addWidget(m_name, 1);
    headerRow->addWidget(m_close);
    outer->addWidget(header);

    auto *body = new QWidget(this);
    auto *bodyLayout = new QVBoxLayout(body);
    bodyLayout->setContentsMargins(12, 12, 12, 12);
    bodyLayout->setSpacing(12);
    m_preview = new QLabel(body);
    m_preview->setAlignment(Qt::AlignCenter);
    m_preview->setMinimumHeight(120);
    m_preview->setObjectName(QStringLiteral("JtfInspectorPreview"));
    bodyLayout->addWidget(m_preview);

    // Text gets read, not looked at, so it is a real text widget rather than
    // a pixmap: selectable, scrollable, and with the line numbers the
    // reference layout shows.
    m_text = new QPlainTextEdit(body);
    m_text->setObjectName(QStringLiteral("JtfInspectorText"));
    m_text->setReadOnly(true);
    m_text->setLineWrapMode(QPlainTextEdit::NoWrap);
    m_text->setFrameShape(QFrame::NoFrame);
    m_text->setMinimumHeight(200);
    m_text->setVisible(false);
    bodyLayout->addWidget(m_text);

    m_textStatus = new QLabel(body);
    m_textStatus->setObjectName(QStringLiteral("JtfInspectorTextStatus"));
    m_textStatus->setVisible(false);
    bodyLayout->addWidget(m_textStatus);
    m_facts = new QFormLayout();
    m_facts->setLabelAlignment(Qt::AlignRight | Qt::AlignVCenter);
    m_facts->setFormAlignment(Qt::AlignLeft | Qt::AlignTop);
    m_facts->setHorizontalSpacing(12);
    m_facts->setVerticalSpacing(6);
    bodyLayout->addLayout(m_facts);
    bodyLayout->addStretch(1);

    m_scroll = new QScrollArea(this);
    m_scroll->setWidget(body);
    m_scroll->setWidgetResizable(true);
    m_scroll->setFrameShape(QFrame::NoFrame);
    outer->addWidget(m_scroll, 1);
}

void Inspector::applyTheme(const QColor &glyphColour) {
    m_close->setIcon(glyph::make(glyph::Shape::Close, glyphColour));
}

QString Inspector::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

void Inspector::setTarget(const QString &path, int markedCount) {
    if (path == m_path && markedCount == m_marked) {
        return;
    }
    m_path = path;
    m_marked = markedCount;
    rebuild();
}

void Inspector::setListFont(const QFont &font) {
    // The facts are columns of values people compare down the panel, so they
    // get the list's monospace font for the same reason the list does.
    QFont bold = font;
    bold.setBold(true);
    m_name->setFont(bold);
    for (int i = 0; i < m_facts->rowCount(); ++i) {
        if (auto *item = m_facts->itemAt(i, QFormLayout::FieldRole)) {
            if (auto *widget = item->widget()) {
                widget->setFont(font);
            }
        }
    }
    m_listFont = font;
}

void Inspector::retranslate() {
    m_close->setToolTip(tr_("inspector.close"));
    rebuild();
}

void Inspector::clearRows() {
    while (m_facts->rowCount() > 0) {
        m_facts->removeRow(0);
    }
}

void Inspector::addRow(const QString &labelKey, const QString &value) {
    auto *label = new QLabel(tr_(labelKey.toUtf8().constData()), this);
    label->setProperty("jtfFactLabel", true);
    auto *field = new QLabel(value, this);
    field->setTextInteractionFlags(Qt::TextSelectableByMouse);
    field->setWordWrap(true);
    field->setFont(m_listFont);
    m_facts->addRow(label, field);
}

bool Inspector::showTextPreview(const QString &path) {
    const QByteArray utf8 = path.toUtf8();
    if (!jtf_preview_open(m_app, utf8.constData())) {
        return false;
    }
    const quint64 lines = jtf_preview_line_count(m_app);
    const int shown = static_cast<int>(qMin<quint64>(lines, kPreviewLines));

    QString body;
    for (int i = 0; i < shown; ++i) {
        body += jtfText([&](char *b, int l) {
            return jtf_preview_row(m_app, static_cast<quint64>(i), b, l);
        });
        body += QLatin1Char('\n');
    }
    m_text->setPlainText(body);
    m_text->setFont(m_listFont);

    // Encoding and line ending come from the decoder that produced the text,
    // not from a second guess made here (AGENTS.md 4).
    const auto key = [&](int (*fn)(const JtfApp *, char *, int)) {
        const QString k = jtfText([&](char *b, int l) { return fn(m_app, b, l); });
        return k.isEmpty() ? QString() : tr_(k.toUtf8().constData());
    };
    QString status = QStringLiteral("%1  ·  %2  ·  %3")
                         .arg(jtfFill(tr_("preview.lines"), "count", QString::number(lines)),
                              key(jtf_preview_encoding_key),
                              key(jtf_preview_line_ending_key));
    if (lines > kPreviewLines) {
        // Say that it is truncated. A preview that silently stops at line 400
        // reads as a file that ends at line 400.
        status += QStringLiteral("  ·  ") +
                  jtfFill(tr_("preview.truncated"), "count", QString::number(kPreviewLines));
    }
    m_textStatus->setText(status);
    return true;
}

void Inspector::showPreview(const QString &path) {
    const QFileInfo info(path);
    m_text->setVisible(false);
    m_textStatus->setVisible(false);
    if (!info.isDir() && showTextPreview(path)) {
        m_preview->setVisible(false);
        m_text->setVisible(true);
        m_textStatus->setVisible(true);
        return;
    }
    jtf_preview_close(m_app);
    m_preview->setVisible(true);

    QImageReader reader(path);
    // Ask the decoder what it is before decoding: a reader that cannot read
    // the format tells us so without allocating anything.
    if (reader.canRead()) {
        const QSize source = reader.size();
        if (source.isValid()) {
            QSize scaled = source;
            scaled.scale(kPreviewEdge, kPreviewEdge, Qt::KeepAspectRatio);
            reader.setScaledSize(scaled);
        }
        const QImage image = reader.read();
        if (!image.isNull()) {
            m_preview->setPixmap(QPixmap::fromImage(image));
            return;
        }
    }
    // Everything else gets its own large type icon, which is still an answer:
    // it says what kind of thing this is.
    m_preview->setPixmap(m_icons.iconFor(path, info.isDir()).pixmap(96, 96));
}

void Inspector::rebuild() {
    clearRows();
    if (m_path.isEmpty()) {
        m_name->setText(tr_("inspector.empty"));
        m_preview->clear();
        return;
    }
    // More than one marked file: report the set, not one member of it. Saying
    // "3 items" is honest; showing the first one's size is not.
    if (m_marked > 1) {
        m_name->setText(jtfFill(tr_("inspector.multiple"), "count", QString::number(m_marked)));
        m_preview->clear();
        return;
    }

    const QFileInfo info(m_path);
    m_name->setText(info.fileName());
    m_name->setToolTip(m_path);
    showPreview(m_path);

    const QMimeDatabase mime;
    addRow(QStringLiteral("inspector.kind"), mime.mimeTypeForFile(info).comment());
    if (info.isDir()) {
        addRow(QStringLiteral("inspector.size"), tr_("inspector.folder_size_hint"));
    } else {
        addRow(QStringLiteral("inspector.size"), humanSize(info.size()));
    }
    const QLocale locale;
    addRow(QStringLiteral("inspector.modified"),
           locale.toString(info.lastModified(), QLocale::ShortFormat));
    const QDateTime born = info.birthTime();
    if (born.isValid()) {
        addRow(QStringLiteral("inspector.created"), locale.toString(born, QLocale::ShortFormat));
    }
    addRow(QStringLiteral("inspector.where"), info.absolutePath());
    if (info.isSymLink()) {
        // A symlink's own target, not what it resolves to: this panel
        // describes the entry in front of you (AGENTS.md 9).
        addRow(QStringLiteral("inspector.links_to"), info.symLinkTarget());
    }
}
