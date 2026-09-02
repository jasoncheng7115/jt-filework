#include "inspector.h"

#include "bridge.h"
#include "icons.h"
#include "theme.h"
#include "platform/filetype.h"
#include "jtfstring.h"

#include <QDateTime>
#include <QFileInfo>
#include <QFormLayout>
#include <QPainter>
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
    m_preview->setAutoFillBackground(true);
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
    // Labels left, not right. Right alignment keeps the gap between a label
    // and its value constant, which is the usual argument for it - but these
    // labels are two to four Han characters, so right-aligning them left a
    // ragged edge down the side of the panel and nothing to read down. Both
    // columns get a straight left edge instead.
    m_facts->setLabelAlignment(Qt::AlignLeft | Qt::AlignTop);
    m_facts->setFormAlignment(Qt::AlignLeft | Qt::AlignTop);
    // A narrow panel and a long value - "Portable Document Format", a size
    // with its exact byte count - do not fit on one line. Wrapping the row
    // puts the value under its label and gives it the height it asked for;
    // without this the label kept its width and the value was simply cut.
    m_facts->setRowWrapPolicy(QFormLayout::WrapLongRows);
    m_facts->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
    m_facts->setHorizontalSpacing(14);
    m_facts->setVerticalSpacing(8);
    bodyLayout->addLayout(m_facts);
    bodyLayout->addStretch(1);

    m_scroll = new QScrollArea(this);
    m_scroll->setWidget(body);
    m_scroll->setWidgetResizable(true);
    m_scroll->setFrameShape(QFrame::NoFrame);
    outer->addWidget(m_scroll, 1);
}

void Inspector::applyTheme(const QColor &glyphColour, const QColor &previewSurface) {
    m_close->setIcon(glyph::make(glyph::Shape::Close, glyphColour));
    m_previewSurface = previewSurface;
    applyPreviewBackground();
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

void Inspector::refreshTarget() {
    // The same target, read again.
    //
    // `setTarget` returns early when the path has not changed, which is what
    // keeps the panel from rebuilding on every cursor move. But a folder that
    // has just been measured has the same path and a different size: pressing
    // `Z` filled the size column in the list and left the panel still saying
    // 「未計算」about the very folder the cursor was on.
    if (!m_path.isEmpty()) {
        rebuild();
    }
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

void Inspector::applyPreviewBackground() {
    // A dark panel hides a scanned page and a white-bordered photograph, and
    // a chequer is the only honest way to show that a PNG has no background
    // of its own. Which of the three is right depends on what the user looks
    // at, so it is theirs to choose rather than the theme's to decide.
    const int mode = jtf_preview_background(m_app);
    QPalette pal = m_preview->palette();
    if (mode == 1) {
        // Painted rather than filled: a chequer has to be a texture.
        QPixmap tile(16, 16);
        tile.fill(QColor(0xC8, 0xC8, 0xC8));
        QPainter painter(&tile);
        painter.fillRect(0, 0, 8, 8, QColor(0xF0, 0xF0, 0xF0));
        painter.fillRect(8, 8, 8, 8, QColor(0xF0, 0xF0, 0xF0));
        painter.end();
        pal.setBrush(QPalette::Window, QBrush(tile));
    } else if (mode == 2) {
        const QString name = jtfText(
            [&](char *buf, int len) { return jtf_preview_background_colour(m_app, buf, len); });
        const QColor colour(name);
        pal.setBrush(QPalette::Window,
                     colour.isValid() ? QBrush(colour) : QBrush(m_previewSurface));
    } else {
        pal.setBrush(QPalette::Window, QBrush(m_previewSurface));
    }
    m_preview->setPalette(pal);
}

void Inspector::addRow(const QString &labelKey, const QString &value) {
    auto *label = new QLabel(tr_(labelKey.toUtf8().constData()), this);
    label->setProperty("jtfFactLabel", true);
    label->setAlignment(Qt::AlignLeft | Qt::AlignTop);
    auto *field = new QLabel(value, this);
    field->setAlignment(Qt::AlignLeft | Qt::AlignTop);
    field->setTextInteractionFlags(Qt::TextSelectableByMouse);
    field->setWordWrap(true);
    field->setFont(m_listFont);
    // Word wrap only produces more lines if the layout is willing to give
    // them room.
    QSizePolicy grows = field->sizePolicy();
    grows.setVerticalPolicy(QSizePolicy::MinimumExpanding);
    grows.setHeightForWidth(true);
    field->setSizePolicy(grows);
    field->setMinimumWidth(0);
    m_facts->addRow(label, field);
}

bool Inspector::showArchivePreview(const QString &path) {
    // CView shows what is inside an archive when you open one (CV.HLP 4);
    // this is the same answer without leaving the folder you are in.
    const QByteArray utf8 = path.toUtf8();
    const QString listing = jtfText([&](char *b, int l) {
        return jtf_archive_listing(m_app, utf8.constData(), b, l);
    });
    if (listing.isEmpty()) {
        return false;
    }
    m_text->setPlainText(listing);
    m_text->setFont(m_listFont);
    const int lines = listing.count(QLatin1Char('\n'));
    m_textStatus->setText(jtfFill(tr_("preview.entries"), "count", QString::number(lines)));
    return true;
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
    if (!info.isDir() && (showArchivePreview(path) || showTextPreview(path))) {
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

/// What kind of thing this is, worded exactly as the list's 種類 column words
/// it.
///
/// The two used to disagree twice over: the column asked the platform and the
/// panel asked Qt, so an archive read 「Zip封存檔」 in one and "Compressed
/// Archive File" in the other. One chain, one answer.
QString Inspector::typeName(const QString &path, const QFileInfo &info) {
    const QString name = m_icons.typeNameFor(path, info.isDir());
    if (!name.isEmpty()) {
        return name;
    }
    const QString suffix = info.suffix().toUpper();
    if (suffix.isEmpty()) {
        return tr_("kind.file");
    }
    return jtfFill(tr_("kind.suffix_file"), "ext", suffix);
}

void Inspector::rebuild() {
    clearRows();
    if (m_path.isEmpty()) {
        m_name->setText(tr_("inspector.empty"));
        m_preview->clear();
        return;
    }
    const QFileInfo info(m_path);
    m_name->setText(info.fileName());
    m_name->setToolTip(m_path);
    showPreview(m_path);

    // How many are marked, as a fact about the folder rather than as a
    // replacement for the panel.
    //
    // The panel used to show "4 items" and nothing else whenever more than one
    // was marked, which meant marks made an hour ago hid the preview of the
    // file under the cursor - and moving the cursor changed nothing. The set
    // is worth saying; it is not worth saying *instead*.
    if (m_marked > 1) {
        addRow(QStringLiteral("inspector.marked"),
               jtfFill(tr_("inspector.multiple"), "count", QString::number(m_marked)));
    }

    // The same answer the list's 種類 column gives, from the same place: the
    // platform first, Qt second, and the suffix phrased through the catalogue
    // if neither can name it. Asking QMimeDatabase directly here is what made
    // the panel say "Compressed Archive File" beside a row reading 「Zip封存檔」
    // - Qt's database has no Chinese, and the platform's does.
    addRow(QStringLiteral("inspector.kind"), typeName(m_path, info));
    if (info.isDir()) {
        addRow(QStringLiteral("inspector.size"), tr_("inspector.folder_size_hint"));
    } else {
        addRow(QStringLiteral("inspector.size"), humanSize(info.size()));
    }
    const QLocale locale;
    addRow(QStringLiteral("inspector.modified"),
           info.lastModified().toString(QStringLiteral("yyyy-MM-dd HH:mm")));
    const QDateTime born = info.birthTime();
    if (born.isValid()) {
        addRow(QStringLiteral("inspector.created"),
               born.toString(QStringLiteral("yyyy-MM-dd HH:mm")));
    }
    const QStringList tags = filetype::tagsFor(m_path);
    if (!tags.isEmpty()) {
        addRow(QStringLiteral("inspector.tags"), tags.join(QStringLiteral(", ")));
    }
    addRow(QStringLiteral("inspector.where"), info.absolutePath());
    if (info.isSymLink()) {
        // A symlink's own target, not what it resolves to: this panel
        // describes the entry in front of you (AGENTS.md 9).
        addRow(QStringLiteral("inspector.links_to"), info.symLinkTarget());
    }
}
