// Package db provides database connection, migrations, and compiled SQLx queries.
package db

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/jmoiron/sqlx"
	_ "github.com/lib/pq"
)

// Config holds database connection settings.
type Config struct {
	DatabaseURL    string
	MaxOpenConns   int
	MaxIdleConns   int
	ConnMaxLifetime time.Duration
	// MigrationsFS is an optional embedded filesystem containing SQL migration files.
	MigrationsFS *struct{} // placeholder; migrations are run via Migrate()
	MigrationsRoot string
	SkipMigrations bool
}

// Open creates a *sqlx.DB with the standard PostgreSQL driver.
func Open(cfg Config) (*sqlx.DB, error) {
	db, err := sqlx.Connect("postgres", cfg.DatabaseURL)
	if err != nil {
		return nil, fmt.Errorf("db open: %w", err)
	}
	db.SetMaxOpenConns(cfg.MaxOpenConns)
	db.SetMaxIdleConns(cfg.MaxIdleConns)
	db.SetConnMaxLifetime(cfg.ConnMaxLifetime)
	return db, nil
}

// Migrate runs all pending SQL migrations from the embedded FS.
func Migrate(ctx context.Context, db *sqlx.DB, cfg Config, logger *slog.Logger) error {
	// Ensure schema_migrations table exists.
	_, err := db.ExecContext(ctx, `
		CREATE TABLE IF NOT EXISTS schema_migrations (
			version     BIGINT PRIMARY KEY,
			dirty       BOOLEAN NOT NULL DEFAULT false,
			applied_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
			rolled_back_at TIMESTAMPTZ
		)`)
	if err != nil {
		return fmt.Errorf("create schema_migrations table: %w", err)
	}

	// Read applied versions.
	rows, err := db.QueryxContext(ctx, `SELECT version FROM schema_migrations ORDER BY version`)
	if err != nil {
		return fmt.Errorf("read applied migrations: %w", err)
	}
	applied := make(map[int64]bool)
	for rows.Next() {
		var v int64
		if err := rows.Scan(&v); err != nil {
			rows.Close()
			return fmt.Errorf("scan version: %w", err)
		}
		applied[v] = true
	}
	rows.Close()

	// Discover migration files from disk (project/migrations/ directory).
	// Caller passes SKIP_MIGRATIONS=1 env to disable.
	migrationsRoot := cfg.MigrationsRoot
	if migrationsRoot == "" {
		migrationsRoot = "migrations"
	}
	entries, err := os.ReadDir(migrationsRoot)
	if err != nil {
		return fmt.Errorf("read migrations dir %q: %w", migrationsRoot, err)
	}

	var files []string
	for _, e := range entries {
		if !e.IsDir() && strings.HasSuffix(e.Name(), ".sql") {
			files = append(files, e.Name())
		}
	}
	sort.Strings(files)

	migrated := 0
	for _, fname := range files {
		// Parse version from filename: 001_init_schema.sql -> 1
		version, _, _ := strings.Cut(fname, "_")
		ver, err := strconv.ParseInt(strings.TrimPrefix(version, "0"), 10, 64)
		if err != nil {
			logger.Warn("skipping migration with unparseable version", "file", fname)
			continue
		}

		if applied[ver] {
			logger.Debug("migration already applied, skipping", "version", ver, "file", fname)
			continue
		}

		// Read SQL content from disk
		sqlBytes, err := os.ReadFile(filepath.Join(migrationsRoot, fname))
		if err != nil {
			return fmt.Errorf("read migration %s: %w", fname, err)
		}

		// Mark dirty before executing
		_, err = db.ExecContext(ctx,
			`INSERT INTO schema_migrations (version, dirty, applied_at) VALUES ($1, true, NOW())
			 ON CONFLICT (version) DO UPDATE SET dirty = true`,
			ver)
		if err != nil {
			return fmt.Errorf("mark dirty: %w", err)
		}

		// Execute migration in a transaction
		tx, err := db.BeginTxx(ctx, nil)
		if err != nil {
			return fmt.Errorf("begin tx: %w", err)
		}
		_, err = tx.ExecContext(ctx, string(sqlBytes))
		if err != nil {
			tx.Rollback()
			// Clear dirty flag but keep the row so it shows as failed
			db.ExecContext(ctx, `UPDATE schema_migrations SET dirty = true WHERE version = $1`, ver)
			return fmt.Errorf("execute migration %d (%s): %w", ver, fname, err)
		}
		_, err = tx.ExecContext(ctx,
			`UPDATE schema_migrations SET dirty = false, applied_at = NOW() WHERE version = $1`, ver)
		if err != nil {
			tx.Rollback()
			return fmt.Errorf("clear dirty: %w", err)
		}
		if err = tx.Commit(); err != nil {
			return fmt.Errorf("commit: %w", err)
		}

		logger.Info("applied migration", "version", ver, "file", fname)
		migrated++
	}

	if migrated == 0 {
		logger.Info("database already up to date")
	} else {
		logger.Info("migrations complete", "applied", migrated)
	}
	return nil
}

// MustMigrate runs Migrate and panics on error. Use in main.go init.
func MustMigrate(ctx context.Context, db *sqlx.DB, cfg Config, logger *slog.Logger) {
	if err := Migrate(ctx, db, cfg, logger); err != nil {
		panic("migrate: " + err.Error())
	}
}
