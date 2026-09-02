#!/usr/bin/env bash
#
# The folder the screenshots are taken in.
#
# Invented, not borrowed. A real folder belonging to a real person is full of
# customer names, order numbers and things marked "do not disclose", and a
# screenshot of one is one missed row away from publishing it. Redacting
# twenty-five rows also produces a picture that shows nothing.
#
# So the content here is made up, and made up to be *useful*: real files of
# real types and plausible sizes, so the type icons, the size column, the
# preview and the disc usage report are all showing genuine work rather than
# a mock-up. Same input every time, so a later release can retake the same
# screenshots and the only difference is the program.
#
#   scripts/make-demo-tree.sh [destination]      default: ~/jtf-demo
set -euo pipefail

ROOT="${1:-$HOME/jtf-demo}"
rm -rf "$ROOT"
mkdir -p "$ROOT"

say() { printf '  %s\n' "$1"; }

# ---------------------------------------------------------------- documents
mkdir -p "$ROOT/Documents/Reports" "$ROOT/Documents/Notes" "$ROOT/Documents/Contracts"
for n in "Q3 Capacity Review" "Network Segmentation Plan" "Backup Restore Drill" \
         "Storage Growth Forecast" "Incident 2026-041 Postmortem"; do
  printf '%%PDF-1.7\n%%\xe2\xe3\xcf\xd3\n' > "$ROOT/Documents/Reports/$n.pdf"
  head -c "$(( (RANDOM % 900 + 120) * 1024 ))" /dev/urandom >> "$ROOT/Documents/Reports/$n.pdf"
done
for n in "meeting-2026-08-14" "migration-checklist" "on-call-handover" "terminology"; do
  {
    echo "# ${n//-/ }"
    echo
    echo "Written while the work was happening, so it says what actually"
    echo "happened rather than what was planned."
    echo
    for i in 1 2 3 4 5 6; do echo "- point $i"; done
  } > "$ROOT/Documents/Notes/$n.md"
done
printf 'Supplier,Contact,Renewal,Value\n' > "$ROOT/Documents/Contracts/renewals.csv"
for i in $(seq 1 40); do
  printf 'Northwind Systems %02d,ops@example.invalid,2027-0%d-15,%d\n' \
    "$i" "$(( i % 9 + 1 ))" "$(( i * 1250 ))" >> "$ROOT/Documents/Contracts/renewals.csv"
done

# ------------------------------------------------------------------ pictures
mkdir -p "$ROOT/Pictures/Screenshots" "$ROOT/Pictures/Diagrams"
if command -v sips >/dev/null 2>&1; then
  # A real PNG of a real size, so the thumbnail and the preview have something.
  for n in rack-elevation topology-2026 wiring-closet cable-map; do
    /usr/bin/python3 - "$ROOT/Pictures/Diagrams/$n.png" <<'PY'
import struct, sys, zlib, random
w = h = 512
random.seed(len(sys.argv[1]))
rows = b"".join(
    b"\x00" + bytes(
        v for x in range(w)
        for v in ((x * 7 + y * 3) % 256, (x * 3) % 256, (y * 5) % 256)
    )
    for y in range(h)
)
def chunk(tag, data):
    return (struct.pack(">I", len(data)) + tag + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xffffffff))
png = (b"\x89PNG\r\n\x1a\n"
       + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
       + chunk(b"IDAT", zlib.compress(rows, 6))
       + chunk(b"IEND", b""))
open(sys.argv[1], "wb").write(png)
PY
  done
fi
for i in 01 02 03 04 05 06 07 08; do
  head -c "$(( (RANDOM % 1800 + 400) * 1024 ))" /dev/urandom \
    > "$ROOT/Pictures/Screenshots/capture-2026-08-$i.jpg"
  printf '\xff\xd8\xff' | dd of="$ROOT/Pictures/Screenshots/capture-2026-08-$i.jpg" \
    conv=notrunc bs=1 count=3 status=none
done

# ------------------------------------------------------------------ archives
mkdir -p "$ROOT/Archives" "$ROOT/.staging/site"
for i in $(seq 1 12); do
  echo "page $i" > "$ROOT/.staging/site/page-$i.html"
done
head -c 400000 /dev/urandom > "$ROOT/.staging/site/assets.bin"
# `COPYFILE_DISABLE` and `--no-xattrs`: macOS's tar writes an AppleDouble
# `._name` beside every entry, and zip writes a `__MACOSX` folder. Both are
# real, both are noise, and an archive listing full of them says more about
# the machine that made it than about the program showing it.
( cd "$ROOT/.staging" && zip -qr -X "$ROOT/Archives/site-backup-2026-08.zip" site )
( cd "$ROOT/.staging" && COPYFILE_DISABLE=1 tar czf "$ROOT/Archives/site-backup-2026-08.tar.gz" site )
( cd "$ROOT/.staging" && COPYFILE_DISABLE=1 tar cJf "$ROOT/Archives/configs.tar.xz" site 2>/dev/null || true )
rm -rf "$ROOT/.staging"

# ---------------------------------------------------------------------- code
mkdir -p "$ROOT/Projects/inventory/src" "$ROOT/Projects/inventory/tests"
cat > "$ROOT/Projects/inventory/src/main.rs" <<'RS'
//! A small inventory service, for the screenshot's sake.

fn main() {
    let racks = load_racks("racks.toml").expect("racks");
    for rack in &racks {
        println!("{:>4}  {:<24} {:>3} U free", rack.id, rack.name, rack.free_units());
    }
}
RS
for n in units racks power; do
  printf '[%s]\ncount = 42\nlabel = "%s"\n' "$n" "$n" > "$ROOT/Projects/inventory/$n.toml"
done
echo 'fn main() {}' > "$ROOT/Projects/inventory/tests/smoke.rs"
printf '#!/bin/sh\nset -eu\nexec cargo run --release -- "$@"\n' > "$ROOT/Projects/inventory/run.sh"
chmod +x "$ROOT/Projects/inventory/run.sh"

# ---------------------------------------------------------------------- logs
mkdir -p "$ROOT/Logs"
for d in 08-28 08-29 08-30 08-31; do
  {
    for i in $(seq 1 4000); do
      printf '2026-%s %02d:%02d:%02d  INFO   worker-%d  handled request %d in %dms\n' \
        "$d" "$(( i % 24 ))" "$(( i % 60 ))" "$(( (i * 7) % 60 ))" \
        "$(( i % 8 ))" "$i" "$(( i % 400 + 5 ))"
    done
  } > "$ROOT/Logs/service-2026-$d.log"
done
head -c 24000000 /dev/urandom > "$ROOT/Logs/capture-2026-08-31.pcap"

# --------------------------------------------------------------- disc images
mkdir -p "$ROOT/Images"
head -c "$(( 180 * 1024 * 1024 ))" /dev/zero > "$ROOT/Images/toolkit-2026.iso"

# ------------------------------------------------------------------ the rest
mkdir -p "$ROOT/Downloads"
for n in "installer-3.4.1" "driver-bundle" "release-notes"; do
  head -c "$(( (RANDOM % 3000 + 500) * 1024 ))" /dev/urandom > "$ROOT/Downloads/$n.bin"
done

printf 'Made at %s\n' "$ROOT"
du -sh "$ROOT"
find "$ROOT" -type f | wc -l | sed 's/^/  files: /'
