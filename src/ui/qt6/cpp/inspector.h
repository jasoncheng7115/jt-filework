// The inspector panel — docs/design/REFERENCE_LAYOUT.md 5.
//
// It answers "what is this file" for whatever the active pane is focused on:
// a preview at the top, then the facts, then the format-specific rows. It is
// a reader, never an editor: nothing in here changes a file.
#pragma once

#include "bridge.h"
#include "iconprovider.h"

#include <QColor>
#include <QFont>
#include <QWidget>

class QLabel;
class QFormLayout;
class QPushButton;
class QScrollArea;

class Inspector : public QWidget {
    Q_OBJECT

public:
    Inspector(JtfApp *app, QWidget *parent = nullptr);

    /// Show the file at `path`. An empty path shows the empty state.
    void setTarget(const QString &path, int markedCount);
    void setListFont(const QFont &font);
    void applyTheme(const QColor &glyphColour);
    void retranslate();

signals:
    void closeRequested();

private:
    /// Catalogue lookup. Qt's tr() would return the key: the strings live in
    /// the Rust catalogue, not in a .ts file (AGENTS.md 11).
    QString tr_(const char *key) const;
    void clearRows();
    bool showTextPreview(const QString &path);
    void addRow(const QString &labelKey, const QString &value);
    void showPreview(const QString &path);
    void rebuild();

    QLabel *m_name = nullptr;
    QPushButton *m_close = nullptr;
    QLabel *m_preview = nullptr;
    class QPlainTextEdit *m_text = nullptr;
    QLabel *m_textStatus = nullptr;
    QFormLayout *m_facts = nullptr;
    QScrollArea *m_scroll = nullptr;
    JtfApp *m_app = nullptr;
    IconProvider m_icons;
    QFont m_listFont;
    QString m_path;
    int m_marked = 0;
};
