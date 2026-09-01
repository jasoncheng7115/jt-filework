// The file viewer.
//
// AGENTS.md 14: the Viewer is stateful and richer than Preview, and it is
// expected to open things that do not fit in memory. The window therefore owns
// no content: it is a virtualized list that asks Rust for the rows it is about
// to paint, so a 10 GB log opens as fast as a 10 KB one.
#pragma once

#include "bridge.h"

#include <QAbstractListModel>
#include <QColor>
#include <QWidget>

class QListView;
class QComboBox;
class QLineEdit;
class QLabel;

// Rows on demand. Never holds more than what is on screen.
class ViewerModel : public QAbstractListModel {
    Q_OBJECT

public:
    explicit ViewerModel(JtfApp *app, QObject *parent = nullptr);

    int rowCount(const QModelIndex &parent = QModelIndex()) const override;
    QVariant data(const QModelIndex &index, int role) const override;

    void reload();

private:
    JtfApp *m_app;
    int m_rows = 0;
};

class ViewerWindow : public QWidget {
    Q_OBJECT

public:
    ViewerWindow(JtfApp *app, QWidget *parent = nullptr);
    ~ViewerWindow() override;

    void refresh();

    /// The colours the found text is picked out in. Without this the delegate
    /// held two default-constructed - that is, invalid - colours, and an
    /// invalid colour fills black: every match in the viewer came out as a
    /// black block with black text on it. The file list supplied them; this
    /// window never did.
    void applyTheme(const QColor &mark, const QColor &text);

protected:
    void keyPressEvent(QKeyEvent *event) override;
    void closeEvent(QCloseEvent *event) override;
    /// The glyphs this window draws carry their colour, so a theme change has
    /// to be applied rather than merely repainted.
    void changeEvent(QEvent *event) override;

private:
    void findNext();
    void updateStatus();
    QString tr_(const char *key) const;
    QString trKey(const QString &key) const;

    JtfApp *m_app;
    ViewerModel *m_model = nullptr;
    QListView *m_view = nullptr;
    QComboBox *m_encoding = nullptr;
    QLineEdit *m_find = nullptr;
    class MatchDelegate *m_matches = nullptr;
    /// A row of key chips, built the way the main window builds its own.
    QWidget *m_hints = nullptr;
    class QAction *m_findIcon = nullptr;
    class QSlider *m_zoom = nullptr;
    void updateHints();
    QLabel *m_status = nullptr;
};
