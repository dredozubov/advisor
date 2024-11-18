#!/bin/bash
set -e

# Create data directories if they don't exist
mkdir -p data

# Install additional cargo tools if needed
cargo install --list | grep -q "^cargo-watch" || cargo install cargo-watch
cargo install --list | grep -q "^cargo-edit" || cargo install cargo-edit
cargo install --list | grep -q "^cargo-audit" || cargo install cargo-audit

echo "Development environment setup complete!"
