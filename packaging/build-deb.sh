#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | cut -d '"' -f2)"
ARCH="$(dpkg --print-architecture)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
STAGE="$(mktemp -d -t knightgambit-deb-XXXXXX)"

cargo build --release --manifest-path "$ROOT/Cargo.toml"

make -C "$ROOT" install DESTDIR="$STAGE" PREFIX=/usr TARGET_DIR="$TARGET_DIR" CARGO=true
install -Dm644 "$ROOT/LICENSE" "$STAGE/usr/share/doc/knightgambit/copyright"
install -Dm644 "$ROOT/THIRD_PARTY.md" "$STAGE/usr/share/doc/knightgambit/THIRD_PARTY.md"
install -Dm644 "$ROOT/README.md" "$STAGE/usr/share/doc/knightgambit/README.md"

INSTALLED_SIZE="$(du -sk "$STAGE/usr" | cut -f1)"
mkdir -p "$STAGE/DEBIAN"
cat > "$STAGE/DEBIAN/control" <<CONTROL
Package: knightgambit
Version: $VERSION
Section: games
Priority: optional
Architecture: $ARCH
Installed-Size: $INSTALLED_SIZE
Depends: libc6 (>= 2.35), libgcc-s1, libgtk-4-1 (>= 4.14), libadwaita-1-0 (>= 1.5), libpango-1.0-0, libcairo2, libgdk-pixbuf-2.0-0, libgraphene-1.0-0
Recommends: pipewire-bin | pulseaudio-utils | alsa-utils
Suggests: stockfish, make, gcc, meson, ninja-build, libopenblas-dev
Maintainer: Junaid Ul Hassan <knightgambit@users.noreply.github.com>
Homepage: https://github.com/knightgambit/KnightGambit
Description: Premium offline chess for the GNOME desktop
 KnightGambit is a GTK4 and libadwaita chess application with original
 Staunton-inspired vector pieces, complete chess rules, chess clocks,
 move review, PGN and FEN support, and integration with the open-source
 UCI engines Stockfish, Berserk, Ethereal, Koivisto and Leela Chess Zero.
CONTROL

cat > "$STAGE/DEBIAN/postinst" <<'POSTINST'
#!/bin/sh
set -e
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || true
fi
POSTINST
cp "$STAGE/DEBIAN/postinst" "$STAGE/DEBIAN/postrm"
chmod 755 "$STAGE/DEBIAN/postinst" "$STAGE/DEBIAN/postrm"

mkdir -p "$ROOT/dist"
OUTPUT="$ROOT/dist/knightgambit_${VERSION}_${ARCH}.deb"
dpkg-deb --root-owner-group -Zxz --build "$STAGE" "$OUTPUT"
echo "Built $OUTPUT"
echo "Staging directory left at $STAGE"
