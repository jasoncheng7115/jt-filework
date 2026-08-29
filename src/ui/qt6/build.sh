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

build_dir="$here/build/$mode"

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

echo
echo "built: $build_dir/jt-filework"
if [ "${JTF_NO_RUN:-0}" != "1" ]; then
    exec "$build_dir/jt-filework"
fi
