package main

import (
	"context"
	"log"
	"log/slog"
	"os"

	"github.com/joho/godotenv"

	"fashionai/api-gateway/internal/config"
	"fashionai/api-gateway/internal/db"
)

// 一键迁移入口：顺序执行 migrations/ 目录全部 .sql（每个文件一个事务，
// 失败即停并回滚，无 dirty=t 卡死）。干净库可直接建出完整 schema。
//
//	go run ./cmd/migrate
//	MIGRATIONS_DIR=../migrations go run ./cmd/migrate
func main() {
	_ = godotenv.Load()

	cfg := config.Load()

	ctx := context.Background()

	pool, err := db.Open(db.Config{
		DatabaseURL:  cfg.DatabaseURL,
		MaxOpenConns: 2,
		MaxIdleConns: 1,
	})
	if err != nil {
		log.Fatalf("[FATAL] cannot connect to database: %v", err)
	}
	defer pool.Close()

	root := os.Getenv("MIGRATIONS_DIR")
	if root == "" {
		root = "migrations"
	}

	if err := db.Migrate(ctx, pool, db.Config{MigrationsRoot: root}, slog.Default()); err != nil {
		log.Fatalf("[FATAL] migrate failed: %v", err)
	}
	log.Println("[OK] migrations applied successfully")
}
