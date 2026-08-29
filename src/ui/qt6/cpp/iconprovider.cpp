#include "iconprovider.h"

#include <QFileInfo>

namespace {
// A bounded per-path cache. Application bundles carry their own icons, so
// they cannot share a suffix entry, but an unbounded map over a million-entry
// directory is a leak.
constexpr int kMaxPathEntries = 2048;

// Directory extensions macOS presents as a single item with its own icon.
bool hasOwnIcon(const QString &suffix) {
    static const QStringList kSelfIconed = {
        QStringLiteral("app"),  QStringLiteral("bundle"),    QStringLiteral("framework"),
        QStringLiteral("kext"), QStringLiteral("plugin"),    QStringLiteral("prefpane"),
        QStringLiteral("dmg"),  QStringLiteral("localized"),
    };
    return kSelfIconed.contains(suffix, Qt::CaseInsensitive);
}
} // namespace

QIcon IconProvider::iconFor(const QString &path, bool isDirectory) {
    const QFileInfo info(path);
    const QString suffix = info.suffix().toLower();

    if (isDirectory && !hasOwnIcon(suffix)) {
        if (m_folder.isNull()) {
            m_folder = m_provider.icon(QFileIconProvider::Folder);
        }
        return m_folder;
    }

    if (hasOwnIcon(suffix)) {
        auto it = m_byPath.constFind(path);
        if (it != m_byPath.constEnd()) {
            return it.value();
        }
        if (m_byPath.size() >= kMaxPathEntries) {
            m_byPath.clear();
        }
        const QIcon icon = m_provider.icon(info);
        m_byPath.insert(path, icon);
        return icon;
    }

    if (suffix.isEmpty()) {
        if (m_file.isNull()) {
            m_file = m_provider.icon(QFileIconProvider::File);
        }
        return m_file;
    }

    auto it = m_bySuffix.constFind(suffix);
    if (it != m_bySuffix.constEnd()) {
        return it.value();
    }
    // The first file of a given type costs one OS lookup; every later file of
    // that type costs a hash probe. Scrolling a directory of ten thousand
    // photographs performs exactly one icon lookup.
    const QIcon icon = m_provider.icon(info);
    m_bySuffix.insert(suffix, icon);
    return icon;
}

void IconProvider::clear() {
    m_bySuffix.clear();
    m_byPath.clear();
    m_folder = QIcon();
    m_file = QIcon();
}
