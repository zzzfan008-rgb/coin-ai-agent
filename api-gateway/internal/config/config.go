package config

import (
	"os"
	"strconv"
)

type Config struct {
	Port         string
	JWTKind      string
	JWTSecret    string
	JWTExpHours  int
	DatabaseURL  string
	RedisURL     string
	RustCoreURL  string
	MinimaxKey   string
	DeepSeekKey  string
}

func Load() *Config {
	expHours, _ := strconv.Atoi(env("JWT_EXP_HOURS", "72"))
	return &Config{
		Port:        env("PORT", "8080"),
		JWTKind:     env("JWT_KIND", "HS256"),
		JWTSecret:   env("JWT_SECRET", "change-me-in-production-32chars!!"),
		JWTExpHours: expHours,
		DatabaseURL: env("DATABASE_URL", "postgres://fashion_ai:***@localhost:5432/fashion_ai?sslmode=disable"),
		RedisURL:    env("REDIS_URL", "redis://localhost:6379"),
		RustCoreURL: env("RUST_CORE_URL", "http://localhost:8081"),
		MinimaxKey:  env("MINIMAX_API_KEY", ""),
		DeepSeekKey: env("DEEPSEEK_API_KEY", ""),
	}
}

func env(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}
