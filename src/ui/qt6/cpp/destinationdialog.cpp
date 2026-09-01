#include "destinationdialog.h"

#include "dialogbuttons.h"
#include "foldertree.h"
#include "icons.h"
#include "iconprovider.h"
#include "jtfstring.h"

#include <QDialogButtonBox>
#include <QFileDialog>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QPushButton>
#include <QTreeView>
#include <QVBoxLayout>

DestinationDialog::DestinationDialog(JtfApp *app, bool moving, int count, QWidget *parent)
    : QDialog(parent), m_app(app) {
    // "移動到" does not say what is being moved. With a count it does, and the
    // count is the one thing a person wants confirmed before they pick a
    // destination for two hundred files.
    setWindowTitle(count > 1
                       ? jtfFill(tr_(moving ? "destination.move_title_many"
                                            : "destination.copy_title_many"),
                                 "count", QString::number(count))
                       : tr_(moving ? "destination.move_title" : "destination.copy_title"));
    setMinimumWidth(560);

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(16, 14, 16, 12);
    layout->setSpacing(10);

    auto *heading = new QLabel(tr_("destination.open_tabs"), this);
    heading->setProperty("jtfFactLabel", true);
    layout->addWidget(heading);

    m_tabs = new QListWidget(this);
    m_tabs->setObjectName(QStringLiteral("JtfDestinationTabs"));
    m_tabs->setAlternatingRowColors(true);
    m_tabs->setMinimumHeight(180);
    layout->addWidget(m_tabs, 1);

    // The whole folder tree, so a destination that is not already open in a tab
    // can still be reached without touching the mouse: arrows walk it, Right
    // opens a folder, and the path box below fills in as the cursor moves.
    auto *treeHeading = new QLabel(tr_("destination.browse_tree"), this);
    treeHeading->setProperty("jtfFactLabel", true);
    layout->addWidget(treeHeading);

    m_treeModel = new FolderTreeModel(m_app, this);
    m_tree = new QTreeView(this);
    m_tree->setModel(m_treeModel);
    m_tree->setHeaderHidden(true);
    m_tree->setIndentation(14);
    m_tree->setTextElideMode(Qt::ElideRight);
    m_tree->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_tree->setSelectionMode(QAbstractItemView::SingleSelection);
    m_tree->setMinimumHeight(180);
    layout->addWidget(m_tree, 1);

    // Moving the cursor is choosing, so the path box follows it. Nothing is
    // committed until OK or Enter, which is what makes arrowing around the
    // tree safe.
    connect(m_tree->selectionModel(), &QItemSelectionModel::currentChanged, this,
            [this](const QModelIndex &current, const QModelIndex &) {
                const QString path = m_treeModel->pathAt(current);
                if (!path.isEmpty()) {
                    m_path->setText(path);
                    m_tabs->clearSelection();
                }
            });
    connect(m_tree, &QTreeView::doubleClicked, this, &QDialog::accept);

    auto *pathRow = new QHBoxLayout;
    pathRow->setSpacing(8);
    auto *pathLabel = new QLabel(tr_("destination.path"), this);
    m_path = new QLineEdit(this);
    m_path->setPlaceholderText(tr_("destination.path_placeholder"));
    auto *browse = new QPushButton(tr_("destination.browse"), this);
    browse->setIcon(glyph::make(glyph::Shape::NewFolder, palette().color(QPalette::Text)));
    connect(browse, &QPushButton::clicked, this, [this] {
        const QString chosen = QFileDialog::getExistingDirectory(
            this, tr_("destination.browse"), m_path->text());
        if (!chosen.isEmpty()) {
            m_path->setText(chosen);
            m_tabs->clearSelection();
        }
    });
    pathRow->addWidget(pathLabel);
    pathRow->addWidget(m_path, 1);
    pathRow->addWidget(browse);
    layout->addLayout(pathRow);

    // Picking a tab fills the path box rather than replacing it, so what is
    // about to happen is written out in one place whichever way it was chosen.
    connect(m_tabs, &QListWidget::currentItemChanged, this,
            [this](QListWidgetItem *current, QListWidgetItem *) {
                if (current != nullptr) {
                    m_path->setText(current->data(Qt::UserRole).toString());
                }
            });
    connect(m_tabs, &QListWidget::itemDoubleClicked, this, &QDialog::accept);

    auto *buttons =
        new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    dialogs::localizeButtons(
        buttons, [this](const char *key) { return tr_(key); }, palette().color(QPalette::Text));
    layout->addWidget(buttons);

    addOpenTabs();
    m_tabs->setFocus();
}

QString DestinationDialog::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

void DestinationDialog::addOpenTabs() {
    IconProvider icons;
    const int panes = jtf_pane_count(m_app);
    const int active = jtf_active_pane(m_app);
    const int target = jtf_target_pane(m_app);

    for (int index = 0; index < panes; ++index) {
        // By id: everything that takes a pane takes its id, and the id is not
        // the position.
        const int pane = jtf_pane_id_at(m_app, index);
        if (pane < 0) {
            continue;
        }
        const int tabs = jtf_tab_count(m_app, pane);
        for (int tab = 0; tab < tabs; ++tab) {
            const QString path = jtfText([&](char *b, int l) {
                return jtf_tab_path(m_app, pane, tab, b, l);
            });
            if (path.isEmpty()) {
                continue;
            }
            const QString title = jtfText([&](char *b, int l) {
                return jtf_tab_title(m_app, pane, tab, b, l);
            });
            auto *item = new QListWidgetItem(icons.iconFor(path, true),
                                             QStringLiteral("%1 — %2").arg(title, path), m_tabs);
            item->setData(Qt::UserRole, path);
            // The pane C and M would have used, preselected, so the two-pane
            // habit is still one key and one Enter.
            if (pane == target && tab == jtf_active_tab(m_app, pane)) {
                m_tabs->setCurrentItem(item);
            }
            // Where the files are now is a legal answer but never the one
            // wanted, so it is listed and never preselected.
            if (pane == active && tab == jtf_active_tab(m_app, pane)) {
                item->setForeground(palette().color(QPalette::PlaceholderText));
            }
        }
    }
    if (m_tabs->currentItem() == nullptr && m_tabs->count() > 0 && jtf_pane_count(m_app) > 1) {
        m_tabs->setCurrentRow(0);
    }
}

QString DestinationDialog::destination() const { return m_path->text().trimmed(); }
