#!/bin/bash
set -e

# Create data directories if they don't exist
mkdir -p data/edgar/filings data/edgar/parsed data/earnings/parsed

# Set up git config if not already set
if [ -z "$(git config --global user.email)" ]; then
    echo "Setting up git configuration..."
    read -p "Enter your git email: " git_email
    read -p "Enter your git name: " git_name
    git config --global user.email "$git_email"
    git config --global user.name "$git_name"
fi

# Install additional cargo tools if needed
cargo install --list | grep -q "^cargo-watch" || cargo install cargo-watch
cargo install --list | grep -q "^cargo-edit" || cargo install cargo-edit
cargo install --list | grep -q "^cargo-audit" || cargo install cargo-audit

echo "Development environment setup complete!"
