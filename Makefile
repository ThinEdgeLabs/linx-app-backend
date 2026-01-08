.PHONY: build rebuild deploy start stop restart cli db clean-images help

# Default target
.DEFAULT_GOAL := help

# Extract version from Cargo.toml
VERSION := $(shell grep '^version = ' examples/linx-indexer/Cargo.toml | head -n1 | cut -d'"' -f2)
export VERSION

# Load environment variables from .env file
ifneq (,$(wildcard ./.env))
    include .env
    export
endif

# Build all Docker images
build:
	@echo "Building Docker images (version: $(VERSION))..."
	VERSION=$(VERSION) docker compose -f docker-compose.prod.yml build cli
	VERSION=$(VERSION) docker compose -f docker-compose.prod.yml build indexer api
	@echo "Build completed (version: $(VERSION))."

# Rebuild all Docker images without cache
rebuild:
	@echo "Rebuilding Docker images without cache (version: $(VERSION))..."
	VERSION=$(VERSION) docker compose -f docker-compose.prod.yml build --no-cache cli
	VERSION=$(VERSION) docker compose -f docker-compose.prod.yml build --no-cache indexer api
	@echo "Rebuild completed (version: $(VERSION))."

# Full deployment: stop, rebuild, clean old images, and start
deploy: stop rebuild clean-images start
	@echo "Deployment completed (version: $(VERSION))."

# Start all services
start:
	@echo "Starting services (version: $(VERSION))..."
	VERSION=$(VERSION) docker compose -f docker-compose.prod.yml up -d
	@echo "Services started."

# Stop all services
stop:
	@echo "Stopping services..."
	VERSION=$(VERSION) docker compose -f docker-compose.prod.yml down

# Restart all services
restart: stop start

# Clean up old Docker images for this project
clean-images:
	@echo "Removing old linx-app-backend images..."
	@docker images --format "{{.Repository}}:{{.Tag}}" | grep "linx-app-backend" | grep -v "$(VERSION)" | xargs -r docker rmi || true
	@echo "Cleanup completed. Current version $(VERSION) images retained."

# Access CLI container
cli:
	@echo "Connecting to CLI container..."
	VERSION=$(VERSION) docker compose -f docker-compose.prod.yml exec cli bash

# Connect to PostgreSQL database
sql-cli:
	@echo "Connecting to database..."
	@docker compose -f docker-compose.prod.yml exec db psql -U $${POSTGRES_USER} -d $${POSTGRES_DB}

# Show help
help:
	@echo "Available commands:"
	@echo "  make build         - Build all Docker images (version: $(VERSION))"
	@echo "  make rebuild       - Force rebuild without cache (version: $(VERSION))"
	@echo "  make deploy        - Full deployment: stop, rebuild, clean, start (version: $(VERSION))"
	@echo "  make start         - Start all services (db, indexer, api, cli)"
	@echo "  make stop          - Stop all services"
	@echo "  make restart       - Restart all services (stop + start)"
	@echo "  make clean-images  - Remove old image versions, keep $(VERSION)"
	@echo "  make cli           - Access CLI container interactively"
	@echo "  make db            - Connect to PostgreSQL database"
