package db

import (
	"context"
	"log"
	"log/slog"
	"os"

	"github.com/jmoiron/sqlx"
)

// InitSchema 已退役为迁移引导：SQL migrations（migrations/ 按序号）是唯一
// 权威 schema 路径，本函数不再自建表结构，仅委托运行器执行全部迁移文件。
//
// 保留函数名以兼容历史调用方（cmd/migrate 旧入口等）。
func InitSchema(ctx context.Context, db *sqlx.DB, logger *slog.Logger) error {
	if logger == nil {
		logger = slog.Default()
	}
	root := os.Getenv("MIGRATIONS_DIR")
	if root == "" {
		root = "migrations"
	}
	return Migrate(ctx, db, Config{MigrationsRoot: root}, logger)
}

// EnsureDB runs the migration runner; logs but does not fail on error.
// This lets the service start in degraded mode when DB is not ready.
func EnsureDB(ctx context.Context, db *sqlx.DB, logger *slog.Logger) {
	if os.Getenv("SKIP_DB_INIT") == "1" {
		return
	}
	if err := InitSchema(ctx, db, logger); err != nil {
		log.Printf("[WARN] migrate failed (will retry on request): %v", err)
	}
}
