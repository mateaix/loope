#!/usr/bin/env sh
# Build and install the `loope` CLI to your Cargo bin directory (~/.cargo/bin).
#
# Usage:
#   ./install.sh
#
# Requires Rust/Cargo (https://rustup.rs). No other dependencies.
set -eu

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo (Rust) not found. Install Rust from https://rustup.rs" >&2
  exit 1
fi

# Run from the repository root (the directory containing this script).
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$script_dir"

echo "Building and installing loope (release)..."
cargo install --path . --force

if command -v loope >/dev/null 2>&1; then
  echo
  echo "Installed: $(command -v loope)"
  echo "Smoke test:"
  loope adapters
  echo
  echo "Next: loope run --dry-run \"Add login\""
else
  echo
  echo "Installed to your Cargo bin. Add ~/.cargo/bin to PATH, e.g.:"
  echo "  export PATH=\"\$HOME/.cargo/bin:\$PATH\""
fi
