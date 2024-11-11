.PHONY: up down build shell test watch

up:
	docker-compose up -d

down:
	docker-compose down

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

clean:
	docker-compose down -v
