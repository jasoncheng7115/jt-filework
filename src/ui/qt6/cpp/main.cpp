#include "bridge.h"
#include "mainwindow.h"

#include "watchdog.h"

#include <QApplication>
#include <QIcon>
#include <QFileInfo>
#include <QDir>
#include "platform/filetype.h"

#include <QLocale>
#include <cstdio>
#include <cstring>

int main(int argc, char **argv) {
    WatchdogApplication application(argc, argv);
    // macOS's guidelines discourage icons in menus, so Qt turns them off for
    // the whole application there - which silently dropped every icon we set
    // on a menu action. This program shows them deliberately: its menus are
    // long, and the picture is what makes a command findable in a list of
    // thirty (docs/UI_CONVENTIONS.md).
    QApplication::setAttribute(Qt::AA_DontShowIconsInMenus, false);
    QApplication::setApplicationName(QStringLiteral("jt-filework"));
    QApplication::setOrganizationName(QStringLiteral("jt-filework"));

    // The window icon, from the PNGs shipped beside the other data.
    //
    // macOS reads the .icns for the Dock and Finder and Qt reads neither, so
    // `QApplication::windowIcon()` was empty - which is why the About box,
    // which asks for exactly that, opened with a blank space where the icon
    // goes. On Windows and Linux this is also the taskbar icon.
    {
        QIcon icon;
        const QDir here(QCoreApplication::applicationDirPath());
        for (const QString &folder : {QStringLiteral("../Resources/appicon"),
                                      QStringLiteral("appicon")}) {
            const QDir dir(here.absoluteFilePath(folder));
            if (!dir.exists()) {
                continue;
            }
            for (const QFileInfo &png : dir.entryInfoList({QStringLiteral("*.png")}, QDir::Files)) {
                icon.addFile(png.absoluteFilePath());
            }
            break;
        }
        if (!icon.isNull()) {
            QApplication::setWindowIcon(icon);
        }
    }

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
    // The platform's trash, installed before anything can delete: Finder's
    // Put Back only works for items trashed through the system's own call.
    jtf_set_native_trash([](const char *path, char *buf, int len) -> int {
        const QString moved = filetype::moveToTrash(QString::fromUtf8(path));
        if (moved.isEmpty()) {
            return 0; // the platform declined; the fallback runs
        }
        const QByteArray utf8 = moved.toUtf8();
        if (utf8.size() >= len) {
            return 0;
        }
        std::memcpy(buf, utf8.constData(), static_cast<size_t>(utf8.size()));
        return utf8.size();
    });

    JtfApp *app = jtf_app_new(systemLocale.constData());
    application.startPeriodicReports();

    MainWindow window(app);
    window.show();
    // A session can hold more than one window: tearing a tab off makes one,
    // and that is remembered. Only the main window is built here, so without
    // this the rest stayed in the model with nothing on screen - counted by
    // everything that asks the model how many panes there are, and reachable
    // by nothing.
    MainWindow::syncWindows(app);

    const int status = QApplication::exec();
    jtf_app_free(app);

    if (application.enabled()) {
        const QString report = application.report();
        std::fputs(qPrintable(report), stderr);
    }
    return status;
}
