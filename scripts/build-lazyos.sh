#!/bin/bash
# Build the session framework: the `lazyos` multi-call binary.
#
# The Rust workspace in rust/ is the source; this script only builds it the way
# it ships (release profile) and stages the binary where install-lazyos.sh
# expects it:
#
#   $LAZYOS_STAGE/lazyos   /usr/bin/lazyos
#
# There is no tarball and no pin: the framework is this repository's own code,
# which is also why the binary reports the committed VERSION (compiled in by
# rust/crates/lazyos-cli/src/version.rs) instead of a cargo version.
#
# Usage: build-lazyos.sh
set -euo pipefail
. "$(dirname "$0")/lib.sh"

command -v cargo >/dev/null 2>&1 || die "cargo is not installed (run scripts/00-deps.sh)"

log "Building the session framework (lazyos)"
cargo build --release --locked --manifest-path "$REPO_DIR/rust/Cargo.toml"

BIN="$REPO_DIR/rust/target/release/lazyos"
[ -x "$BIN" ] || die "cargo finished but $BIN is missing"
NOTIFY_BIN="$REPO_DIR/rust/target/release/lazyos-notify"
[ -x "$NOTIFY_BIN" ] || die "cargo finished but $NOTIFY_BIN is missing"
# A binary that cannot report its version is not the one the image should
# carry: the check is one process and it catches a broken include_str!.
VERSION="$(cat "$REPO_DIR/VERSION")"
REPORTED="$("$BIN" --version)"
[ "$REPORTED" = "$VERSION" ] ||
	die "lazyos reports '$REPORTED' but VERSION is '$VERSION'"

mkdir -p "$LAZYOS_STAGE"
install -m 0755 "$BIN" "$LAZYOS_STAGE/lazyos"
install -m 0755 "$NOTIFY_BIN" "$LAZYOS_STAGE/lazyos-notify"
log "lazyos $REPORTED -> $LAZYOS_STAGE/lazyos"
log "lazyos-notify -> $LAZYOS_STAGE/lazyos-notify"
