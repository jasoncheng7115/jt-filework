// Native file icons for the list.
//
// AGENTS.md 8: use the platform's own behaviour where users expect it, and a
// file's icon is the most visible instance of that. QFileIconProvider asks
// the OS, so on macOS this is the same icon Finder shows, including custom
// icons on application bundles.
//
// AGENTS.md 18.2 is the constraint: an icon lookup can hit the disk, and the
// list repaints on every scroll frame. So results are cached, and the cache is
// keyed by what actually determines the icon - the extension for ordinary
// files, the path only for the cases where the icon is per-item.
#pragma once

#include <QFileIconProvider>
#include <QHash>
#include <QIcon>
#include <QString>

class IconProvider {
public:
    // `path` is used only on a cache miss.
    QIcon iconFor(const QString &path, bool isDirectory);

    /// The platform's human-readable name for a file's type.
    ///
    /// "PDF Document", not "File". This is platform knowledge in exactly the
    /// same way the icon is, so it is answered here rather than in the core
    /// (`AGENTS.md` 8): the model asks the platform what a thing is, and the
    /// platform is the only one that knows what is installed to open it.
    QString typeNameFor(const QString &path, bool isDirectory);

    void clear();

private:
    QFileIconProvider m_provider;
    QIcon m_folder;
    QIcon m_file;
    QHash<QString, QIcon> m_bySuffix;
    QHash<QString, QString> m_typeBySuffix;
    QHash<QString, QIcon> m_byPath; // bundles and anything with its own icon
};
