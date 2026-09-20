//go:build ignore

// This file exists so sqlx can be run via go generate.
// Run: cd api-gateway && go generate ./...
//
// This is a placeholder; real codegen requires the sqlx CLI:
//
//   go install github.com/jmoiron/sqlx/cmd/sqlx@latest
//   cd internal/db/queries && sqlx generate
//
// The .sql files above contain sqlx named-query annotations (-- name: ...).
// After running sqlx generate, corresponding .sql.go files will be created
// with compile-time type-safe query functions.

package main

func main() {}
