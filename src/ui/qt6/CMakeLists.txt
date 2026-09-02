# Qt 6 Widgets front end for jt-filework (Phase 0B PoC, ADR-0001).
#
# The C++ here is the UI layer only. All logic lives in the Rust crates and is
# reached through the C ABI in bridge/ - AGENTS.md 4 keeps the toolkit
# replaceable, and this build file is the seam.

cmake_minimum_required(VERSION 3.21)

# The version, read from Cargo.toml and never written out again here.
#
# It reaches three places: the macOS bundle's Info.plist, the Windows resource
# stamp that Explorer and the properties dialog read, and - through
# `CARGO_PKG_VERSION` on the Rust side - the About box and the window title.
# Every hand-written copy is a number that drifts, and a build whose properties
# dialog disagrees with its own About box is worse than one with no version at
# all.
file(READ "${CMAKE_CURRENT_LIST_DIR}/../../../Cargo.toml" JTF_CARGO_TOML)
string(REGEX MATCH "\nversion = \"([0-9]+\\.[0-9]+\\.[0-9]+)\"" _ "${JTF_CARGO_TOML}")
set(JTF_VERSION "${CMAKE_MATCH_1}")
if(NOT JTF_VERSION)
    message(FATAL_ERROR "could not read the version out of Cargo.toml")
endif()

# Objective-C++ is enabled on Apple only; the platform-neutral build never
# sees it (AGENTS.md 5).
if(APPLE)
    project(jt-filework-qt6 LANGUAGES CXX OBJCXX)
else()
    project(jt-filework-qt6 LANGUAGES CXX)
endif()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

find_package(Qt6 REQUIRED COMPONENTS Widgets Svg)

# `qt_standard_project_setup()` arrived in Qt 6.3, and Ubuntu 22.04 - the
# oldest release this project builds on, and therefore the one the Linux
# binaries are made on - ships 6.2.4. Everything the call does that this
# project relies on is three variables, so they are set directly and the
# convenience is used only where it exists.
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTOUIC ON)
set(CMAKE_AUTORCC ON)
if(COMMAND qt_standard_project_setup)
    qt_standard_project_setup()
endif()

# --- Rust bridge -------------------------------------------------------------
# Built into the CMake binary dir so the two build systems never disagree
# about where the archive is.
set(RUST_TARGET_DIR "${CMAKE_BINARY_DIR}/rust")
if(CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(CARGO_PROFILE_FLAG "")
    set(CARGO_PROFILE_DIR "debug")
else()
    set(CARGO_PROFILE_FLAG "--release")
    set(CARGO_PROFILE_DIR "release")
endif()

# `libfoo.a` everywhere Rust uses a Unix toolchain; `foo.lib` when it targets
# MSVC. Named from the toolchain rather than hardcoded, or the Windows link
# fails looking for a file cargo never writes.
if(MSVC)
    set(BRIDGE_LIB "${RUST_TARGET_DIR}/${CARGO_PROFILE_DIR}/jtf_qt6_bridge.lib")
else()
    set(BRIDGE_LIB "${RUST_TARGET_DIR}/${CARGO_PROFILE_DIR}/libjtf_qt6_bridge.a")
endif()
get_filename_component(REPO_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.." ABSOLUTE)


# Without an explicit dependency list the archive is built once and then
# considered up to date forever, so a new bridge function silently becomes an
# undefined symbol at link time.
file(GLOB_RECURSE BRIDGE_SOURCES CONFIGURE_DEPENDS
    "${REPO_ROOT}/src/*/src/*.rs"
    "${REPO_ROOT}/src/ui/qt6/bridge/src/*.rs"
    "${REPO_ROOT}/src/*/Cargo.toml")

add_custom_command(
    OUTPUT "${BRIDGE_LIB}"
    COMMAND ${CMAKE_COMMAND} -E env
            "CARGO_TARGET_DIR=${RUST_TARGET_DIR}"
            cargo build ${CARGO_PROFILE_FLAG} -p jtf-qt6-bridge
    WORKING_DIRECTORY "${REPO_ROOT}"
    DEPENDS ${BRIDGE_SOURCES}
    COMMENT "Building the Rust bridge"
    VERBATIM
    USES_TERMINAL)

add_custom_target(rust_bridge DEPENDS "${BRIDGE_LIB}")

# --- application -------------------------------------------------------------
qt_add_executable(jt-filework
    cpp/main.cpp
    cpp/mainwindow.cpp
    cpp/mainwindow.h
    cpp/panewidget.cpp
    cpp/panewidget.h
    cpp/filelistmodel.cpp
    cpp/filelistmodel.h
    cpp/iconprovider.cpp
    cpp/iconprovider.h
    cpp/foldertree.cpp
    cpp/foldertree.h
    cpp/breadcrumb.cpp
    cpp/breadcrumb.h
    cpp/inspector.cpp
    cpp/inspector.h
    cpp/placeslist.cpp
    cpp/placeslist.h
    cpp/headerview.cpp
    cpp/headerview.h
    cpp/modeswitch.cpp
    cpp/modeswitch.h
    cpp/keyhintbar.cpp
    cpp/keyhintbar.h
    cpp/thumbnails.cpp
    cpp/thumbnails.h
    cpp/matchdelegate.cpp
    cpp/rowdelegate.cpp
    cpp/dialogbuttons.cpp
    cpp/searchoverlay.cpp
    cpp/destinationdialog.cpp
    cpp/jobsdialog.cpp
    cpp/remotedialog.cpp
    cpp/matchdelegate.h
    cpp/shortcutsdialog.cpp
    cpp/shortcutsdialog.h
    cpp/theme.cpp
    cpp/theme.h
    cpp/watchdog.cpp
    cpp/watchdog.h
    cpp/icons.cpp
    cpp/icons.h
    cpp/operations.cpp
    cpp/operations.h
    cpp/settingsdialog.cpp
    cpp/settingsdialog.h
    cpp/batchrenamedialog.cpp
    cpp/batchrenamedialog.h
    cpp/commandpalette.cpp
    cpp/archivewindow.cpp
    cpp/comparewindow.cpp
    cpp/devicedelegate.cpp
    cpp/imagewriterdialog.cpp
    cpp/listlook.cpp
    cpp/usagewindow.cpp
    cpp/archivewindow.h
    cpp/aboutdialog.cpp
    cpp/aboutdialog.h
    cpp/commandpalette.h
    cpp/viewerwindow.cpp
    cpp/viewerwindow.h
    cpp/platform/quicklook.h
    cpp/platform/filetype.h
    cpp/bridge.h
    cpp/jtfstring.h)

# One platform implementation, chosen here rather than by an #ifdef inside a
# shared file.
if(APPLE)
    target_sources(jt-filework PRIVATE
        cpp/platform/quicklook_mac.mm
        cpp/platform/filetype_mac.mm
        cpp/platform/share_mac.mm)
    target_link_libraries(jt-filework PRIVATE "-framework UniformTypeIdentifiers")
else()
    target_sources(jt-filework PRIVATE
        cpp/platform/quicklook_stub.cpp
        cpp/platform/filetype_stub.cpp
        cpp/platform/share_stub.cpp)
endif()

# The Windows icon and version stamp travel in a resource script. Explorer,
# the taskbar and the Alt-Tab list read those from the executable itself and
# never run it to ask for a QIcon, so setting one at runtime is not enough.
if(WIN32)
    # The version stamp Explorer, the taskbar and the properties dialog read,
    # filled in from Cargo.toml rather than written out again by hand.
    string(REPLACE "." "," JTF_VERSION_COMMA "${JTF_VERSION}")
    # The .ico is generated from the master SVGs by assets/icon/build-icons.sh
    # and is deliberately not committed (`AGENTS.md` 2). `rc.exe` fails outright
    # on an icon it cannot open - RC2135, and the build stops - so a clone must
    # produce a resource script that does not mention one. The executable then
    # carries the toolkit's default icon in Explorer, which is a missing picture
    # rather than a build nobody outside this machine can run.
    set(JTF_ICON_PATH "${CMAKE_CURRENT_SOURCE_DIR}/../../../assets/icon/generated/jt-filework.ico")
    if(EXISTS "${JTF_ICON_PATH}")
        # Backslashes, because a resource script is not a CMake path.
        string(REPLACE "/" "\\\\" JTF_ICON_PATH "${JTF_ICON_PATH}")
        set(JTF_ICON_LINE "IDI_ICON1 ICON \"${JTF_ICON_PATH}\"")
    else()
        message(STATUS
            "jt-filework: no generated .ico; run assets/icon/build-icons.sh for "
            "an executable that carries its icon in Explorer")
        set(JTF_ICON_LINE "// no icon: assets/icon/generated/jt-filework.ico was not built")
    endif()
    configure_file(jt-filework.rc.in "${CMAKE_CURRENT_BINARY_DIR}/jt-filework.rc" @ONLY)
    target_sources(jt-filework PRIVATE "${CMAKE_CURRENT_BINARY_DIR}/jt-filework.rc")
    # `shlobj` for the shell's own name for the drives root.
    target_link_libraries(jt-filework PRIVATE shell32 ole32)
endif()

add_dependencies(jt-filework rust_bridge)
target_include_directories(jt-filework PRIVATE cpp)
target_link_libraries(jt-filework PRIVATE Qt6::Widgets Qt6::Svg "${BRIDGE_LIB}")
if(WIN32)
    # What the Rust standard library and its dependencies call into. A static
    # Rust archive carries no import list of its own, so these are named here
    # or the link ends in a page of undefined symbols with no obvious cause.
    target_link_libraries(jt-filework PRIVATE
        ws2_32 userenv ntdll bcrypt advapi32 secur32 crypt32 ncrypt)
endif()

if(APPLE)
    # Frameworks the Rust standard library needs when linked into a C++ binary.
    target_link_libraries(jt-filework PRIVATE
        "-framework CoreFoundation"
        "-framework Security"
        "-framework SystemConfiguration"
        "-framework AppKit"
        "-framework Quartz")
endif()

# --- hardening ---------------------------------------------------------------
# AGENTS.md 21: the C++ layer is the only memory-unsafe surface in the
# product, so it is compiled with every cheap mitigation available.
if(NOT MSVC)
    target_compile_options(jt-filework PRIVATE
        -Wall -Wextra -Wpedantic
        -fstack-protector-strong
        -D_FORTIFY_SOURCE=2)
else()
    target_compile_options(jt-filework PRIVATE /W4 /GS /guard:cf /sdl)
    target_link_options(jt-filework PRIVATE /DYNAMICBASE /NXCOMPAT /guard:cf)
endif()

# Opt-in sanitizer build for CI and for local debugging:
#   cmake -DJTF_SANITIZE=ON ...
option(JTF_SANITIZE "Build with AddressSanitizer and UndefinedBehaviorSanitizer" OFF)
if(JTF_SANITIZE AND NOT MSVC)
    target_compile_options(jt-filework PRIVATE -fsanitize=address,undefined -fno-omit-frame-pointer -g)
    target_link_options(jt-filework PRIVATE -fsanitize=address,undefined)
endif()

# macOS shows an icon for a bundle, not for a bare executable: a loose binary
# gets the generic document icon in the Dock however the icon file is named.
# The bundle is also what signing, notarization and the Dock name all attach
# to, so it is built from the start rather than bolted on at release time
# (docs/DISTRIBUTION.md).
if(APPLE)
    set(JTF_ICNS "${CMAKE_CURRENT_SOURCE_DIR}/../../../assets/icon/generated/jt-filework.icns")
    if(NOT EXISTS "${JTF_ICNS}")
        message(FATAL_ERROR
            "the application icon is missing: ${JTF_ICNS}\n"
            "run assets/icon/build-icons.sh to generate it")
    endif()
    # Copied into Contents/Resources by being a source file marked as one.
    target_sources(jt-filework PRIVATE "${JTF_ICNS}")
    set_source_files_properties("${JTF_ICNS}" PROPERTIES
        MACOSX_PACKAGE_LOCATION "Resources")

    set(MACOSX_BUNDLE_GUI_IDENTIFIER "tw.jasoncheng.jt-filework")
    set(MACOSX_BUNDLE_SHORT_VERSION_STRING "${JTF_VERSION}")
    set(MACOSX_BUNDLE_BUNDLE_VERSION "${JTF_VERSION}")

    set_target_properties(jt-filework PROPERTIES
        MACOSX_BUNDLE TRUE
        MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_SOURCE_DIR}/Info.plist.in")

    # The bundle carries its own locales and keymaps. Without this the app
    # only works while a source tree happens to sit above it, and the failure
    # is silent: every label falls back to its catalogue key.
    #
    # Attached to a target that is always out of date, not to POST_BUILD.
    # POST_BUILD runs only when the executable itself is relinked, so editing
    # a keymap and rebuilding copied nothing: the program went on reading the
    # previous file and the change looked like it had had no effect. Data is
    # not a build artifact of the code and must not depend on the code
    # changing.
    set(JTF_DATA_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../..")
    add_custom_target(jtf_bundle_data ALL
        COMMAND ${CMAKE_COMMAND} -E copy_directory
            "${JTF_DATA_ROOT}/locales"
            "$<TARGET_BUNDLE_CONTENT_DIR:jt-filework>/Resources/locales"
        COMMAND ${CMAKE_COMMAND} -E copy_directory
            "${JTF_DATA_ROOT}/keymaps"
            "$<TARGET_BUNDLE_CONTENT_DIR:jt-filework>/Resources/keymaps"
        COMMAND ${CMAKE_COMMAND} -E copy_directory
            "${JTF_DATA_ROOT}/assets/icons"
            "$<TARGET_BUNDLE_CONTENT_DIR:jt-filework>/Resources/icons"
        # The application's own icon, as PNGs. The .icns is what Finder and the
        # Dock read; Qt reads neither, so a window icon and the About box had
        # nothing to show.
        #
        # `copy_directory_if_different` rather than `copy_directory`: these are
        # generated from the master SVGs by assets/icon/build-icons.sh and are
        # deliberately not committed (`AGENTS.md` 2), so a fresh clone does not
        # have them and must still build. Without the icons the window simply
        # has none, which is a missing picture rather than a failed build.
        COMMAND ${CMAKE_COMMAND}
            -D "SRC=${JTF_DATA_ROOT}/assets/icon/generated/png"
            -D "DST=$<TARGET_BUNDLE_CONTENT_DIR:jt-filework>/Resources/appicon"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/cmake/copy-if-present.cmake"
        # The master SVG too, which unlike the PNGs is committed, so a
        # clone always has an icon to show.
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
            "${JTF_DATA_ROOT}/assets/icon/jt-filework.svg"
            "$<TARGET_BUNDLE_CONTENT_DIR:jt-filework>/Resources/appicon/jt-filework.svg"
        # Empty .lproj directories alongside the declaration above: some of
        # the system's lookups check for a real directory rather than reading
        # Info.plist, and an absent one silently means "not localized".
        COMMAND ${CMAKE_COMMAND} -E make_directory
            "$<TARGET_BUNDLE_CONTENT_DIR:jt-filework>/Resources/en.lproj"
        COMMAND ${CMAKE_COMMAND} -E make_directory
            "$<TARGET_BUNDLE_CONTENT_DIR:jt-filework>/Resources/zh-Hant.lproj"
        COMMENT "Copying locales and keymaps into the bundle")
    # After the bundle exists, or the copy has nowhere to land.
    add_dependencies(jtf_bundle_data jt-filework)
else()
    # No bundle to put the data in, so it goes beside the executable - which is
    # where `locate_repo_root` and `iconRoots` look before walking up a source
    # tree. Without this the program starts and shows every label as its
    # catalogue key, every shortcut blank and no icons at all, because the
    # catalogues, keymaps and icons simply are not there.
    set_target_properties(jt-filework PROPERTIES
        MACOSX_BUNDLE FALSE
        WIN32_EXECUTABLE FALSE)

    set(JTF_DATA_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../..")
    # Always out of date, for the reason given in the Apple branch above: a
    # POST_BUILD step runs only when the executable is relinked, so editing a
    # keymap and rebuilding copied nothing.
    add_custom_target(jtf_bundle_data ALL
        COMMAND ${CMAKE_COMMAND} -E copy_directory
            "${JTF_DATA_ROOT}/locales" "$<TARGET_FILE_DIR:jt-filework>/locales"
        COMMAND ${CMAKE_COMMAND} -E copy_directory
            "${JTF_DATA_ROOT}/keymaps" "$<TARGET_FILE_DIR:jt-filework>/keymaps"
        COMMAND ${CMAKE_COMMAND} -E copy_directory
            "${JTF_DATA_ROOT}/assets/icons" "$<TARGET_FILE_DIR:jt-filework>/icons"
        # Optional, for the reason given in the Apple branch above.
        COMMAND ${CMAKE_COMMAND}
            -D "SRC=${JTF_DATA_ROOT}/assets/icon/generated/png"
            -D "DST=$<TARGET_FILE_DIR:jt-filework>/appicon"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/cmake/copy-if-present.cmake"
        # The master SVG too, which unlike the PNGs is committed, so a
        # clone always has an icon to show.
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
            "${JTF_DATA_ROOT}/assets/icon/jt-filework.svg"
            "$<TARGET_FILE_DIR:jt-filework>/appicon/jt-filework.svg"
        COMMENT "Copying locales, keymaps and icons beside the executable")
    add_dependencies(jtf_bundle_data jt-filework)
endif()
