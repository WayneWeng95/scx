#!/bin/bash
# run.sh - Build and run scx_debugfifo
#
# Usage:
#   ./run.sh              # Build (debug) and run
#   ./run.sh build        # Build only (debug)
#   ./run.sh release      # Build (release) and run
#   ./run.sh run          # Run without rebuilding (uses last build)
#   ./run.sh log          # Build, run, and tee output to a timestamped log file
#   ./run.sh clean        # Clean build artifacts

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$SCRIPT_DIR"
CRATE_NAME="scx_debugfifo"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }

check_root() {
    if [ "$(id -u)" -ne 0 ]; then
        error "scx schedulers must run as root. Re-run with: sudo $0 $*"
    fi
}

do_build() {
    local profile="${1:-debug}"
    info "Building $CRATE_NAME (profile: $profile) ..."
    cd "$REPO_ROOT"
    if [ "$profile" = "debug" ]; then
        cargo build -p "$CRATE_NAME"
    else
        cargo build --profile="$profile" -p "$CRATE_NAME"
    fi
    info "Build complete."
}

get_binary() {
    local profile="${1:-debug}"
    local dir="$profile"
    [ "$dir" = "release-tiny" ] || [ "$dir" = "release-fast" ] && dir="$dir"
    echo "$REPO_ROOT/target/$dir/$CRATE_NAME"
}

do_run() {
    local profile="${1:-debug}"
    local binary
    binary="$(get_binary "$profile")"
    if [ ! -f "$binary" ]; then
        error "Binary not found at $binary. Run './run.sh build' first."
    fi
    check_root
    info "Running $binary ..."
    info "Press Ctrl+C to stop the scheduler."
    echo ""
    exec "$binary"
}

do_log() {
    local profile="${1:-debug}"
    local binary
    binary="$(get_binary "$profile")"
    if [ ! -f "$binary" ]; then
        error "Binary not found at $binary."
    fi
    check_root
    local logfile="$SCRIPT_DIR/debugfifo_$(date +%Y%m%d_%H%M%S).log"
    info "Running $binary, logging to $logfile ..."
    info "Press Ctrl+C to stop."
    echo ""
    "$binary" 2>&1 | tee "$logfile"
}

usage() {
    echo "Usage: $0 [command]"
    echo ""
    echo "Commands:"
    echo "  (none)    Build (debug) and run"
    echo "  build     Build only (debug profile)"
    echo "  release   Build (release) and run"
    echo "  run       Run last debug build without rebuilding"
    echo "  log       Build (debug), run, and save output to a log file"
    echo "  clean     cargo clean"
    echo "  help      Show this message"
}

case "${1:-}" in
    build)
        do_build debug
        ;;
    release)
        do_build release
        do_run release
        ;;
    run)
        do_run debug
        ;;
    log)
        do_build debug
        do_log debug
        ;;
    clean)
        cd "$REPO_ROOT"
        cargo clean -p "$CRATE_NAME"
        info "Cleaned."
        ;;
    help|-h|--help)
        usage
        ;;
    "")
        do_build debug
        do_run debug
        ;;
    *)
        error "Unknown command: $1. Use '$0 help' for usage."
        ;;
esac
