.PHONY: build start stop cli db help

# Default target
.DEFAULT_GOAL := help

# Load environment variables from .env file
ifneq (,$(wildcard ./.env))
    include .env
    export
endif

# Build all Docker images
build:
	@echo "Building Docker images..."
	docker compose -f docker-compose.prod.yml build

# Start all services
start:
	@echo "Starting services..."
	docker compose -f docker-compose.prod.yml up -d
	@echo "Services started."

# Stop all services
stop:
	@echo "Stopping services..."
	docker compose -f docker-compose.prod.yml down

# Access CLI container
cli:
	@echo "Connecting to CLI container..."
	docker compose -f docker-compose.prod.yml exec cli bash

# Connect to PostgreSQL database
sql-cli:
	@echo "Connecting to database..."
	@docker compose -f docker-compose.prod.yml exec db psql -U $${POSTGRES_USER} -d $${POSTGRES_DB}

# Show help
help:
	@echo "Available commands:"
	@echo "  make build  - Build all Docker images"
	@echo "  make start  - Start all services (db, indexer, api, cli)"
	@echo "  make stop   - Stop all services"
	@echo "  make cli    - Access CLI container interactively"
	@echo "  make db     - Connect to PostgreSQL database"
