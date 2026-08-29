#include "bridge.h"
#include "mainwindow.h"

#include "watchdog.h"

#include <QApplication>
#include <QLocale>
#include <cstdio>
#include <cstring>

int main(int argc, char **argv) {
    WatchdogApplication application(argc, argv);
    QApplication::setApplicationName(QStringLiteral("jt-filework"));
    QApplication::setOrganizationName(QStringLiteral("jt-filework"));

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
    // The count matching is not enough. A reordering in Rust would keep the
    // count identical and silently recolour everything, so every name is
    // checked against its index.
    for (int i = 0; i < TokenCount; ++i) {
        char name[64] = {};
        jtf_theme_token_name(i, name, sizeof(name));
        if (std::strcmp(name, kTokenNames[i]) != 0) {
            std::fprintf(stderr,
                         "theme token %d is \"%s\" in Rust but \"%s\" in the C++ header.\n"
                         "JtfToken and kTokenNames must match ThemeToken::ALL in order.\n",
                         i, name, kTokenNames[i]);
            return 2;
        }
    }

    // uiLanguages(), not name(). QLocale::system().name() reports the format
    // locale, which on macOS mixes the region with the language the process
    // was launched in: a Mac set to Traditional Chinese with a Taiwan region
    // reports `en_TW` to an application it does not know is localized.
    // uiLanguages() is the user's actual ordered preference list.
    const QByteArray systemLocale =
        QLocale::system().uiLanguages().join(QLatin1Char(',')).toUtf8();
    JtfApp *app = jtf_app_new(systemLocale.constData());
    MainWindow window(app);
    window.show();

    const int status = QApplication::exec();
    jtf_app_free(app);

    if (application.enabled()) {
        const QString report = application.report();
        std::fputs(qPrintable(report), stderr);
    }
    return status;
}
