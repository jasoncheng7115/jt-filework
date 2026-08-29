#include "bridge.h"
#include "mainwindow.h"

#include <QApplication>
#include <cstdio>

int main(int argc, char **argv) {
    QApplication application(argc, argv);
    QApplication::setApplicationName(QStringLiteral("JT FileWork"));
    QApplication::setOrganizationName(QStringLiteral("JT FileWork"));

    // The C++ token enum and Rust's ThemeToken::ALL must agree. Checking it
    // once at startup turns a silent wrong-colour bug into an immediate,
    // obvious failure.
    if (jtf_theme_token_count() != TokenCount) {
        std::fprintf(stderr,
                     "theme token mismatch: Rust reports %d, C++ header has %d.\n"
                     "Update JtfToken in cpp/bridge.h to match ThemeToken::ALL.\n",
                     jtf_theme_token_count(), static_cast<int>(TokenCount));
        return 2;
    }

    JtfApp *app = jtf_app_new();
    MainWindow window(app);
    window.show();

    const int status = QApplication::exec();
    jtf_app_free(app);
    return status;
}
