.PHONY: build start stop cli help

# Default target
.DEFAULT_GOAL := help

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

# Show help
help:
	@echo "Available commands:"
	@echo "  make build  - Build all Docker images"
	@echo "  make start  - Start all services (db, indexer, api, cli)"
	@echo "  make stop   - Stop all services"
	@echo "  make cli    - Access CLI container interactively"
