#!/usr/bin/env bash
#
# Build the Debian / Ubuntu package: <dist>/jt-filework_<version>_<arch>.deb
#
# Run on the oldest distribution the project supports - Ubuntu 22.04 today -
# because glibc compatibility runs one way: a package built there installs
# on 24.04, and one built on 24.04 does not install on 22.04
# (docs/RELEASE_CHECKLIST.md 3).
#
# Qt comes from the distribution, not from inside the package. The library
# dependencies are worked out by dpkg-shlibdeps from what the binary actually
# links. The plugins Qt loads at run time - the X11 platform, the SVG icon
# engine - are not linked, so shlibdeps cannot see them; they are named here,
# and without them the program either will not start or draws no icons.
#
# Layout: the program and its data (catalogues, keymaps, icons) together in
# /usr/lib/jt-filework, which is where the program looks for them - beside
# its own executable - and a link in /usr/bin.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
here="$root/packaging/linux"
version=$(awk -F'"' '/^version = "/ {print $2; exit}' "$root/Cargo.toml")
debarch="$(dpkg --print-architecture)"
build_root="${JTF_BUILD_ROOT:-$HOME/.cache/jt-filework-qt}"
build="$build_root/package"
dist="${JTF_DIST:-$build_root/dist}"
name="jt-filework_${version}_${debarch}"

fail() { printf '\033[31mFAILED: %s\033[0m\n' "$1" >&2; exit 1; }
step() { printf '\n\033[1m== %s\033[0m\n' "$1"; }
export PATH="$HOME/.cargo/bin:$PATH"

step "building $version for $debarch on $(lsb_release -ds 2>/dev/null || uname -sr)"
cmake -S "$root/src/ui/qt6" -B "$build" -DCMAKE_BUILD_TYPE=Release >/dev/null
cmake --build "$build" --parallel
exe="$build/jt-filework"
[ -x "$exe" ] || fail "no executable at $exe"
[ -f "$build/locales/en/main.catalog" ] \
    || fail "no catalogue beside the executable; every label would show its key"
# The number the program shows is compiled into its Rust half; a build that
# reused an older library would install as one version and call itself another.
# Read directly, not piped from `strings`: under pipefail, `grep -q` quitting
# at the first match is a SIGPIPE upstream, and a correct build fails.
grep -aqF -- "$version" "$exe" \
    || fail "the program was not built as $version; it would show another version"

step "laying out the package"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
pkg="$work/$name"
lib="$pkg/usr/lib/jt-filework"
install -Dm755 "$exe" "$lib/jt-filework"
# Symbols are for a debugger, not for everyone who installs it; they are
# most of the file.
strip --strip-unneeded "$lib/jt-filework"
for data in locales keymaps icons appicon; do
    [ -d "$build/$data" ] && cp -R "$build/$data" "$lib/"
done
install -d "$pkg/usr/bin"
ln -s ../lib/jt-filework/jt-filework "$pkg/usr/bin/jt-filework"

install -Dm644 "$here/jt-filework.desktop" "$pkg/usr/share/applications/jt-filework.desktop"
install -Dm644 "$root/assets/icon/jt-filework.svg" \
    "$pkg/usr/share/icons/hicolor/scalable/apps/jt-filework.svg"
for size in 16 24 32 48 64 128 256 512; do
    png="$root/assets/icon/generated/png/jt-filework-$size.png"
    if [ -f "$png" ]; then
        install -Dm644 "$png" "$pkg/usr/share/icons/hicolor/${size}x${size}/apps/jt-filework.png"
    fi
done

install -d "$pkg/usr/share/doc/jt-filework"
cat > "$pkg/usr/share/doc/jt-filework/copyright" <<EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: jt-filework
Source: https://github.com/jasoncheng7115/jt-filework

Files: *
Copyright: Jason Cheng (Jason Tools)
License: GPL-3.0-or-later
 On Debian systems, the full text of the GNU General Public License
 version 3 can be found in /usr/share/common-licenses/GPL-3.

Files: usr/lib/jt-filework/icons/iconoir/*
Copyright: 2021 Luca Burgio
License: MIT
EOF

step "working out what it depends on"
# dpkg-shlibdeps wants to be run from a source package; a control file with
# the one stanza it reads is enough.
mkdir -p "$work/src/debian"
printf 'Source: jt-filework\n\nPackage: jt-filework\nArchitecture: any\n' \
    > "$work/src/debian/control"
shlibs=$(cd "$work/src" && dpkg-shlibdeps -O -e"$lib/jt-filework" 2>/dev/null \
    | sed -n 's/^shlibs:Depends=//p')
[ -n "$shlibs" ] || fail "dpkg-shlibdeps found no dependencies"
depends="$shlibs"
for plugin_package in qt6-qpa-plugins libqt6svg6; do
    case ", $depends," in
        *", $plugin_package "*|*", $plugin_package,"*) ;;
        *) depends="$depends, $plugin_package" ;;
    esac
done
echo "  $depends"

installed=$(du -sk "$pkg/usr" | cut -f1)
install -d "$pkg/DEBIAN"
cat > "$pkg/DEBIAN/control" <<EOF
Package: jt-filework
Version: $version
Section: utils
Priority: optional
Architecture: $debarch
Maintainer: Jason Cheng (Jason Tools) <sanyu3u@gmail.com>
Installed-Size: $installed
Depends: $depends
Recommends: qt6-wayland
Homepage: https://github.com/jasoncheng7115/jt-filework
Description: keyboard-first file manager with multiple panes
 A file manager built to be driven from the keyboard: recursive split panes
 with tabs of their own, CView-style marking, a filter and a recursive
 search, archives and ISO images browsed like folders, SFTP locations, and
 disk image writing. Every command is also on a menu.
EOF

step "building the package"
mkdir -p "$dist"
rm -f "$dist/$name.deb"
dpkg-deb --build --root-owner-group -Zxz "$pkg" "$dist/$name.deb" >/dev/null
dpkg-deb --info "$dist/$name.deb" >/dev/null || fail "the package does not read back"

( cd "$dist" && sha256sum "$name.deb" > "$name.deb.sha256" )
printf '\n\033[32m%s\033[0m\n' "$dist/$name.deb"
cat "$dist/$name.deb.sha256"
