#!/bin/bash

# Ensure we have the default environment file
if [ ! -f .env ]; then
    echo "Creating .env from .env.default..."
    cp .env.default .env
fi

# Build and start services
echo "Building and starting services..."
make setup

echo "Setup complete! You can now:"
echo "1. Run 'make dev' to start the application with logs"
echo "2. Run 'make shell' to access the application shell"
echo "3. Run 'make test' to run tests"
echo "If you need to reset everything, run 'make reset'"
