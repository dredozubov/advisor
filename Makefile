.PHONY: up down build shell test watch logs clean dev reset setup init-db migrate

# Development commands
up:
	docker-compose up -d

down:
	docker-compose down -v

build:
	docker-compose build

shell:
	docker-compose exec app /bin/bash

test:
	docker-compose exec app cargo test

watch:
	docker-compose exec app cargo watch -x run

logs:
	docker-compose logs -f

dev:
	docker-compose up

# Database commands
init-db:
	docker-compose exec app cargo sqlx database create
	docker-compose exec app cargo sqlx migrate run

migrate:
	docker-compose exec app cargo sqlx migrate run

# Reset everything and start fresh
reset: clean setup

# Complete setup
setup: down
	docker-compose up -d db
	@echo "Waiting for database to be ready..."
	@sleep 5
	docker-compose up -d app
	@echo "Running database migrations..."
	@make init-db
	@echo "Setup complete!"

# Clean everything
clean:
	docker-compose down -v
