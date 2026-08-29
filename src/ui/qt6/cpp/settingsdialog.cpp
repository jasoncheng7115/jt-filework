#include "settingsdialog.h"
#include "jtfstring.h"

#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFileDialog>
#include <QFormLayout>
#include <QHeaderView>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPushButton>
#include <QSpinBox>
#include <QTabWidget>
#include <QTableWidget>
#include <QVBoxLayout>

namespace {

// Qt key -> the keymap file's spelling. Only keys the format knows about are
// accepted; anything else is refused rather than stored as something that
// will not parse back.
QString keyName(int key) {
    switch (key) {
    case Qt::Key_Up: return QStringLiteral("up");
    case Qt::Key_Down: return QStringLiteral("down");
    case Qt::Key_Left: return QStringLiteral("left");
    case Qt::Key_Right: return QStringLiteral("right");
    case Qt::Key_Return:
    case Qt::Key_Enter: return QStringLiteral("enter");
    case Qt::Key_Escape: return QStringLiteral("escape");
    case Qt::Key_Space: return QStringLiteral("space");
    case Qt::Key_Tab: return QStringLiteral("tab");
    case Qt::Key_Backspace: return QStringLiteral("backspace");
    case Qt::Key_Delete: return QStringLiteral("delete");
    case Qt::Key_Home: return QStringLiteral("home");
    case Qt::Key_End: return QStringLiteral("end");
    case Qt::Key_PageUp: return QStringLiteral("pageup");
    case Qt::Key_PageDown: return QStringLiteral("pagedown");
    case Qt::Key_Insert: return QStringLiteral("insert");
    default:
        break;
    }
    if (key >= Qt::Key_F1 && key <= Qt::Key_F24) {
        return QStringLiteral("f%1").arg(key - Qt::Key_F1 + 1);
    }
    if (key >= 0x20 && key <= 0x7e) {
        return QString(QChar(key)).toLower();
    }
    return {};
}

} // namespace

// ---------------------------------------------------------- ShortcutCapture

ShortcutCapture::ShortcutCapture(const QString &title, const QString &prompt, QWidget *parent)
    : QDialog(parent) {
    setWindowTitle(title);
    auto *layout = new QVBoxLayout(this);
    layout->addWidget(new QLabel(prompt, this));
    m_display = new QLabel(this);
    m_display->setAlignment(Qt::AlignCenter);
    m_display->setMinimumHeight(48);
    layout->addWidget(m_display);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Cancel, this);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    layout->addWidget(buttons);
    setMinimumWidth(340);
}

void ShortcutCapture::keyPressEvent(QKeyEvent *event) {
    const int key = event->key();
    // A modifier on its own is not a shortcut; wait for the real key.
    if (key == Qt::Key_Control || key == Qt::Key_Shift || key == Qt::Key_Alt ||
        key == Qt::Key_Meta) {
        return;
    }
    if (key == Qt::Key_Escape && event->modifiers() == Qt::NoModifier) {
        reject();
        return;
    }

    const QString name = keyName(key);
    if (name.isEmpty()) {
        return;
    }

    QStringList parts;
    // Qt reports Command as ControlModifier on macOS, which is exactly what
    // the keymap calls `primary`.
    if (event->modifiers().testFlag(Qt::ControlModifier)) {
        parts << QStringLiteral("primary");
    }
    if (event->modifiers().testFlag(Qt::MetaModifier)) {
        parts << QStringLiteral("ctrl");
    }
    if (event->modifiers().testFlag(Qt::AltModifier)) {
        parts << QStringLiteral("alt");
    }
    if (event->modifiers().testFlag(Qt::ShiftModifier)) {
        parts << QStringLiteral("shift");
    }
    parts << name;

    m_chord = parts.join(QLatin1Char('+'));
    m_display->setText(m_chord);
    accept();
}

// ----------------------------------------------------------- SettingsDialog

QString SettingsDialog::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

QString SettingsDialog::trKey(const QString &key) const {
    const QByteArray utf8 = key.toUtf8();
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, utf8.constData(), buf, len); });
}

SettingsDialog::SettingsDialog(JtfApp *app, QWidget *parent) : QDialog(parent), m_app(app) {
    setWindowTitle(tr_("settings.title"));
    resize(680, 520);

    auto *tabs = new QTabWidget(this);
    tabs->addTab(buildGeneralTab(), tr_("settings.tab.general"));
    tabs->addTab(buildAppearanceTab(), tr_("settings.tab.appearance"));
    tabs->addTab(buildKeyboardTab(), tr_("settings.tab.keyboard"));

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Close, this);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::accept);

    auto *layout = new QVBoxLayout(this);
    layout->addWidget(tabs);
    layout->addWidget(buttons);
}

QWidget *SettingsDialog::buildGeneralTab() {
    auto *page = new QWidget(this);
    auto *form = new QFormLayout(page);

    m_startupMode = new QComboBox(page);
    m_startupMode->addItem(tr_("settings.startup.last_session"));
    m_startupMode->addItem(tr_("settings.startup.home"));
    m_startupMode->addItem(tr_("settings.startup.fixed_location"));
    m_startupMode->setCurrentIndex(jtf_startup_mode(m_app));

    m_startupLocation = new QLineEdit(page);
    m_startupLocation->setText(
        jtfText([&](char *buf, int len) { return jtf_startup_location(m_app, buf, len); }));
    m_startupLocation->setEnabled(m_startupMode->currentIndex() == 2);

    auto *browse = new QPushButton(tr_("settings.browse"), page);
    browse->setEnabled(m_startupLocation->isEnabled());

    const auto applyStartup = [this, browse] {
        const int mode = m_startupMode->currentIndex();
        m_startupLocation->setEnabled(mode == 2);
        browse->setEnabled(mode == 2);

        // Leaving "restore the last session" erases what was stored, and the
        // user is told that is what just happened rather than discovering it
        // next launch (docs/UI_UX_SPEC.md 16.2).
        const QByteArray location = m_startupLocation->text().toUtf8();
        jtf_set_startup(m_app, mode, location.constData());
        emit changed();
    };

    connect(m_startupMode, &QComboBox::currentIndexChanged, this, [applyStartup](int) { applyStartup(); });
    connect(m_startupLocation, &QLineEdit::editingFinished, this, applyStartup);
    connect(browse, &QPushButton::clicked, this, [this, applyStartup] {
        const QString chosen = QFileDialog::getExistingDirectory(
            this, tr_("settings.startup.fixed_location"), m_startupLocation->text());
        if (!chosen.isEmpty()) {
            m_startupLocation->setText(chosen);
            applyStartup();
        }
    });

    auto *locationRow = new QWidget(page);
    auto *locationLayout = new QHBoxLayout(locationRow);
    locationLayout->setContentsMargins(0, 0, 0, 0);
    locationLayout->addWidget(m_startupLocation, 1);
    locationLayout->addWidget(browse);

    form->addRow(tr_("settings.startup"), m_startupMode);
    form->addRow(QString(), locationRow);

    m_rememberTabs = new QCheckBox(tr_("settings.remember_closed_tabs"), page);
    m_rememberTabs->setChecked(jtf_remember_closed_tabs(m_app) != 0);
    m_rememberMarks = new QCheckBox(tr_("settings.remember_marks"), page);
    m_rememberMarks->setChecked(jtf_remember_marks(m_app) != 0);

    const auto applyRemember = [this] {
        jtf_set_remember(m_app, m_rememberTabs->isChecked() ? 1 : 0,
                         m_rememberMarks->isChecked() ? 1 : 0);
        emit changed();
    };
    connect(m_rememberTabs, &QCheckBox::toggled, this, [applyRemember](bool) { applyRemember(); });
    connect(m_rememberMarks, &QCheckBox::toggled, this, [applyRemember](bool) { applyRemember(); });

    form->addRow(QString(), m_rememberTabs);
    form->addRow(QString(), m_rememberMarks);
    return page;
}

QWidget *SettingsDialog::buildAppearanceTab() {
    auto *page = new QWidget(this);
    auto *form = new QFormLayout(page);

    auto *theme = new QComboBox(page);
    theme->addItem(tr_("theme.system"));
    theme->addItem(tr_("theme.light"));
    theme->addItem(tr_("theme.dark"));
    theme->setCurrentIndex(jtf_theme_mode(m_app));
    connect(theme, &QComboBox::currentIndexChanged, this, [this](int index) {
        jtf_set_theme_mode(m_app, index);
        emit changed();
    });
    form->addRow(tr_("menu.theme"), theme);

    auto *locale = new QComboBox(page);
    locale->addItem(tr_("language.english"), QStringLiteral("en"));
    locale->addItem(tr_("language.zh_tw"), QStringLiteral("zh-TW"));
    const QString current =
        jtfText([&](char *buf, int len) { return jtf_locale(m_app, buf, len); });
    locale->setCurrentIndex(current == QLatin1String("zh-TW") ? 1 : 0);
    connect(locale, &QComboBox::currentIndexChanged, this, [this, locale](int) {
        const QByteArray code = locale->currentData().toString().toUtf8();
        jtf_set_locale(m_app, code.constData());
        emit changed();
    });
    form->addRow(tr_("menu.language"), locale);

    auto *monospace = new QCheckBox(tr_("settings.monospace"), page);
    monospace->setChecked(jtf_font_monospace(m_app) != 0);
    auto *family = new QLineEdit(page);
    family->setText(jtfText([&](char *buf, int len) { return jtf_font_family(m_app, buf, len); }));
    family->setPlaceholderText(tr_("settings.font_placeholder"));
    auto *size = new QSpinBox(page);
    size->setRange(0, 32);
    size->setSpecialValueText(tr_("settings.font_default_size"));
    size->setValue(jtf_font_point_size(m_app));

    const auto applyFont = [this, monospace, family, size] {
        const QByteArray name = family->text().trimmed().toUtf8();
        jtf_set_font(m_app, name.constData(), size->value(), monospace->isChecked() ? 1 : 0);
        emit changed();
    };
    connect(monospace, &QCheckBox::toggled, this, [applyFont](bool) { applyFont(); });
    connect(family, &QLineEdit::editingFinished, this, applyFont);
    connect(size, &QSpinBox::valueChanged, this, [applyFont](int) { applyFont(); });

    form->addRow(QString(), monospace);
    form->addRow(tr_("settings.font_family"), family);
    form->addRow(tr_("settings.font_size"), size);
    return page;
}

QWidget *SettingsDialog::buildKeyboardTab() {
    auto *page = new QWidget(this);
    auto *layout = new QVBoxLayout(page);

    auto *presetRow = new QWidget(page);
    auto *presetLayout = new QHBoxLayout(presetRow);
    presetLayout->setContentsMargins(0, 0, 0, 0);
    presetLayout->addWidget(new QLabel(tr_("menu.keymap"), presetRow));

    auto *preset = new QComboBox(presetRow);
    preset->addItem(tr_("keymap.platform"), QStringLiteral("platform"));
    preset->addItem(tr_("keymap.cview"), QStringLiteral("cview"));
    const QString active =
        jtfText([&](char *buf, int len) { return jtf_keymap_name(m_app, buf, len); });
    preset->setCurrentIndex(active == QLatin1String("cview") ? 1 : 0);
    connect(preset, &QComboBox::currentIndexChanged, this, [this, preset](int) {
        const QByteArray name = preset->currentData().toString().toUtf8();
        jtf_set_keymap(m_app, name.constData());
        reloadShortcuts();
        emit changed();
    });
    presetLayout->addWidget(preset, 1);

    auto *reset = new QPushButton(tr_("settings.reset_shortcuts"), presetRow);
    connect(reset, &QPushButton::clicked, this, [this] {
        jtf_reset_shortcuts(m_app);
        reloadShortcuts();
        emit changed();
    });
    presetLayout->addWidget(reset);
    layout->addWidget(presetRow);

    m_shortcuts = new QTableWidget(page);
    m_shortcuts->setColumnCount(3);
    m_shortcuts->setHorizontalHeaderLabels(
        {tr_("settings.column.category"), tr_("settings.column.command"),
         tr_("settings.column.shortcut")});
    m_shortcuts->horizontalHeader()->setStretchLastSection(false);
    m_shortcuts->verticalHeader()->setVisible(false);
    m_shortcuts->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_shortcuts->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_shortcuts->setAlternatingRowColors(true);
    connect(m_shortcuts, &QTableWidget::cellDoubleClicked, this,
            [this](int row, int) { editShortcut(row); });
    layout->addWidget(m_shortcuts, 1);

    m_shortcutHint = new QLabel(page);
    m_shortcutHint->setWordWrap(true);
    // An upgrade that drops a binding says so here, rather than leaving the
    // user with a key that quietly stopped working (docs/UPGRADE.md 4.2).
    const int dropped = jtf_dropped_bindings(m_app);
    if (dropped > 0) {
        m_shortcutHint->setText(
            jtfFill(tr_("settings.dropped_bindings"), "count", QString::number(dropped)) +
            QStringLiteral("\n") + tr_("settings.shortcut_hint"));
    } else {
        m_shortcutHint->setText(tr_("settings.shortcut_hint"));
    }
    layout->addWidget(m_shortcutHint);

    reloadShortcuts();
    return page;
}

void SettingsDialog::reloadShortcuts() {
    const int count = jtf_command_count(m_app);
    m_shortcuts->setRowCount(count);

    for (int i = 0; i < count; ++i) {
        char id[128] = {};
        char label[128] = {};
        char category[128] = {};
        if (!jtf_command_at(m_app, i, id, sizeof(id), label, sizeof(label), category,
                            sizeof(category))) {
            continue;
        }
        const QString shortcut = jtfText(
            [&](char *buf, int len) { return jtf_shortcut_for(m_app, id, buf, len); });

        auto *categoryItem = new QTableWidgetItem(trKey(QString::fromUtf8(category)));
        auto *commandItem = new QTableWidgetItem(trKey(QString::fromUtf8(label)));
        auto *shortcutItem = new QTableWidgetItem(shortcut);
        // The command id travels with the row, so a re-sort cannot bind the
        // wrong command.
        commandItem->setData(Qt::UserRole, QString::fromUtf8(id));

        if (jtf_command_is_destructive(m_app, i)) {
            commandItem->setToolTip(tr_("settings.destructive"));
        }
        m_shortcuts->setItem(i, 0, categoryItem);
        m_shortcuts->setItem(i, 1, commandItem);
        m_shortcuts->setItem(i, 2, shortcutItem);
    }
    m_shortcuts->resizeColumnsToContents();
}

void SettingsDialog::editShortcut(int row) {
    QTableWidgetItem *commandItem = m_shortcuts->item(row, 1);
    if (!commandItem) {
        return;
    }
    const QString id = commandItem->data(Qt::UserRole).toString();

    ShortcutCapture capture(tr_("settings.capture_title"), tr_("settings.capture_prompt"), this);
    if (capture.exec() != QDialog::Accepted || capture.chord().isEmpty()) {
        return;
    }

    const QByteArray idUtf8 = id.toUtf8();
    const QByteArray chordUtf8 = capture.chord().toUtf8();
    char conflict[128] = {};

    if (jtf_bind_shortcut(m_app, idUtf8.constData(), chordUtf8.constData(), conflict,
                          sizeof(conflict))) {
        reloadShortcuts();
        emit changed();
        return;
    }

    // A refused binding names the command that already owns the chord, rather
    // than saying only that it failed (docs/UI_TEST_PLAN.md KEY-005).
    const QString other = QString::fromUtf8(conflict);
    QString message = tr_("settings.conflict");
    message = jtfFill(message, "chord", capture.chord());
    message = jtfFill(message, "command", other.isEmpty() ? QStringLiteral("?") : other);
    QMessageBox::warning(this, tr_("settings.title"), message);
}
