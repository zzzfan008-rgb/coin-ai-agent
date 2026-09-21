package db

import (
	"context"
	"log/slog"
	"os"
	"testing"

	"github.com/jmoiron/sqlx"
)

func TestGateFreshMigrate(t *testing.T) {
	url := os.Getenv("GATE_DB_URL")
	if url == "" {
		t.Skip("GATE_DB_URL not set")
	}
	db, err := sqlx.Connect("postgres", url)
	if err != nil {
		t.Fatalf("connect: %v", err)
	}
	defer db.Close()
	logger := slog.Default()
	err = Migrate(context.Background(), db, Config{MigrationsRoot: "../../../migrations"}, logger)
	t.Logf("Migrate result: %v", err)
	if err != nil {
		t.Fatalf("Migrate failed: %v", err)
	}
}

func TestGateInitSchema(t *testing.T) {
	url := os.Getenv("GATE_INIT_URL")
	if url == "" {
		t.Skip("GATE_INIT_URL not set")
	}
	db, err := sqlx.Connect("postgres", url)
	if err != nil {
		t.Fatalf("connect: %v", err)
	}
	defer db.Close()
	if err := InitSchema(context.Background(), db, slog.Default()); err != nil {
		t.Fatalf("InitSchema failed: %v", err)
	}
}
