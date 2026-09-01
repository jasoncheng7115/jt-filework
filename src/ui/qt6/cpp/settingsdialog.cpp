#include "settingsdialog.h"
#include "dialogbuttons.h"
#include "icons.h"
#include "jtfstring.h"

#include <QCheckBox>
#include <QColorDialog>
#include <QComboBox>
#include <QFontDatabase>
#include <QFontMetricsF>
#include <QAbstractButton>
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

// The sidebar's recent list. The floor is 1 rather than 0 because turning the
// list off belongs to a switch, not to a number nobody would think to drag to
// zero; the ceiling is what `Places` keeps, so the setting can never promise
// more history than exists.
constexpr int kRecentMin = 1;
constexpr int kRecentMax = 32;
constexpr int kRecentDefault = 10;

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

ShortcutCapture::ShortcutCapture(const QString &title, const QString &prompt,
                                 const QString &cancelText, QWidget *parent)
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
    // This class has no catalogue of its own, so the one word it shows comes
    // in already translated from the dialog that opens it.
    if (QAbstractButton *cancel = buttons->button(QDialogButtonBox::Cancel)) {
        cancel->setText(cancelText);
        cancel->setIcon(glyph::make(glyph::Shape::Close, palette().color(QPalette::Text)));
        cancel->setProperty("jtfCloseIcon", true);
    }
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

void SettingsDialog::recolourIcons() {
    // Drawn glyphs carry the colour they were made with, so they have to be
    // remade when the colour changes. The theme is chosen *in this dialog*,
    // and rebuilding on `changed()` happened before the new palette had
    // reached us - so the tab icons kept the old theme's colour until the
    // dialog was reopened.
    const QColor colour = palette().color(QPalette::Text);
    if (m_tabs != nullptr && m_tabs->count() >= 3) {
        m_tabs->setTabIcon(0, glyph::make(glyph::Shape::Settings, colour));
        m_tabs->setTabIcon(1, glyph::make(glyph::Shape::Visible, colour));
        m_tabs->setTabIcon(2, glyph::make(glyph::Shape::Keyboard, colour));
    }
    for (QAbstractButton *button : findChildren<QAbstractButton *>()) {
        if (!button->icon().isNull() && button->property("jtfCloseIcon").toBool()) {
            button->setIcon(glyph::make(glyph::Shape::Close, colour));
        }
    }
}

void SettingsDialog::changeEvent(QEvent *event) {
    QDialog::changeEvent(event);
    // Asked of Qt rather than of whoever changed the theme: a palette change
    // arrives however it was caused, and this does not have to know.
    if (event->type() == QEvent::PaletteChange) {
        recolourIcons();
    }
}

void SettingsDialog::buildTabs() {
    // The dialog holds no state of its own - every control reads and writes
    // the model - so rebuilding is the honest way to change its language.
    // Retranslating in place would mean holding a pointer to every label the
    // three tabs create, and forgetting one would show a half-translated
    // panel.
    const int wasOn = m_tabs->currentIndex();
    while (m_tabs->count() > 0) {
        QWidget *page = m_tabs->widget(0);
        m_tabs->removeTab(0);
        page->deleteLater();
    }
    m_tabs->addTab(buildGeneralTab(), QIcon(), tr_("settings.tab.general"));
    m_tabs->addTab(buildAppearanceTab(), QIcon(), tr_("settings.tab.appearance"));
    m_tabs->addTab(buildKeyboardTab(), QIcon(), tr_("settings.tab.keyboard"));
    recolourIcons();
    if (wasOn >= 0 && wasOn < m_tabs->count()) {
        m_tabs->setCurrentIndex(wasOn);
    }
    setWindowTitle(tr_("settings.title"));
}

SettingsDialog::SettingsDialog(JtfApp *app, QWidget *parent) : QDialog(parent), m_app(app) {
    setWindowTitle(tr_("settings.title"));
    resize(680, 520);

    m_tabs = new QTabWidget(this);
    auto *tabs = m_tabs;
    buildTabs();

    m_buttons = new QDialogButtonBox(QDialogButtonBox::Close, this);
    auto *buttons = m_buttons;
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::accept);
    dialogs::localizeButtons(buttons, [this](const char *key) { return tr_(key); }, palette().color(QPalette::Text));

    auto *layout = new QVBoxLayout(this);
    layout->addWidget(tabs);
    layout->addWidget(buttons);
}

QWidget *SettingsDialog::buildGeneralTab() {
    auto *page = new QWidget(this);
    auto *form = new QFormLayout(page);
    form->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
    form->setLabelAlignment(Qt::AlignRight | Qt::AlignVCenter);
    form->setHorizontalSpacing(14);
    form->setVerticalSpacing(10);
    form->setContentsMargins(16, 14, 16, 14);
    // Rows keep their height and the slack goes to the bottom, rather than
    // the rows spreading out to fill a tall tab.
    form->setFormAlignment(Qt::AlignLeft | Qt::AlignTop);

    m_startupMode = new QComboBox(page);
    m_startupMode->addItem(tr_("settings.startup.last_session"));
    m_startupMode->addItem(tr_("settings.startup.home"));
    m_startupMode->addItem(tr_("settings.startup.fixed_location"));
    m_startupMode->setCurrentIndex(jtf_startup_mode(m_app));

    m_startupLocation = new QLineEdit(page);
    m_startupLocation->setText(
        jtfText([&](char *buf, int len) { return jtf_startup_location(m_app, buf, len); }));
    m_startupLocation->setEnabled(m_startupMode->currentIndex() == 2);

    // Not the default: Return on a settings page should not open a file
    // chooser.
    auto *browse = new QPushButton(tr_("settings.browse"), page);
    browse->setEnabled(m_startupLocation->isEnabled());
    // Not the default action: Return on a settings page should not open a
    // file chooser, and Qt makes the first button in a dialog the default
    // unless told otherwise.
    browse->setAutoDefault(false);
    browse->setDefault(false);

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
    form->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
    form->setLabelAlignment(Qt::AlignRight | Qt::AlignVCenter);
    form->setHorizontalSpacing(14);
    form->setVerticalSpacing(10);
    form->setContentsMargins(16, 14, 16, 14);
    // Rows keep their height and the slack goes to the bottom, rather than
    // the rows spreading out to fill a tall tab.
    form->setFormAlignment(Qt::AlignLeft | Qt::AlignTop);

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

    // What the preview area is drawn on. Three modes and a colour well, in
    // one row, because the colour only means anything with the third mode
    // chosen and separating them would leave a control that usually does
    // nothing.
    auto *previewRow = new QWidget(page);
    auto *previewLayout = new QHBoxLayout(previewRow);
    previewLayout->setContentsMargins(0, 0, 0, 0);
    previewLayout->setSpacing(8);
    auto *previewMode = new QComboBox(previewRow);
    previewMode->addItem(tr_("preview.background.theme"));
    previewMode->addItem(tr_("preview.background.chequer"));
    previewMode->addItem(tr_("preview.background.custom"));
    previewMode->setCurrentIndex(jtf_preview_background(m_app));
    auto *previewColour = new QPushButton(previewRow);
    previewColour->setText(tr_("preview.background.choose"));
    const auto storedColour = [this] {
        return jtfText(
            [&](char *buf, int len) { return jtf_preview_background_colour(m_app, buf, len); });
    };
    const auto applyPreview = [this, previewMode, previewColour, storedColour] {
        const int mode = previewMode->currentIndex();
        previewColour->setEnabled(mode == 2);
        const QByteArray colour = storedColour().toUtf8();
        jtf_set_preview_background(m_app, mode, colour.constData());
        emit changed();
    };
    connect(previewMode, &QComboBox::currentIndexChanged, this,
            [applyPreview](int) { applyPreview(); });
    connect(previewColour, &QPushButton::clicked, this,
            [this, previewMode, storedColour, applyPreview] {
                const QColor chosen = QColorDialog::getColor(
                    QColor(storedColour()), this, tr_("preview.background.choose"));
                if (!chosen.isValid()) {
                    return;
                }
                const QByteArray name = chosen.name(QColor::HexRgb).toUtf8();
                jtf_set_preview_background(m_app, previewMode->currentIndex(), name.constData());
                applyPreview();
            });
    previewColour->setEnabled(previewMode->currentIndex() == 2);
    previewLayout->addWidget(previewMode, 1);
    previewLayout->addWidget(previewColour);
    form->addRow(tr_("preview.background"), previewRow);

    auto *inspectorSide = new QComboBox(page);
    inspectorSide->addItem(tr_("inspector.position.right"));
    inspectorSide->addItem(tr_("inspector.position.bottom"));
    inspectorSide->setCurrentIndex(jtf_inspector_position(m_app));
    connect(inspectorSide, &QComboBox::currentIndexChanged, this, [this](int index) {
        jtf_set_inspector_position(m_app, index);
        emit changed();
    });
    form->addRow(tr_("inspector.position"), inspectorSide);

    auto *locale = new QComboBox(page);
    // Empty means "follow the system", and it is first because it is the
    // default: a user who never opens this screen gets their own language.
    locale->addItem(tr_("language.system"), QString());
    locale->addItem(tr_("language.english"), QStringLiteral("en"));
    locale->addItem(tr_("language.zh_tw"), QStringLiteral("zh-TW"));
    // The stored *preference*, not the effective locale: those differ exactly
    // when following the system, which is the case this entry exists for.
    const QString current =
        jtfText([&](char *buf, int len) { return jtf_locale_preference(m_app, buf, len); });
    locale->setCurrentIndex(qMax(0, locale->findData(current)));
    connect(locale, &QComboBox::currentIndexChanged, this, [this, locale](int) {
        const QByteArray code = locale->currentData().toString().toUtf8();
        jtf_set_locale(m_app, code.constData());
        emit changed();
        // This dialog is the one window guaranteed to be open when the
        // language changes, and the last one anybody thinks to check.
        QMetaObject::invokeMethod(this, &SettingsDialog::buildTabs, Qt::QueuedConnection);
    });
    form->addRow(tr_("menu.language"), locale);

    auto *monospace = new QCheckBox(tr_("settings.monospace"), page);
    monospace->setChecked(jtf_font_monospace(m_app) != 0);
    // The families actually installed, rather than a box to type a name into
    // and find out later that it was not one. Editable, so a family this
    // machine does not have - a session copied from another one - is still
    // shown and kept rather than silently replaced.
    auto *family = new QComboBox(page);
    family->setEditable(true);

    // Fills the list for the current mode, and says how wide each family is.
    //
    // Two things this fixes. Offering proportional families while "fixed-width"
    // is ticked is offering a choice that contradicts the tick. And a person
    // asking for a narrower face to fit more columns cannot tell which of forty
    // monospace families is narrower by reading their names - so each one
    // carries the width of a digit, which is the only number that matters when
    // every character is that wide.
    const QString currentFamily =
        jtfText([&](char *buf, int len) { return jtf_font_family(m_app, buf, len); });
    const auto fillFamilies = [this, family, currentFamily](bool fixedOnly) {
        const QString kept = family->currentIndex() <= 0
                                 ? currentFamily
                                 : family->currentData().toString();
        QSignalBlocker blocker(family);
        family->clear();
        family->addItem(tr_("settings.font_placeholder"), QString());

        QStringList families = QFontDatabase::families();
        families.removeDuplicates();
        for (const QString &name : std::as_const(families)) {
            if (fixedOnly && !QFontDatabase::isFixedPitch(name)) {
                continue;
            }
            QString label = name;
            if (fixedOnly) {
                QFont probe(name);
                probe.setPointSize(13);
                const qreal advance = QFontMetricsF(probe).horizontalAdvance(QLatin1Char('0'));
                label = QStringLiteral("%1  —  %2 px").arg(name).arg(advance, 0, 'f', 1);
            }
            family->addItem(label, name);
        }

        if (kept.isEmpty()) {
            family->setCurrentIndex(0);
            return;
        }
        const int at = family->findData(kept);
        if (at >= 0) {
            family->setCurrentIndex(at);
        } else {
            // A family this machine does not have - a session copied from
            // another one - is shown and kept rather than silently replaced.
            family->setEditText(kept);
        }
    };
    fillFamilies(monospace->isChecked());
    auto *size = new QSpinBox(page);
    size->setRange(0, 32);
    size->setSpecialValueText(tr_("settings.font_default_size"));
    size->setValue(jtf_font_point_size(m_app));

    const auto applyFont = [this, monospace, family, size] {
        // Index 0 is "the system's own", which is an empty name.
        const QString chosen =
            family->currentIndex() == 0 ? QString() : family->currentText().trimmed();
        const QByteArray name = chosen.toUtf8();
        jtf_set_font(m_app, name.constData(), size->value(), monospace->isChecked() ? 1 : 0);
        emit changed();
    };
    // Where the fixed-width face applies. The default is the aligned columns
    // alone: sizes, dates and permissions are compared down the column and
    // want digits that line up, while names are read one at a time and are
    // easier in proportional type.
    auto *scope = new QComboBox(page);
    scope->addItem(tr_("settings.monospace_aligned"));
    scope->addItem(tr_("settings.monospace_all"));
    scope->setCurrentIndex(jtf_font_monospace_everywhere(m_app) != 0 ? 1 : 0);
    scope->setEnabled(monospace->isChecked());
    connect(scope, &QComboBox::currentIndexChanged, this, [this](int index) {
        jtf_set_font_monospace_everywhere(m_app, index == 1 ? 1 : 0);
        emit changed();
    });

    connect(monospace, &QCheckBox::toggled, this, [applyFont, scope, fillFamilies](bool on) {
        // The scope means nothing when there is no fixed-width face in play,
        // and the family list should stop offering faces that contradict the
        // tick that was just made.
        scope->setEnabled(on);
        fillFamilies(on);
        applyFont();
    });
    connect(family, &QComboBox::currentIndexChanged, this, [applyFont](int) { applyFont(); });
    connect(family->lineEdit(), &QLineEdit::editingFinished, this, applyFont);
    connect(size, &QSpinBox::valueChanged, this, [applyFont](int) { applyFont(); });

    auto *parentRow = new QCheckBox(tr_("settings.parent_row"), page);
    parentRow->setChecked(jtf_parent_row(m_app) != 0);
    connect(parentRow, &QCheckBox::toggled, this, [this](bool on) {
        jtf_set_parent_row(m_app, on ? 1 : 0);
        emit changed();
    });

    auto *foldersFirst = new QCheckBox(tr_("settings.folders_first"), page);
    foldersFirst->setChecked(jtf_folders_first(m_app) != 0);
    connect(foldersFirst, &QCheckBox::toggled, this, [this](bool on) {
        jtf_set_folders_first(m_app, on ? 1 : 0);
        emit changed();
    });
    // How long the sidebar's 最近使用 list is. The useful number depends on how
    // the person works - long enough to hold this morning is clutter to
    // someone who only wants the last three - so it is a setting rather than
    // a constant.
    auto *recent = new QSpinBox(page);
    recent->setRange(kRecentMin, kRecentMax);
    const int storedRecent = jtf_recent_limit(m_app);
    recent->setValue(storedRecent > 0 ? storedRecent : kRecentDefault);
    connect(recent, &QSpinBox::valueChanged, this, [this](int value) {
        jtf_set_recent_limit(m_app, value);
        emit changed();
    });

    form->addRow(QString(), foldersFirst);
    form->addRow(QString(), parentRow);
    form->addRow(tr_("settings.recent_limit"), recent);

    form->addRow(QString(), monospace);
    form->addRow(tr_("settings.monospace_scope"), scope);
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
    presetLayout->addWidget(new QLabel(tr_("keyboard.profile"), presetRow));

    auto *preset = new QComboBox(presetRow);
    // CView first: it is the default, and the first entry in a list reads as
    // the normal one.
    preset->addItem(tr_("keyboard.profile.single_key"), QStringLiteral("single-key"));
    preset->addItem(tr_("keyboard.profile.native"), QStringLiteral("native"));
    const QString active =
        jtfText([&](char *buf, int len) { return jtf_keymap_name(m_app, buf, len); });
    // By value, not by a hardcoded index: the order of the two entries is
    // a layout decision and should not silently change what is selected.
    preset->setCurrentIndex(qMax(0, preset->findData(active)));
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

    ShortcutCapture capture(tr_("settings.capture_title"), tr_("settings.capture_prompt"),
                            tr_("dialog.cancel"), this);
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
