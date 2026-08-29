// Thumbnails for the file list.
//
// Decoding an image is slow enough to matter and happens for every row that
// scrolls past, so this is a cache in front of a worker pool, never a decode
// on the UI thread. The rules it exists to keep:
//
//   * Nothing decodes on the UI thread. Scrolling a folder of photographs
//     must stay as smooth as scrolling a folder of text files.
//   * A request for a row that has scrolled away is dropped rather than
//     finished. The user is not waiting for it.
//   * The decoder is told the size it is decoding to, so a 100-megapixel
//     photograph never becomes a 100-megapixel QImage on the way to a 32
//     pixel square (docs/SECURITY.md 13: bound what untrusted input can
//     allocate).
//   * The cache is bounded, keyed by path *and* modification time, so an
//     edited file does not keep showing its old picture.
#pragma once

#include <QCache>
#include <QHash>
#include <QObject>
#include <QPixmap>
#include <QSet>
#include <QString>

class QThreadPool;

class ThumbnailCache : public QObject {
    Q_OBJECT

public:
    explicit ThumbnailCache(QObject *parent = nullptr);
    ~ThumbnailCache() override;

    /// The thumbnail for `path`, or a null pixmap while one is being made.
    ///
    /// Asking is what schedules the work; a path nobody asks about is never
    /// decoded.
    QPixmap thumbnail(const QString &path, int edge);

    /// Whether this file is worth trying at all.
    static bool canThumbnail(const QString &path);

    /// Drop everything. For a theme or size change.
    void clear();

signals:
    /// A thumbnail arrived; the view should repaint the row.
    void ready(const QString &path);

private slots:
    void store(const QString &key, const QString &path, const QPixmap &pixmap);

private:
    QString keyFor(const QString &path, int edge) const;

    QCache<QString, QPixmap> m_cache;
    QSet<QString> m_pending;
    QThreadPool *m_pool = nullptr;
};
