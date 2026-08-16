#!/usr/bin/env bash
# Build native tar.gz, Debian (.deb), and RPM (.rpm) packages for LAR.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
HOST_ARCH="$(uname -m)"
case "$HOST_ARCH" in
  x86_64) DEB_ARCH=amd64 ;;
  aarch64|arm64) DEB_ARCH=arm64 ;;
  *) DEB_ARCH="$HOST_ARCH" ;;
esac

TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
DIST="${DIST:-$ROOT/dist}"
STAGING="$ROOT/packaging/staging"

rm -rf "$DIST" "$STAGING"
mkdir -p "$DIST" "$STAGING/bin"

echo "==> Building release binaries (lar, lar-exec)"
cargo build --release -p lar -p lar-exec

LAR_BIN="$TARGET_DIR/release/lar"
EXEC_BIN="$TARGET_DIR/release/lar-exec"
if [[ ! -x "$LAR_BIN" || ! -x "$EXEC_BIN" ]]; then
  echo "missing release binaries under $TARGET_DIR/release" >&2
  exit 1
fi

install -m 755 "$LAR_BIN" "$STAGING/bin/lar"
install -m 755 "$EXEC_BIN" "$STAGING/bin/lar-exec"

echo "==> Native tarball"
STAGE_NAME="lar-${VERSION}-linux-${HOST_ARCH}"
STAGE="$DIST/$STAGE_NAME"
mkdir -p "$STAGE/bin" "$STAGE/share/doc/lar"
install -m 755 "$STAGING/bin/lar" "$STAGE/bin/lar"
install -m 755 "$STAGING/bin/lar-exec" "$STAGE/bin/lar-exec"
install -m 644 "$ROOT/README.md" "$STAGE/share/doc/lar/README.md"
install -m 644 "$ROOT/LICENSE" "$STAGE/share/doc/lar/LICENSE"
install -m 644 "$ROOT/docs/releases/${VERSION}.md" "$STAGE/share/doc/lar/RELEASE-${VERSION}.md"
cat >"$STAGE/INSTALL.txt" <<EOF
LAR ${VERSION} — native binary archive

1. Copy bin/lar and bin/lar-exec onto the same directory on PATH, e.g.:
     sudo install -m 755 bin/lar bin/lar-exec /usr/local/bin/
2. Keep both binaries together so lar finds lar-exec as a sibling
   (or set LAR_EXEC to the absolute path of lar-exec).

Docs: share/doc/lar/
EOF
tar -C "$DIST" -czf "$DIST/${STAGE_NAME}.tar.gz" "$STAGE_NAME"
rm -rf "$STAGE"
echo "    wrote $DIST/${STAGE_NAME}.tar.gz"

echo "==> Debian package (.deb)"
if ! cargo deb --help >/dev/null 2>&1; then
  echo "cargo-deb not found; install with: cargo install cargo-deb" >&2
  exit 1
fi
cargo deb -p lar --no-build -o "$DIST"
echo "    wrote deb under $DIST/"

echo "==> RPM package (.rpm)"
if ! cargo generate-rpm --help >/dev/null 2>&1; then
  echo "cargo-generate-rpm not found; install with: cargo install cargo-generate-rpm" >&2
  exit 1
fi
# cargo-generate-rpm expects to run from the package (or finds crate poorly from workspace root).
(
  cd "$ROOT/crates/lar"
  cargo generate-rpm --target-dir "$TARGET_DIR" -o "$DIST"
)
shopt -s nullglob
rpms=("$DIST"/*.rpm)
if ((${#rpms[@]} == 0)); then
  echo "no rpm produced in $DIST" >&2
  exit 1
fi
for rpm in "${rpms[@]}"; do
  echo "    wrote $rpm"
done

echo "==> Artifacts (${DEB_ARCH} / ${HOST_ARCH})"
ls -lh "$DIST"
