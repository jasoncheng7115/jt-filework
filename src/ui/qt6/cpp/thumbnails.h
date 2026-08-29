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
    /// decoded. `row` is remembered so the answer can be delivered back to
    /// the row that asked, rather than searched for.
    QPixmap thumbnail(const QString &path, int edge, int row);

    /// Whether this file is worth trying at all.
    static bool canThumbnail(const QString &path);

    /// Drop everything. For a theme or size change.
    void clear();

signals:
    /// A thumbnail arrived for the row that asked, which should repaint.
    ///
    /// The row is carried rather than the path alone: finding the row by path
    /// meant scanning the whole model, and in a directory of a hundred
    /// thousand that was a hundred thousand lookups on the UI thread to
    /// repaint one line - far more work than the decoding it was there to
    /// keep off the thread. The receiver still checks the row holds the path
    /// it expects, because rows move.
    void ready(int row, const QString &path);

private slots:
    void store(const QString &key, const QString &path, int row, const QPixmap &pixmap);

private:
    QString keyFor(const QString &path, int edge) const;

    QCache<QString, QPixmap> m_cache;
    QHash<QString, int> m_pending;
    QThreadPool *m_pool = nullptr;
};
