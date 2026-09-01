#include "thumbnails.h"

#include <QFileInfo>
#include <QImageReader>
#include <QMetaObject>
#include <QMimeDatabase>
#include <QRunnable>
#include <QThreadPool>

namespace {

// How much memory the thumbnails may occupy, in kilobytes.
//
// Counted in bytes rather than in pictures. The bound used to be 4096
// *entries* with `QCache`'s default cost of one apiece, which says nothing
// about size: at the grid's 72-pixel edge that is 4096 x 72 x 72 x 4 bytes,
// about 84 MB of pixmaps, and at a larger edge proportionally more. A budget
// in bytes holds whatever number of thumbnails fits, which is the thing worth
// bounding.
constexpr int kMaxCacheKilobytes = 48 * 1024;

/// What one pixmap costs the cache, in kilobytes, never zero.
///
/// Zero-cost entries would never be evicted, so an unsuitable file remembered
/// as "nothing" still counts for one.
int pixmapCost(const QPixmap &pixmap) {
    const qint64 bytes =
        static_cast<qint64>(pixmap.width()) * pixmap.height() * (pixmap.depth() / 8);
    const qint64 kilobytes = bytes / 1024;
    const qint64 ceiling = static_cast<qint64>(kMaxCacheKilobytes);
    return static_cast<int>(qBound<qint64>(1, kilobytes, ceiling));
}

// Files larger than this are not thumbnailed. A very large image is exactly
// the one that would stall a worker, and the icon is a fine answer.
constexpr qint64 kMaxFileBytes = 64LL * 1024 * 1024;

// At most this many decodes at once. More threads than this only queue disk
// reads behind each other.
constexpr int kMaxThreads = 3;

class DecodeTask : public QRunnable {
public:
    DecodeTask(ThumbnailCache *cache, QString key, QString path, int edge, int row)
        : m_cache(cache), m_key(std::move(key)), m_path(std::move(path)), m_edge(edge),
          m_row(row) {
        setAutoDelete(true);
    }

    void run() override {
        QImageReader reader(m_path);
        reader.setAutoTransform(true); // honour EXIF orientation, as Finder does

        // Scaled during decode, not after: this is what keeps a huge image
        // from being fully materialised on the way to a small square.
        const QSize source = reader.size();
        if (source.isValid()) {
            QSize scaled = source;
            scaled.scale(m_edge, m_edge, Qt::KeepAspectRatio);
            reader.setScaledSize(scaled);
        }
        const QImage image = reader.read();
        if (image.isNull()) {
            // Reported anyway, so the request stops being pending and the
            // file is not retried on every repaint.
            QMetaObject::invokeMethod(m_cache, "store", Qt::QueuedConnection,
                                      Q_ARG(QString, m_key), Q_ARG(QString, m_path),
                                      Q_ARG(int, m_row), Q_ARG(QPixmap, QPixmap()));
            return;
        }
        QMetaObject::invokeMethod(m_cache, "store", Qt::QueuedConnection, Q_ARG(QString, m_key),
                                  Q_ARG(QString, m_path), Q_ARG(int, m_row),
                                  Q_ARG(QPixmap, QPixmap::fromImage(image)));
    }

private:
    ThumbnailCache *m_cache;
    QString m_key;
    QString m_path;
    int m_edge;
    int m_row;
};

} // namespace

ThumbnailCache::ThumbnailCache(QObject *parent) : QObject(parent), m_cache(kMaxCacheKilobytes) {
    m_pool = new QThreadPool(this);
    m_pool->setMaxThreadCount(kMaxThreads);
}

ThumbnailCache::~ThumbnailCache() {
    // Decoding threads must not outlive the object they report to.
    m_pool->clear();
    m_pool->waitForDone();
}

bool ThumbnailCache::canThumbnail(const QString &path) {
    const QFileInfo info(path);
    if (!info.isFile() || info.size() <= 0 || info.size() > kMaxFileBytes) {
        return false;
    }
    // By extension only: sniffing content here would read every file in the
    // directory just to decide what to draw.
    static const QMimeDatabase database;
    const QString type =
        database.mimeTypeForFile(info, QMimeDatabase::MatchExtension).name();
    return type.startsWith(QLatin1String("image/"));
}

QString ThumbnailCache::keyFor(const QString &path, int edge) const {
    // The modification time is part of the key, so an edited file gets a new
    // thumbnail instead of keeping the old one.
    const QFileInfo info(path);
    return QStringLiteral("%1|%2|%3")
        .arg(path)
        .arg(info.lastModified().toMSecsSinceEpoch())
        .arg(edge);
}

QPixmap ThumbnailCache::thumbnail(const QString &path, int edge, int row) {
    const QString key = keyFor(path, edge);
    if (QPixmap *cached = m_cache.object(key)) {
        return *cached;
    }
    if (m_pending.contains(key)) {
        return {};
    }
    if (!canThumbnail(path)) {
        // Remembered as "nothing", so an unsuitable file is asked about once.
        m_cache.insert(key, new QPixmap(), 1);
        return {};
    }
    m_pending.insert(key, row);
    m_pool->start(new DecodeTask(this, key, path, edge, row));
    return {};
}

void ThumbnailCache::store(const QString &key, const QString &path, int row,
                           const QPixmap &pixmap) {
    m_pending.remove(key);
    m_cache.insert(key, new QPixmap(pixmap), pixmapCost(pixmap));
    if (!pixmap.isNull()) {
        emit ready(row, path);
    }
}

void ThumbnailCache::clear() {
    m_cache.clear();
    m_pending.clear();
}
