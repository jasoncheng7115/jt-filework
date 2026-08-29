// Batch rename.
//
// The preview is the point. It is computed by the same code the apply uses,
// updates as you type, and a collision blocks the whole batch rather than
// letting half of it through - a directory half renamed is a state the user
// did not ask for and cannot easily reverse.
#pragma once

#include "bridge.h"

#include <QDialog>

class QLineEdit;
class QCheckBox;
class QSpinBox;
class QTableWidget;
class QLabel;
class QPushButton;

class BatchRenameDialog : public QDialog {
    Q_OBJECT

public:
    BatchRenameDialog(JtfApp *app, int paneId, QWidget *parent = nullptr);

private:
    void refreshPreview();
    QString tr_(const char *key) const;
    QString trKey(const QString &key) const;

    JtfApp *m_app;
    int m_pane;
    QLineEdit *m_template = nullptr;
    QLineEdit *m_find = nullptr;
    QLineEdit *m_replace = nullptr;
    QCheckBox *m_regex = nullptr;
    QSpinBox *m_start = nullptr;
    QTableWidget *m_rows = nullptr;
    QLabel *m_summary = nullptr;
    QPushButton *m_apply = nullptr;
};
