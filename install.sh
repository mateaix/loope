#!/usr/bin/env sh
# Build and install the `loope` CLI to your Cargo bin directory (~/.cargo/bin).
#
# Installs with the interactive TUI by default, so plain `loope` opens the
# full-screen prompt (like claude / codex). For the minimal std-only build,
# run: ./install.sh --no-tui
#
# Usage:
#   ./install.sh            # install with the interactive TUI
#   ./install.sh --no-tui   # minimal build, no TUI dependencies
#
# Requires Rust/Cargo (https://rustup.rs).
set -eu

features="--features tui"
if [ "${1:-}" = "--no-tui" ]; then
  features=""
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo (Rust) not found. Install Rust from https://rustup.rs" >&2
  exit 1
fi

# Run from the repository root (the directory containing this script).
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$script_dir"

echo "Building and installing loope (release)..."
# shellcheck disable=SC2086
cargo install --path . --force $features

if command -v loope >/dev/null 2>&1; then
  echo
  echo "Installed: $(command -v loope)"
  echo "Smoke test:"
  loope adapters
  echo
  if [ -n "$features" ]; then
    echo "Next: run \`loope\` to open the interactive prompt."
  else
    echo "Next: loope run --dry-run \"Add login\""
  fi
else
  echo
  echo "Installed to your Cargo bin. Add ~/.cargo/bin to PATH, e.g.:"
  echo "  export PATH=\"\$HOME/.cargo/bin:\$PATH\""
fi
