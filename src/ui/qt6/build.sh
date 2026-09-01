#!/usr/bin/env bash
# Build and run the Qt 6 PoC.
#
#   ./src/ui/qt6/build.sh            debug build, then run
#   ./src/ui/qt6/build.sh release    optimised build, then run
#   ./src/ui/qt6/build.sh asan       sanitizer build, then run
#
# Requires Qt 6 and CMake. On an Apple Silicon Mac, Qt must be the arm64
# build: /usr/local Homebrew is the Rosetta one and its Qt will not link
# against an aarch64 Rust archive.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
mode="${1:-debug}"

case "$mode" in
    release) build_type=Release; extra=() ;;
    asan)    build_type=Debug;   extra=("-DJTF_SANITIZE=ON") ;;
    *)       build_type=Debug;   extra=() ;;
esac

# Out of the source tree, for the same reason .cargo/config.toml puts the Rust
# target directory there: this checkout lives in a synced folder, and 4 GB of
# object files re-uploading on every build is not something a build should do
# to somebody's network. Override with JTF_BUILD_ROOT.
build_root="${JTF_BUILD_ROOT:-$HOME/.cache/jt-filework-qt}"
build_dir="$build_root/$mode"

# Prefer an arm64 Qt when one is present.
for prefix in /opt/homebrew/opt/qt /usr/local/opt/qt; do
    if [ -x "$prefix/bin/qmake6" ]; then
        export CMAKE_PREFIX_PATH="$prefix${CMAKE_PREFIX_PATH:+:$CMAKE_PREFIX_PATH}"
        export PATH="$prefix/bin:$PATH"
        break
    fi
done
export PATH="$HOME/.cargo/bin:$PATH"

# On an Apple Silicon Mac the Rosetta Homebrew at /usr/local ships an x86_64
# cmake, which then defaults the build to x86_64 and silently fails to link
# against an arm64 Qt and an aarch64 Rust archive. Pin the architecture to
# whatever the Rust toolchain is actually producing.
if [ "$(uname -s)" = "Darwin" ]; then
    host_arch="$(uname -m)"
    extra+=("-DCMAKE_OSX_ARCHITECTURES=$host_arch")
fi

cmake -S "$here" -B "$build_dir" -DCMAKE_BUILD_TYPE="$build_type" ${extra[@]+"${extra[@]}"}
cmake --build "$build_dir" --parallel

# On macOS the product is an .app; everywhere else it is the executable.
app="$build_dir/jt-filework"
if [ -d "$build_dir/jt-filework.app" ]; then
    app="$build_dir/jt-filework.app/Contents/MacOS/jt-filework"
fi

# A release build also becomes /Applications/jt-filework.app.
#
# The Dock pins a path, not a project. Pinned straight at a build directory it
# kept whichever configuration was pinned first - which is how the Dock icon
# ended up launching a stale debug build while the release one ran beside it,
# two icons for one application because macOS groups by bundle path. One
# install location, refreshed by every release build, is a path worth pinning.
if [ "$mode" = "release" ] && [ -d "$build_dir/jt-filework.app" ]; then
    installed="/Applications/jt-filework.app"
    # Removed rather than copied over: a stale file left from an older build
    # inside a bundle is the kind of thing that only shows up much later.
    rm -rf "$installed"
    if cp -R "$build_dir/jt-filework.app" "$installed" 2>/dev/null; then
        echo "installed: $installed"
        app="$installed/Contents/MacOS/jt-filework"
    else
        echo "note: could not write $installed; leaving it as it was" >&2
    fi
fi

echo
echo "built: $app"
if [ "${JTF_NO_RUN:-0}" != "1" ]; then
    exec "$app"
fi
