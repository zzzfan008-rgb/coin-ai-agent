package config

import (
	"fmt"
	"os"
	"strconv"
)

type Config struct {
	Port          string
	JWTKind       string
	JWTSecret     string
	JWTExpHours   int
	DatabaseURL   string
	RedisURL      string
	RustCoreURL   string
	MinimaxKey    string
	DeepSeekKey   string
	CookieSecure  bool // Secure attribute for httpOnly cookie (default false for HTTP dev)
	// T-022: SSE timeout config (seconds)
	SSEIdleTimeoutSecs  int
	SSEWriteTimeoutSecs int
}

func Load() *Config {
	expHours, _ := strconv.Atoi(env("JWT_EXP_HOURS", "72"))
	sseIdle, _ := strconv.Atoi(env("SSE_IDLE_TIMEOUT", "120"))
	sseWrite, _ := strconv.Atoi(env("SSE_WRITE_TIMEOUT", "300"))

	// Warn on insecure default — never silently start with the placeholder secret.
	if os.Getenv("JWT_SECRET") == "" {
		fmt.Fprintf(os.Stderr, "[FATAL] JWT_SECRET is not set; refusing to start with insecure default\n")
		os.Exit(1)
	}

	return &Config{
		Port:          env("PORT", "8080"),
		JWTKind:       env("JWT_KIND", "HS256"),
		JWTSecret:     os.Getenv("JWT_SECRET"),
		JWTExpHours:   expHours,
		DatabaseURL:   env("DATABASE_URL", "postgres://fashion_ai:***@localhost:5432/fashion_ai?sslmode=disable"),
		RedisURL:      env("REDIS_URL", "redis://localhost:6379"),
		RustCoreURL:   env("RUST_CORE_URL", "http://localhost:8081"),
		MinimaxKey:    env("MINIMAX_API_KEY", ""),
		DeepSeekKey:   env("DEEPSEEK_API_KEY", ""),
		CookieSecure:  env("COOKIE_SECURE", "false") == "true",
		// T-022: SSE timeouts (seconds); passed as env to core via upstream headers
		SSEIdleTimeoutSecs:  sseIdle,
		SSEWriteTimeoutSecs: sseWrite,
	}
}

func env(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}
