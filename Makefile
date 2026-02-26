.PHONY: pull deploy start stop restart cli db clean-images help

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

# Pull images from DigitalOcean registry
pull:
	@echo "Pulling Docker images (version: $(VERSION))..."
	VERSION=$(VERSION) docker compose -f docker-compose.prod.yml pull indexer api cli
	@echo "Pull completed (version: $(VERSION))."

# Full deployment: stop, pull new images, clean old images, and start
deploy: stop pull clean-images start
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
	@echo "  make pull          - Pull images from registry (version: $(VERSION))"
	@echo "  make deploy        - Full deployment: stop, pull, clean, start (version: $(VERSION))"
	@echo "  make start         - Start all services (db, indexer, api, cli)"
	@echo "  make stop          - Stop all services"
	@echo "  make restart       - Restart all services (stop + start)"
	@echo "  make clean-images  - Remove old image versions, keep $(VERSION)"
	@echo "  make cli           - Access CLI container interactively"
	@echo "  make db            - Connect to PostgreSQL database"
