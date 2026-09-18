.DEFAULT_GOAL := help
.PHONY: up down migrate test lint fmt verify-versions help

COMPOSE := docker compose -f infra/compose/docker-compose.yml -f infra/compose/docker-compose.local.yml

# Local/CI-only fixture credentials — see infra/migrations/0001_init.sql and
# infra/compose/docker-compose.yml for why these are not secrets.
MIGRATE_ENV := \
	SENTINEL_DB_HOST=127.0.0.1 \
	SENTINEL_DB_PORT=5432 \
	SENTINEL_DB_USER=sentinel_bootstrap \
	SENTINEL_DB_PASSWORD=sentinel_local_dev_only_bootstrap \
	SENTINEL_DB_NAME=sentinel

help:
	@echo "Sentinel — Phase 1 targets:"
	@echo "  make up              start Postgres, Surfpool, OTel Collector, Prometheus, Grafana, Redis"
	@echo "  make down            stop and remove the local stack"
	@echo "  make migrate         apply Postgres migrations (bounded retry; fails clearly if unreachable)"
	@echo "  make test            run the full offline Rust test suite"
	@echo "  make lint            cargo clippy with the workspace's deny-by-default lints"
	@echo "  make fmt             cargo fmt --check"
	@echo "  make verify-versions print every real, currently-installed toolchain version"

up:
	$(COMPOSE) up -d
	@echo "waiting for postgres and surfpool health checks..."
	@for i in $$(seq 1 60); do \
		pg_status=$$(docker inspect --format='{{.State.Health.Status}}' compose-postgres-1 2>/dev/null || echo missing); \
		sp_status=$$(docker inspect --format='{{.State.Health.Status}}' compose-surfpool-1 2>/dev/null || echo missing); \
		if [ "$$pg_status" = "healthy" ] && [ "$$sp_status" = "healthy" ]; then \
			echo "postgres: $$pg_status, surfpool: $$sp_status"; \
			exit 0; \
		fi; \
		sleep 2; \
	done; \
	echo "timed out waiting for postgres/surfpool to become healthy" >&2; \
	exit 1

down:
	$(COMPOSE) down -v

migrate:
	$(MIGRATE_ENV) cargo run --quiet -p sentinel-db --example migrate

test:
	cargo test --workspace
	cd ts && npm test

lint:
	cargo clippy --workspace --all-targets -- -D warnings
	bash scripts/ci-guards.sh
	cd ts && npx eslint .

fmt:
	cargo fmt --all -- --check
	cd ts && npx prettier --check .

verify-versions:
	@echo "=== docs/ecosystem-research.md §12 — mandatory Phase 1 re-verification ==="
	@echo "--- solana --version ---"; solana --version
	@echo "--- surfpool --version ---"; surfpool --version
	@echo "--- rustc / cargo ---"; rustc --version; cargo --version
	@echo "--- node / docker ---"; node --version; docker --version; docker compose version
	@echo "--- postgres (containerized; no local psql client required) ---"; \
		docker run --rm postgres:18-alpine postgres --version
	@echo "--- npm view @solana/kit version ---"; npm view @solana/kit version
	@echo "--- npm view @anchor-lang/core version ---"; npm view @anchor-lang/core version
	@echo "--- resolved Rust client crate versions (Cargo.lock; SR-5) ---"; \
		grep -A1 'name = "solana-rpc-client"' Cargo.lock | tail -1; \
		grep -A1 'name = "solana-pubsub-client"' Cargo.lock | tail -1; \
		grep -A1 'name = "solana-transaction-status"' Cargo.lock | tail -1
	@echo "--- local cluster getVersion (requires 'make up' first) ---"; \
		curl -s -X POST -H 'content-type: application/json' \
		  -d '{"jsonrpc":"2.0","id":1,"method":"getVersion"}' http://127.0.0.1:8899 || true
