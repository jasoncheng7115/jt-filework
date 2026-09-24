#!/usr/bin/env bash
#
# Build the macOS disk image: dist/jt-filework-<version>-macos-<arch>.dmg
#
# What `build.sh release` makes runs on this Mac only: it links Qt from
# Homebrew, at paths that exist here and nowhere else. This copies Qt into
# the bundle (macdeployqt), signs the result so Apple Silicon will execute
# it, and refuses to go on if anything in the bundle still points outside it.
#
# The signature is ad hoc, not a Developer ID, and the image is not
# notarized (docs/DISTRIBUTION.md 1.1). Anyone who downloads it will be
# stopped by Gatekeeper once and has to allow it in System Settings. That is
# said in the release notes rather than hidden.
#
# Built with its own build directory, not build.sh's: build.sh installs into
# /Applications as a side effect, and packaging must not replace the copy
# somebody is using.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
version=$(awk -F'"' '/^version = "/ {print $2; exit}' "$root/Cargo.toml")
arch="$(uname -m)"
# Beside the build, not in the source tree: a source tree inside a synced
# folder would upload every image made in it.
build_root="${JTF_BUILD_ROOT:-$HOME/.cache/jt-filework-qt}"
build="$build_root/package"
dist="${JTF_DIST:-$build_root/dist}"
name="jt-filework-$version-macos-$arch"

fail() { printf '\033[31mFAILED: %s\033[0m\n' "$1" >&2; exit 1; }
step() { printf '\n\033[1m== %s\033[0m\n' "$1"; }

qt=""
for prefix in /opt/homebrew/opt/qt /usr/local/opt/qt; do
    if [ -x "$prefix/bin/macdeployqt" ]; then qt="$prefix"; break; fi
done
[ -n "$qt" ] || fail "no Qt with macdeployqt under /opt/homebrew or /usr/local"
export PATH="$HOME/.cargo/bin:$qt/bin:$PATH"

# The oldest macOS it will run on. Qt from Homebrew is built for 14.0, so the
# program cannot promise anything earlier; and it must promise it everywhere,
# or the C inside the Rust half is compiled for whatever the SDK is - 26.2 on
# this machine - and the linker warns that the result may not start on the
# version the bundle claims.
minimum="${JTF_MACOS_MINIMUM:-14.0}"
export MACOSX_DEPLOYMENT_TARGET="$minimum"

step "building $version for $arch, macOS $minimum and later"
cmake -S "$root/src/ui/qt6" -B "$build" -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_PREFIX_PATH="$qt" -DCMAKE_OSX_ARCHITECTURES="$arch" \
    -DCMAKE_OSX_DEPLOYMENT_TARGET="$minimum" >/dev/null
cmake --build "$build" --parallel

app="$build/jt-filework.app"
[ -d "$app" ] || fail "no bundle at $app"
[ -f "$app/Contents/Resources/locales/en/main.catalog" ] \
    || fail "the bundle has no catalogue; every label would show its key"

step "copying Qt into the bundle"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
bundle="$stage/jt-filework.app"
cp -R "$app" "$bundle"

# The plugins are chosen here rather than left to macdeployqt, which copies
# every one Homebrew has - including a PDF image plugin that needs a
# framework from another formula, and a virtual keyboard that brings QML and
# Quick with it: forty megabytes of things this program never loads.
# Offscreen is kept because it is how a build is started without a screen.
# Asked of Qt rather than worked out from where macdeployqt lives: Homebrew
# splits Qt into formulas, and the SVG plugins are not in qtbase's.
plugins="$("$qt/bin/qtpaths6" --query QT_INSTALL_PLUGINS 2>/dev/null || true)"
[ -d "$plugins" ] || fail "cannot find Qt's plugins (qtpaths6 --query QT_INSTALL_PLUGINS)"
wanted=(
    platforms/libqcocoa.dylib
    platforms/libqoffscreen.dylib
    styles/libqmacstyle.dylib
    iconengines/libqsvgicon.dylib
    imageformats/libqsvg.dylib
    imageformats/libqjpeg.dylib
    imageformats/libqgif.dylib
    imageformats/libqwebp.dylib
    imageformats/libqtiff.dylib
    imageformats/libqico.dylib
    imageformats/libqicns.dylib
    imageformats/libqmacheif.dylib
)
extra=()
for plugin in "${wanted[@]}"; do
    [ -f "$plugins/$plugin" ] || fail "no $plugin under $plugins"
    install -d "$bundle/Contents/PlugIns/$(dirname "$plugin")"
    cp "$plugins/$plugin" "$bundle/Contents/PlugIns/$plugin"
    extra+=("-executable=$bundle/Contents/PlugIns/$plugin")
done
macdeployqt "$bundle" -no-plugins -always-overwrite "${extra[@]}" >/dev/null
# Where Qt is to look for them, instead of the path it was built with - which
# on this machine is Homebrew's, and would quietly load those instead.
printf '[Paths]\nPlugins = PlugIns\n' > "$bundle/Contents/Resources/qt.conf"

# Every library the bundle loads must be in the bundle or part of the system.
# A bundle that reaches outside itself runs perfectly on the machine that
# built it and fails to start everywhere else, which is the one place it is
# never tested. Only load commands are read: a library's own install name is
# not something anything loads.
step "checking every binary loads only from the bundle or the system"
leaks=0
while IFS= read -r -d '' file; do
    file "$file" | grep -q 'Mach-O' || continue
    dir="$(dirname "$file")"
    while IFS= read -r dep; do
        case "$dep" in
            /System/*|/usr/lib/*) ;;
            @executable_path/*) resolved="$bundle/Contents/MacOS/${dep#@executable_path/}"
                [ -e "$resolved" ] || { echo "  $file -> $dep (missing)" >&2; leaks=1; } ;;
            @loader_path/*) resolved="$dir/${dep#@loader_path/}"
                [ -e "$resolved" ] || { echo "  $file -> $dep (missing)" >&2; leaks=1; } ;;
            @rpath/*) resolved="$bundle/Contents/Frameworks/${dep#@rpath/}"
                [ -e "$resolved" ] || { echo "  $file -> $dep (not in Frameworks)" >&2; leaks=1; } ;;
            *) echo "  $file -> $dep" >&2; leaks=1 ;;
        esac
    done < <(otool -l "$file" | awk '/cmd LC_(LOAD|LOAD_WEAK|REEXPORT)_DYLIB$/ {getline; getline; print $2}')
done < <(find "$bundle" -type f -print0)
[ "$leaks" -eq 0 ] || fail "the bundle depends on libraries outside it"

step "signing (ad hoc)"
# macdeployqt rewrote every load path, which voids the signatures the linker
# made. An unsigned binary will not execute at all on Apple Silicon.
codesign --force --deep --sign - "$bundle"
codesign --verify --deep --strict "$bundle" || fail "signature does not verify"

step "making the disk image"
mkdir -p "$dist"
image="$stage/image"
mkdir -p "$image"
mv "$bundle" "$image/"
ln -s /Applications "$image/Applications"
rm -f "$dist/$name.dmg"
hdiutil create -volname "jt-filework $version" -srcfolder "$image" \
    -fs HFS+ -format UDZO -imagekey zlib-level=9 -ov "$dist/$name.dmg" >/dev/null
hdiutil verify "$dist/$name.dmg" >/dev/null || fail "the image does not verify"

( cd "$dist" && shasum -a 256 "$name.dmg" > "$name.dmg.sha256" )
printf '\n\033[32m%s\033[0m\n' "$dist/$name.dmg"
cat "$dist/$name.dmg.sha256"
