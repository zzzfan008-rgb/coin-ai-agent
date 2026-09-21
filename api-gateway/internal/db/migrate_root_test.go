package db

import (
	"os"
	"path/filepath"
	"testing"
)

// 构造生产典型布局并在结束后恢复 cwd：
//
//	root/migrations/*.sql
//	root/work/          ← 测试中切换到这里作为 cwd
func setupLayout(t *testing.T) (root, migrations string) {
	t.Helper()
	root = t.TempDir()
	migrations = filepath.Join(root, "migrations")
	if err := os.MkdirAll(migrations, 0o755); err != nil {
		t.Fatal(err)
	}
	work := filepath.Join(root, "work")
	if err := os.MkdirAll(work, 0o755); err != nil {
		t.Fatal(err)
	}
	orig, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	if err := os.Chdir(work); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chdir(orig) })
	return root, migrations
}

// MIGRATIONS_DIR 命中时优先采用。
func TestResolveMigrationsRoot_EnvOverride(t *testing.T) {
	_, migrations := setupLayout(t)
	if err := os.WriteFile(filepath.Join(migrations, "001_init.sql"), []byte("SELECT 1;"), 0o600); err != nil {
		t.Fatal(err)
	}
	custom := t.TempDir()
	if err := os.WriteFile(filepath.Join(custom, "009_x.sql"), []byte("SELECT 2;"), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("MIGRATIONS_DIR", custom)
	got, err := ResolveMigrationsRoot()
	if err != nil {
		t.Fatalf("expected env override, got: %v", err)
	}
	if got != custom {
		t.Errorf("resolved %q, want %q", got, custom)
	}
}

// MIGRATIONS_DIR 指向空目录时应被跳过，继续用 cwd/../migrations 定位，
// 复现“从仓库子目录启动也能找到迁移”的修复目标。
func TestResolveMigrationsRoot_CwdParentFallback(t *testing.T) {
	_, migrations := setupLayout(t)
	if err := os.WriteFile(filepath.Join(migrations, "001_init.sql"), []byte("SELECT 1;"), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("MIGRATIONS_DIR", t.TempDir()) // 空目录，必须被跳过
	got, err := ResolveMigrationsRoot()
	if err != nil {
		t.Fatalf("expected cwd/../migrations fallback, got: %v", err)
	}
	want, _ := filepath.EvalSymlinks(migrations)
	gotAbs, _ := filepath.EvalSymlinks(got)
	if gotAbs != want {
		t.Errorf("resolved %q, want %q", gotAbs, want)
	}
}

// hasSQLFiles 对不存在/无 sql 的目录返回 false。
func TestHasSQLFiles(t *testing.T) {
	if hasSQLFiles(filepath.Join(t.TempDir(), "missing")) {
		t.Error("missing dir should not be treated as having SQL files")
	}
	empty := t.TempDir()
	if hasSQLFiles(empty) {
		t.Error("empty dir should not be treated as having SQL files")
	}
}
