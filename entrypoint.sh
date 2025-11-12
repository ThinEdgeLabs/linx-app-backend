#!/bin/bash

set -e

# Start the application with cargo-watch
# echo "Starting application with cargo-watch..."
# exec cargo watch -x 'run --release'

# Extract database URL from config.toml
if [ -f /app/config.toml ]; then
  DB_URL=$(grep -oP 'database_url\s*=\s*"\K[^"]+' /app/config.toml)
  if [ -n "$DB_URL" ]; then
    echo "Found database URL in config.toml: $DB_URL"
    export DATABASE_URL="$DB_URL"
    echo "DATABASE_URL set from config.toml"
  fi
fi