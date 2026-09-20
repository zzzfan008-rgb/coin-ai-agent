package config

import (
	"os"
	"testing"
)

func TestLoad(t *testing.T) {
	// Reset env vars after test
	orig := os.Getenv("PORT")
	defer os.Setenv("PORT", orig)

	os.Setenv("PORT", "9090")
	os.Setenv("JWT_SECRET", "test-secret-key-at-least-32-chars!!")
	os.Setenv("DATABASE_URL", "postgres://test:test@localhost:5432/testdb")
	os.Setenv("REDIS_URL", "redis://localhost:6380")
	os.Setenv("RUST_CORE_URL", "http://localhost:9999")

	cfg := Load()

	if cfg.Port != "9090" {
		t.Errorf("Port = %q, want %q", cfg.Port, "9090")
	}
	if cfg.JWTSecret != "test-secret-key-at-least-32-chars!!" {
		t.Errorf("JWTSecret = %q", cfg.JWTSecret)
	}
	if cfg.DatabaseURL != "postgres://test:test@localhost:5432/testdb" {
		t.Errorf("DatabaseURL = %q", cfg.DatabaseURL)
	}
	if cfg.RedisURL != "redis://localhost:6380" {
		t.Errorf("RedisURL = %q", cfg.RedisURL)
	}
	if cfg.RustCoreURL != "http://localhost:9999" {
		t.Errorf("RustCoreURL = %q", cfg.RustCoreURL)
	}
	if cfg.JWTExpHours != 72 {
		t.Errorf("JWTExpHours = %d, want 72", cfg.JWTExpHours)
	}
}

func TestLoad_FallbackDefaults(t *testing.T) {
	// Clear all env vars
	vars := []string{"PORT", "JWT_SECRET", "DATABASE_URL", "REDIS_URL", "RUST_CORE_URL", "JWT_EXP_HOURS"}
	orig := map[string]string{}
	for _, v := range vars {
		orig[v] = os.Getenv(v)
		os.Unsetenv(v)
	}
	defer func() {
		for k, v := range orig {
			os.Setenv(k, v)
		}
	}()

	cfg := Load()

	if cfg.Port != "8080" {
		t.Errorf("default Port = %q, want %q", cfg.Port, "8080")
	}
	if cfg.JWTSecret == "" {
		t.Error("default JWTSecret should not be empty")
	}
	if cfg.JWTExpHours != 72 {
		t.Errorf("default JWTExpHours = %d, want 72", cfg.JWTExpHours)
	}
}
