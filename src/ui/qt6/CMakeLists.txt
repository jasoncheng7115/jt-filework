# Qt 6 Widgets front end for jt-filework (Phase 0B PoC, ADR-0001).
#
# The C++ here is the UI layer only. All logic lives in the Rust crates and is
# reached through the C ABI in bridge/ - AGENTS.md 4 keeps the toolkit
# replaceable, and this build file is the seam.

cmake_minimum_required(VERSION 3.21)

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
qt_standard_project_setup()

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

set(BRIDGE_LIB "${RUST_TARGET_DIR}/${CARGO_PROFILE_DIR}/libjtf_qt6_bridge.a")
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
        cpp/platform/filetype_mac.mm)
    target_link_libraries(jt-filework PRIVATE "-framework UniformTypeIdentifiers")
else()
    target_sources(jt-filework PRIVATE
        cpp/platform/quicklook_stub.cpp
        cpp/platform/filetype_stub.cpp)
endif()

add_dependencies(jt-filework rust_bridge)
target_include_directories(jt-filework PRIVATE cpp)
target_link_libraries(jt-filework PRIVATE Qt6::Widgets Qt6::Svg "${BRIDGE_LIB}")

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
    set(MACOSX_BUNDLE_SHORT_VERSION_STRING "0.1.0")
    set(MACOSX_BUNDLE_BUNDLE_VERSION "0.1.0")

    set_target_properties(jt-filework PROPERTIES
        MACOSX_BUNDLE TRUE
        MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_SOURCE_DIR}/Info.plist.in")

    # The bundle carries its own locales and keymaps. Without this the app
    # only works while a source tree happens to sit above it, and the failure
    # is silent: every label falls back to its catalogue key.
    set(JTF_DATA_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../..")
    add_custom_command(TARGET jt-filework POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_directory
            "${JTF_DATA_ROOT}/locales"
            "$<TARGET_BUNDLE_CONTENT_DIR:jt-filework>/Resources/locales"
        COMMAND ${CMAKE_COMMAND} -E copy_directory
            "${JTF_DATA_ROOT}/keymaps"
            "$<TARGET_BUNDLE_CONTENT_DIR:jt-filework>/Resources/keymaps"
        COMMAND ${CMAKE_COMMAND} -E copy_directory
            "${JTF_DATA_ROOT}/assets/icons"
            "$<TARGET_BUNDLE_CONTENT_DIR:jt-filework>/Resources/icons"
        # Empty .lproj directories alongside the declaration above: some of
        # the system's lookups check for a real directory rather than reading
        # Info.plist, and an absent one silently means "not localized".
        COMMAND ${CMAKE_COMMAND} -E make_directory
            "$<TARGET_BUNDLE_CONTENT_DIR:jt-filework>/Resources/en.lproj"
        COMMAND ${CMAKE_COMMAND} -E make_directory
            "$<TARGET_BUNDLE_CONTENT_DIR:jt-filework>/Resources/zh-Hant.lproj"
        COMMENT "Copying locales and keymaps into the bundle")
else()
    set_target_properties(jt-filework PROPERTIES
        MACOSX_BUNDLE FALSE
        WIN32_EXECUTABLE FALSE)
endif()
