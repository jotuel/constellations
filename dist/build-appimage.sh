#!/usr/bin/env bash
#
# Build a Constellations AppImage.
#
# Produces Constellations-<arch>-<version>.AppImage in dist/.
#
# Requirements:
#   - rustup + a recent stable Rust toolchain
#   - system deps for libcosmic (see README "Build")
#   - appimagetool and linuxdeploy on $PATH
#     (https://appimage.org/, or installed via `appimagetool` AUR / download
#      the AppImage releases and chmod +x them)
#   - rsvg-convert (librsvg) to rasterize the SVG icon
#
# Usage:
#   ./dist/build-appimage.sh            # build release binary + AppImage
#   ./dist/build-appimage.sh --skip-build  # skip `cargo build`, reuse target/release
set -euo pipefail

cd "$(dirname "$0")/.."

APP_NAME="Constellations"
APP_ID="fi.joonastuomi.Constellations"
BIN_NAME="constellations"
# Read the version straight from Cargo.toml.
VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
ARCH="$(uname -m)"

APPDIR="dist/${APP_NAME}.AppDir"
OUT="dist/${APP_NAME}-${ARCH}-${VERSION}.AppImage"

log() { printf '\033[1;34m[appimage]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[appimage error]\033[0m %s\n' "$*" >&2; exit 1; }

command -v appimagetool >/dev/null || die "appimagetool not found on PATH."
command -v linuxdeploy  >/dev/null || die "linuxdeploy not found on PATH."
command -v rsvg-convert >/dev/null || die "rsvg-convert not found on PATH."

if [[ "${1:-}" != "--skip-build" ]]; then
  log "Building release binary…"
  cargo build --release
fi
[[ -f "target/release/${BIN_NAME}" ]] || die "release binary not found; run without --skip-build."

log "Preparing AppDir at ${APPDIR}…"
rm -rf "${APPDIR}"
mkdir -p "${APPDIR}/usr/bin" \
         "${APPDIR}/usr/share/applications" \
         "${APPDIR}/usr/share/icons/hicolor/512x512/apps" \
         "${APPDIR}/usr/share/icons/hicolor/scalable/apps" \
         "${APPDIR}/usr/share/metainfo"

install -Dm755 "target/release/${BIN_NAME}" "${APPDIR}/usr/bin/${BIN_NAME}"
install -Dm644 "res/${APP_ID}.desktop"      "${APPDIR}/usr/share/applications/${APP_ID}.desktop"
install -Dm644 "res/${APP_ID}.metainfo.xml" "${APPDIR}/usr/share/metainfo/${APP_ID}.metainfo.xml"

# Scalable SVG + a rasterized 512px PNG (covers most launchers).
install -Dm644 "res/const.svg" "${APPDIR}/usr/share/icons/hicolor/scalable/apps/${APP_ID}.svg"
rsvg-convert -w 512 -h 512 "res/const.svg" \
  -o "${APPDIR}/usr/share/icons/hicolor/512x512/apps/${APP_ID}.png"

# Top-level AppDir entry points (AppImage runtime conventions).
cp "${APPDIR}/usr/share/applications/${APP_ID}.desktop" "${APPDIR}/${APP_ID}.desktop"
cp "${APPDIR}/usr/share/icons/hicolor/512x512/apps/${APP_ID}.png" "${APPDIR}/${APP_ID}.png"

# AppRun -> AppImage's runtime exec. We point it at our binary.
cat >"${APPDIR}/AppRun" <<EOF
#!/usr/bin/env bash
HERE="\$(dirname "\$(readlink -f "\${0}")")"
exec "\${HERE}/usr/bin/${BIN_NAME}" "\$@"
EOF
chmod +x "${APPDIR}/AppRun"

log "Bundling runtime deps with linuxdeploy…"
# Constellations is almost fully statically linked (Rust crates link
# statically); only a couple of system .so files (sqlite3, xkbcommon) remain
# dynamic. linuxdeploy resolves those into the AppDir and sets rpath. The
# bundled strip in linuxdeploy is older than the toolchain here and chokes on
# `.relr.dyn` sections, so disable it via NO_STRIP to avoid noisy errors.
export NO_STRIP=1
LINUXDEPLOY_ARGS=(
  --appdir "${APPDIR}"
  --desktop-file "${APPDIR}/usr/share/applications/${APP_ID}.desktop"
  --icon-file "${APPDIR}/usr/share/icons/hicolor/512x512/apps/${APP_ID}.png"
)
# The gtk plugin is optional — only useful if GObject schemas/GI typelibs are
# needed at runtime (they are not for this pure-Rust app). Use it if present.
if command -v linuxdeploy-plugin-gtk >/dev/null 2>&1; then
  LINUXDEPLOY_ARGS+=(--plugin gtk)
  export DEPLOY_GTK_VERSION=3
fi
linuxdeploy "${LINUXDEPLOY_ARGS[@]}"

log "Finalizing with appimagetool…"
rm -f "${OUT}"
appimagetool "${APPDIR}" "${OUT}"

log "Done: ${OUT}"
