#!/bin/bash

# Load environment variables
if [ -f .env ]; then
    source .env
else
    source .env.default
    echo "Warning: Using default environment variables from .env.default"
fi

# Function to wait for PostgreSQL to be ready
wait_for_postgres() {
    echo "Waiting for PostgreSQL to be ready..."
    while ! pg_isready -h $POSTGRES_HOST -p $POSTGRES_PORT -U $POSTGRES_USER > /dev/null 2>&1; do
        sleep 1
    done
    echo "PostgreSQL is ready!"
}

# Stop and remove existing container if it exists
if docker ps -a | grep -q "advisor-db"; then
    echo "Stopping and removing existing advisor-db container..."
    docker stop advisor-db
    docker rm advisor-db
fi

# Start PostgreSQL container
echo "Starting PostgreSQL container..."
docker run --name advisor-db \
    -e POSTGRES_USER=$POSTGRES_USER \
    -e POSTGRES_PASSWORD=$POSTGRES_PASSWORD \
    -e POSTGRES_DB=$POSTGRES_DB \
    -p $POSTGRES_PORT:5432 \
    -d postgres:15

# Wait for PostgreSQL to be ready
sleep 5
wait_for_postgres

# Run migrations
echo "Running database migrations..."
cargo sqlx migrate run

echo "Database setup complete!"
echo "To reset the database, run: ./scripts/setup_db.sh"
