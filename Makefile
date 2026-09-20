.PHONY: help dev prod build test run lint fmt clean docker-up docker-down

# Variables
COMPOSE := docker compose
NODE := node
CARGO := cargo
GIT_REPO := $(shell git rev-parse --show-toplevel 2>/dev/null || echo ".")

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

# ── Docker ────────────────────────────────────────────────────────────────────

docker-up: ## Start all services (docker compose up -d)
	$(COMPOSE) up -d

docker-down: ## Stop all services
	$(COMPOSE) down

docker-logs: ## Tail logs from all services
	$(COMPOSE) logs -f

# ── Database ──────────────────────────────────────────────────────────────────

db-migrate: ## Run database migrations
	cd api-gateway && go run ./cmd/migrate/main.go

db-reset: ## Reset database (WARNING: destroys data)
	docker compose exec postgres psql -U fashion_ai -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"

db-shell: ## Open psql shell
	docker compose exec postgres psql -U fashion_ai -d fashion_ai

# ── Backend (Go) ──────────────────────────────────────────────────────────────

go-build: ## Build Go API gateway
	cd api-gateway && go build -o bin/server ./cmd/server

go-run: ## Run Go API gateway locally
	cd api-gateway && go run ./cmd/server/main.go

go-test: ## Run Go tests
	cd api-gateway && go test ./...

go-lint: ## Lint Go code
	cd api-gateway && golangci-lint run ./...

go-fmt: ## Format Go code
	cd api-gateway && go fmt ./...

# ── Core (Rust) ───────────────────────────────────────────────────────────────

core-build: ## Build Rust core service
	cd api-core && cargo build --release

core-run: ## Run Rust core service locally
	cd api-core && cargo run --release

core-test: ## Run Rust tests
	cd api-core && cargo test

core-clippy: ## Run clippy linter
	cd api-core && cargo clippy -- -D warnings

core-fmt: ## Format Rust code
	cd api-core && cargo fmt

# ── Frontend (React) ──────────────────────────────────────────────────────────

web-install: ## Install frontend dependencies
	cd web-client && npm install

web-dev: ## Run frontend dev server
	cd web-client && npm run dev

web-build: ## Build frontend for production
	cd web-client && npm run build

web-preview: ## Preview production build
	cd web-client && npm run preview

web-test: ## Run frontend tests
	cd web-client && npm test

web-lint: ## Lint frontend code
	cd web-client && npm run lint

web-fmt: ## Format frontend code
	cd web-client && npx prettier --write .

# ── Full Stack ────────────────────────────────────────────────────────────────

dev: docker-up ## Start Docker + run all services in dev mode
	@echo "Waiting for services..."
	@sleep 5
	@echo "Services ready. Go to http://localhost:3000"

prod: docker-up ## Start Docker for production
	cd api-gateway && go build -o bin/server ./cmd/server
	cd api-core && cargo build --release

build: go-build core-build web-build ## Build all modules
	@echo "All modules built successfully"

test: go-test core-test web-test ## Run all tests

run: ## Run all services locally (requires dependencies)
	@echo "Starting all services locally..."
	@make go-run &
	@make core-run &
	@make web-dev
	@wait

# ── Quality ───────────────────────────────────────────────────────────────────

lint: go-lint core-clippy web-lint ## Run all linters

fmt: go-fmt core-fmt web-fmt ## Format all code

# ── Clean ─────────────────────────────────────────────────────────────────────

clean: ## Remove build artifacts
	rm -rf api-gateway/bin
	rm -rf api-core/target
	rm -rf web-client/dist
	rm -rf web-client/node_modules/.vite

reset: docker-down clean ## Full reset (Docker down + clean artifacts)
