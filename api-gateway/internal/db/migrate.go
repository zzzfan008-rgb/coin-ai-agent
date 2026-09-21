package db

import (
	"context"
	"errors"
	"fmt"
	"log"
	"log/slog"
	"os"
	"path/filepath"

	"github.com/jmoiron/sqlx"
)

// ErrMigrationsNotFound 表示在所有候选位置都找不到迁移目录。
// 它属于配置/部署错误（区别于数据库暂不可用），调用方应显著告警。
var ErrMigrationsNotFound = errors.New("migrations directory not found in any candidate location")

// resolveMigrationsRoot 按优先级定位迁移目录，返回首个包含 .sql 文件的目录：
//  1. MIGRATIONS_DIR 环境变量
//  2. 可执行文件同目录 /migrations（容器布局 /app/server + /app/migrations）
//  3. 可执行文件上一级 /migrations（容器布局 /migrations + 二进制在子目录）
//  4. 工作目录 /migrations
//  5. 工作目录上一级 /migrations（从仓库子目录用 go run 启动的常见情形）
//
// 不再依赖单一的 cwd 相对路径，避免 cwd 不对时静默降级。
func ResolveMigrationsRoot() (string, error) {
	var candidates []string
	if env := os.Getenv("MIGRATIONS_DIR"); env != "" {
		candidates = append(candidates, env)
	}
	if exe, err := os.Executable(); err == nil {
		exeDir := filepath.Dir(exe)
		candidates = append(candidates,
			filepath.Join(exeDir, "migrations"),
			filepath.Join(exeDir, "..", "migrations"),
		)
	}
	if cwd, err := os.Getwd(); err == nil {
		candidates = append(candidates,
			filepath.Join(cwd, "migrations"),
			filepath.Join(cwd, "..", "migrations"),
		)
	}
	for _, c := range candidates {
		if hasSQLFiles(c) {
			return c, nil
		}
	}
	return "", fmt.Errorf("%w (tried %v)", ErrMigrationsNotFound, candidates)
}

func hasSQLFiles(dir string) bool {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return false
	}
	for _, e := range entries {
		if !e.IsDir() && filepath.Ext(e.Name()) == ".sql" {
			return true
		}
	}
	return false
}

// InitSchema 已退役为迁移引导：SQL migrations（migrations/ 按序号）是唯一
// 权威 schema 路径，本函数不再自建表结构，仅委托运行器执行全部迁移文件。
//
// 保留函数名以兼容历史调用方（cmd/migrate 旧入口等）。
func InitSchema(ctx context.Context, db *sqlx.DB, logger *slog.Logger) error {
	if logger == nil {
		logger = slog.Default()
	}
	root, err := ResolveMigrationsRoot()
	if err != nil {
		return err
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
		if errors.Is(err, ErrMigrationsNotFound) {
			// 部署/配置错误：继续启动只会让所有 DB 请求失败，用最高可见度告警。
			log.Printf("[ERROR] %v — set MIGRATIONS_DIR or run from a working directory that can locate migrations", err)
		} else {
			log.Printf("[WARN] migrate failed (will retry on request): %v", err)
		}
	}
}
