#include "thumbnails.h"

#include <QFileInfo>
#include <QImageReader>
#include <QMetaObject>
#include <QMimeDatabase>
#include <QRunnable>
#include <QThreadPool>

namespace {

// How many thumbnails are kept. Pixmaps are small at this size; the bound is
// here so that scrolling a directory of a million images cannot grow without
// limit, not because the memory matters at ordinary sizes.
constexpr int kMaxCached = 4096;

// Files larger than this are not thumbnailed. A very large image is exactly
// the one that would stall a worker, and the icon is a fine answer.
constexpr qint64 kMaxFileBytes = 64LL * 1024 * 1024;

// At most this many decodes at once. More threads than this only queue disk
// reads behind each other.
constexpr int kMaxThreads = 3;

class DecodeTask : public QRunnable {
public:
    DecodeTask(ThumbnailCache *cache, QString key, QString path, int edge)
        : m_cache(cache), m_key(std::move(key)), m_path(std::move(path)), m_edge(edge) {
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
                                      Q_ARG(QPixmap, QPixmap()));
            return;
        }
        QMetaObject::invokeMethod(m_cache, "store", Qt::QueuedConnection, Q_ARG(QString, m_key),
                                  Q_ARG(QString, m_path),
                                  Q_ARG(QPixmap, QPixmap::fromImage(image)));
    }

private:
    ThumbnailCache *m_cache;
    QString m_key;
    QString m_path;
    int m_edge;
};

} // namespace

ThumbnailCache::ThumbnailCache(QObject *parent) : QObject(parent), m_cache(kMaxCached) {
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

QPixmap ThumbnailCache::thumbnail(const QString &path, int edge) {
    const QString key = keyFor(path, edge);
    if (QPixmap *cached = m_cache.object(key)) {
        return *cached;
    }
    if (m_pending.contains(key)) {
        return {};
    }
    if (!canThumbnail(path)) {
        // Remembered as "nothing", so an unsuitable file is asked about once.
        m_cache.insert(key, new QPixmap());
        return {};
    }
    m_pending.insert(key);
    m_pool->start(new DecodeTask(this, key, path, edge));
    return {};
}

void ThumbnailCache::store(const QString &key, const QString &path, const QPixmap &pixmap) {
    m_pending.remove(key);
    m_cache.insert(key, new QPixmap(pixmap));
    if (!pixmap.isNull()) {
        emit ready(path);
    }
}

void ThumbnailCache::clear() {
    m_cache.clear();
    m_pending.clear();
}
