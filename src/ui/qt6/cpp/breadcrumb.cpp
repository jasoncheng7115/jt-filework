#include "breadcrumb.h"

#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QTimer>
#include <QFocusEvent>
#include <QKeyEvent>
#include <QLineEdit>
#include <QStringListModel>
#include <QAbstractItemView>
#include <QCompleter>
#include <QMouseEvent>
#include <QResizeEvent>
#include <QVBoxLayout>

namespace {
// Below this many segments there is never anything to hide.
constexpr int kMinimumSegments = 4;
// The height of a crumb, and so of the bar. Fixed, so that what the bar asks
// for does not depend on what is currently in it.
constexpr int kCrumbHeight = 20;
} // namespace

Breadcrumb::Breadcrumb(QWidget *parent) : QWidget(parent) {
    setObjectName(QStringLiteral("JtfCrumbs"));
    setCursor(Qt::IBeamCursor);

    auto *stack = new QVBoxLayout(this);
    stack->setContentsMargins(0, 0, 0, 0);

    // The bar takes whatever width it is given and never asks for more.
    //
    // `rebuild` already drops leading segments and puts an ellipsis in when
    // the crumbs do not fit - but it measured against `width()`, and the row
    // of buttons underneath was reporting its full length as a minimum. So the
    // splitter widened the pane until the crumbs fitted, `width()` grew with
    // it, and the elision it was measuring for never happened: opening a
    // folder eight levels deep shoved the pane beside it half off the screen.
    //
    // `Ignored` says the size hint is not a request. The same thing the status
    // line does, and for the same reason.
    setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Fixed);
    setMinimumWidth(0);

    m_crumbHost = new QWidget(this);
    m_crumbHost->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Fixed);
    m_crumbHost->setMinimumWidth(0);
    m_layout = new QHBoxLayout(m_crumbHost);
    m_layout->setContentsMargins(6, 2, 6, 2);
    m_layout->setSpacing(0);
    m_layout->setSizeConstraint(QLayout::SetNoConstraint);
    m_layout->addStretch(1);
    stack->addWidget(m_crumbHost);

    // The space beside the last segment answers with the same menu, for the
    // folder the pane is showing. Left alone it fell through to Qt's own
    // menu for whatever widget was under it - Undo/Cut/Paste, in English,
    // about a text field the user cannot see.
    m_crumbHost->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(m_crumbHost, &QWidget::customContextMenuRequested, this,
            [this](const QPoint &at) {
                if (!m_path.isEmpty()) {
                    emit segmentMenuRequested(m_path, m_crumbHost->mapToGlobal(at));
                }
            });

    m_edit = new QLineEdit(this);
    m_edit->setObjectName(QStringLiteral("JtfPathEdit"));
    m_edit->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Fixed);
    m_edit->setMinimumWidth(0);
    m_edit->setFrame(false);
    m_edit->setVisible(false);
    connect(m_edit, &QLineEdit::returnPressed, this, [this] { endEditing(true); });

    // Completion, from the pane's own listing rather than from Qt's file
    // system model - see `setCompletionSource`.
    m_completions = new QStringListModel(this);
    m_completer = new QCompleter(m_completions, this);
    // Case-insensitive because the filesystems people use here mostly are, and
    // a completer that refuses `/us` for `/Users` is worse than none.
    m_completer->setCaseSensitivity(Qt::CaseInsensitive);
    m_completer->setCompletionMode(QCompleter::PopupCompletion);
    // A path is compared whole, not by its last segment: the model holds full
    // paths so that what is inserted is a full path.
    m_completer->setFilterMode(Qt::MatchStartsWith);
    m_edit->setCompleter(m_completer);
    connect(m_edit, &QLineEdit::textEdited, this,
            [this](const QString &typed) { refreshCompletions(typed); });
    m_edit->installEventFilter(this);
    stack->addWidget(m_edit);
}

void Breadcrumb::setLeadingIcon(const QPixmap &icon) {
    m_leadingIcon = icon;
    rebuild();
}

void Breadcrumb::setCompletionSource(std::function<QStringList(const QString &folder)> source) {
    m_completionSource = std::move(source);
}

void Breadcrumb::refreshCompletions(const QString &typed) {
    if (!m_completionSource) {
        return;
    }
    // The folder being typed into is everything up to the last separator. With
    // no separator yet there is nothing to complete against.
    const int slash = typed.lastIndexOf(QLatin1Char('/'));
    if (slash < 0) {
        return;
    }
    // Keep the slash for the root, so `/` asks about `/` and not about "".
    const QString folder = slash == 0 ? QStringLiteral("/") : typed.left(slash);
    if (folder == m_completionFolder) {
        return; // still inside the same folder: the list already fits
    }
    m_completionFolder = folder;
    m_completions->setStringList(m_completionSource(folder));
}

void Breadcrumb::beginEditing() {
    // The list belongs to whatever gets typed next, not to the last edit.
    m_completionFolder.clear();
    m_completions->setStringList({});
    m_edit->setText(m_path);
    m_crumbHost->setVisible(false);
    m_edit->setVisible(true);
    m_edit->setFocus();
    m_edit->selectAll();
}

void Breadcrumb::endEditing(bool navigateThere) {
    const QString typed = m_edit->text().trimmed();
    m_edit->setVisible(false);
    m_crumbHost->setVisible(true);
    // Abandoning an edit puts the real path back, so a half-typed path never
    // stays on screen pretending to be where you are.
    if (navigateThere && !typed.isEmpty() && typed != m_path) {
        emit navigate(typed);
    }
}

bool Breadcrumb::eventFilter(QObject *watched, QEvent *event) {
    if (watched == m_edit && event->type() == QEvent::KeyPress) {
        auto *key = static_cast<QKeyEvent *>(event);
        if (key->key() == Qt::Key_Escape) {
            endEditing(false);
            return true;
        }
        if (key->key() == Qt::Key_Tab && key->modifiers() == Qt::NoModifier) {
            // Tab fills the path in, as it does in a shell. Claimed whether or
            // not there is anything to add: a Tab that sometimes completes and
            // sometimes jumps the focus out of the field is worse than one
            // that sometimes does nothing.
            completeTyped();
            return true;
        }
    }
    if (watched == m_edit && event->type() == QEvent::FocusOut) {
        // Clicking elsewhere abandons the edit - but the completer's own popup
        // takes the focus when it opens, and treating that as "the user has
        // gone somewhere else" closed the field the moment its suggestions
        // appeared. The focus then landed back in the file list, so the next
        // letter typed ran a command instead of continuing the path.
        //
        // Qt says where the focus went: `Qt::PopupFocusReason` is this case
        // and nothing else.
        const auto reason = static_cast<QFocusEvent *>(event)->reason();
        if (reason != Qt::PopupFocusReason
            && (m_completer == nullptr || !m_completer->popup()->isVisible())) {
            endEditing(false);
        }
    }
    return QWidget::eventFilter(watched, event);
}

// Fill in as much of the path as is unambiguous.
//
// Shell behaviour, because this is a path being typed and that is what
// everyone's fingers already expect: one match completes it and adds the
// separator so the next Tab carries on into it; several matches fill in as far
// as they agree and then show the list.
void Breadcrumb::completeTyped() {
    if (m_completions == nullptr || m_completer == nullptr) {
        return;
    }
    const QString typed = m_edit->text();
    QStringList matches;
    const QStringList all = m_completions->stringList();
    for (const QString &candidate : all) {
        if (candidate.startsWith(typed, Qt::CaseInsensitive)) {
            matches.append(candidate);
        }
    }
    if (matches.isEmpty()) {
        return;
    }

    // The longest opening every match shares. Compared without case, because
    // the completer matches without case and `/us` has to be able to reach
    // `/Users` - but taken from a real candidate, so what lands in the field is
    // spelled the way the disk spells it rather than the way it was typed.
    QString common = matches.constFirst();
    for (const QString &match : matches) {
        int shared = 0;
        while (shared < common.size() && shared < match.size()
               && common.at(shared).toCaseFolded() == match.at(shared).toCaseFolded()) {
            ++shared;
        }
        common.truncate(shared);
    }

    const bool single = matches.size() == 1;
    if (!single && common.size() <= typed.size()) {
        // Nothing more can be added without guessing. Show the choices.
        m_completer->setCompletionPrefix(typed);
        m_completer->complete();
        return;
    }

    QString filled = common;
    // Every candidate is a directory, so ending on one means the next Tab can
    // go straight on into it.
    if (single && !filled.endsWith(QLatin1Char('/'))) {
        filled += QLatin1Char('/');
    }
    if (filled == typed) {
        return;
    }
    m_edit->setText(filled);
    refreshCompletions(filled);
    if (single) {
        m_completer->popup()->hide();
    }
}

void Breadcrumb::mousePressEvent(QMouseEvent *event) {
    // The left button only. A right click here is on its way to the folder's
    // context menu, and turning the bar into a text field underneath the menu
    // that is about to open left the path selected for editing when all the
    // user asked for was a menu.
    if (event->button() != Qt::LeftButton) {
        QWidget::mousePressEvent(event);
        return;
    }
    // Only the empty space: a click on a crumb is a click on that crumb.
    beginEditing();
}

void Breadcrumb::setPath(const QString &path) {
    if (path == m_path) {
        return;
    }
    m_path = path;
    rebuild();
}

void Breadcrumb::resizeEvent(QResizeEvent *event) {
    QWidget::resizeEvent(event);
    // Only when the width actually changed. Rebuilding on every resize was a
    // feedback loop: adding and removing the crumb widgets changes this
    // widget's size hint, the parent layout answers by resizing it, and that
    // resize rebuilt them again. It never settled - the crumbs were destroyed
    // and recreated thousands of times a second, so Qt never reached the point
    // in the event loop where it would have shown and laid them out, and the
    // path bar was simply blank. The width is the only input the elision
    // depends on, so it is also the only thing worth reacting to.
    if (width() == m_builtForWidth) {
        return;
    }
    rebuild();
}

void Breadcrumb::rebuild() {
    if (m_edit != nullptr && m_edit->isVisible()) {
        return; // never rebuild the crumbs out from under an edit in progress
    }
    if (m_rebuilding) {
        return;
    }
    m_rebuilding = true;
    m_builtForWidth = width();
    while (QLayoutItem *item = m_layout->takeAt(0)) {
        if (QWidget *widget = item->widget()) {
            // Hidden and reparented out of the layout now, destroyed later.
            // Deleting a widget outright leaves any event already posted to
            // it - a show, a polish - pointing at freed memory, and the crash
            // lands far away in the event loop rather than here. This rebuild
            // runs on every resize, so there is always something in flight.
            widget->hide();
            widget->setParent(nullptr);
            widget->deleteLater();
        }
        delete item;
    }

    // A remote path arrives as `sftp://user@host/a/b`. Split on `/` alone and
    // the scheme comes apart into a crumb saying `sftp:` and another saying
    // `user@host`, neither of which is a folder anyone can click to. The
    // authority is one thing - it is the root of that server - so it is taken
    // off first and becomes the leading crumb, in place of `/`.
    QString remaining = m_path;
    QString root = QStringLiteral("/");
    QString rootLabel = QStringLiteral("/");
    const int scheme = m_path.indexOf(QStringLiteral("://"));
    if (scheme > 0) {
        const int firstSlash = m_path.indexOf(QLatin1Char('/'), scheme + 3);
        if (firstSlash > 0) {
            root = m_path.left(firstSlash);
            remaining = m_path.mid(firstSlash);
        } else {
            root = m_path;
            remaining.clear();
        }
        rootLabel = root;
    }

    const QStringList parts = remaining.split(QLatin1Char('/'), Qt::SkipEmptyParts);
    QStringList labels;
    QStringList paths;
    labels << rootLabel;
    paths << root;

    QString walked = (root == QStringLiteral("/")) ? QString() : root;
    for (const QString &part : parts) {
        walked += QLatin1Char('/') + part;
        labels << part;
        paths << walked;
    }

    // Work out how many trailing segments fit, then hide the middle. The last
    // segment is never dropped: it is the folder you are in.
    const QFontMetrics metrics(font());
    int available = width() - 24;
    int firstShown = 0;
    if (labels.size() > kMinimumSegments) {
        int used = metrics.horizontalAdvance(QStringLiteral("/  …  "));
        for (int i = labels.size() - 1; i >= 0; --i) {
            used += metrics.horizontalAdvance(labels.at(i)) + 22;
            if (used > available && i < labels.size() - 1) {
                firstShown = i + 1;
                break;
            }
        }
    }

    const auto addSegment = [this](const QString &label, const QString &path) {
        auto *button = new QPushButton(label, m_crumbHost);
        button->setFlat(true);
        // Fixed, and only as wide as its text. A default QPushButton asks for
        // 30px of height and a minimum width far past its label, which is
        // both wrong for a path bar and enough to keep the size hint moving.
        button->setSizePolicy(QSizePolicy::Maximum, QSizePolicy::Fixed);
        button->setFixedHeight(kCrumbHeight);
        button->setCursor(Qt::PointingHandCursor);
        button->setProperty("jtfCrumb", true);
        connect(button, &QPushButton::clicked, this, [this, path] { emit navigate(path); });
        // Right-click acts on the segment under the pointer, not on the
        // folder the pane is showing: the whole point of the menu is to do
        // something with an ancestor without going there first.
        button->setContextMenuPolicy(Qt::CustomContextMenu);
        connect(button, &QWidget::customContextMenuRequested, this,
                [this, button, path](const QPoint &at) {
                    emit segmentMenuRequested(path, button->mapToGlobal(at));
                });
        m_layout->addWidget(button);
    };

    if (!m_leadingIcon.isNull()) {
        m_leading = new QLabel(m_crumbHost);
        m_leading->setPixmap(m_leadingIcon);
        m_leading->setProperty("jtfCrumbSeparator", true);
        m_leading->setContentsMargins(2, 0, 6, 0);
        m_layout->addWidget(m_leading);
    }
    if (firstShown > 0) {
        addSegment(labels.first(), paths.first());
        auto *ellipsis = new QLabel(QStringLiteral("…"), m_crumbHost);
        ellipsis->setProperty("jtfCrumbSeparator", true);
        m_layout->addWidget(ellipsis);
    }
    for (int i = firstShown; i < labels.size(); ++i) {
        if (i > firstShown || firstShown > 0) {
            auto *separator = new QLabel(QStringLiteral("›"), m_crumbHost);
            separator->setProperty("jtfCrumbSeparator", true);
            m_layout->addWidget(separator);
        }
        addSegment(labels.at(i), paths.at(i));
    }
    m_layout->addStretch(1);
    m_rebuilding = false;
    Q_UNUSED(available);
}
