#!/bin/bash
set -e

# Create required data directories
mkdir -p data/edgar
mkdir -p data/transcripts
mkdir -p data/vectors

# Install additional cargo tools if needed
cargo install --list | grep -q "^cargo-watch" || cargo install cargo-watch
cargo install --list | grep -q "^cargo-edit" || cargo install cargo-edit
cargo install --list | grep -q "^cargo-audit" || cargo install cargo-audit

# Fix permissions for target directory
sudo chown -R vscode:vscode /workspace/target

echo "Development environment setup complete!"
