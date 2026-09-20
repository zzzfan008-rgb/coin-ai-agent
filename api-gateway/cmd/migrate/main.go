package main

import (
	"context"
	"fmt"
	"log"

	"github.com/joho/godotenv"

	"fashionai/api-gateway/internal/config"
	"fashionai/api-gateway/internal/db"
)

func main() {
	_ = godotenv.Load()

	cfg := config.Load()

	ctx := context.Background()

	// Connect to database to run schema init
	pool, err := db.Open(db.Config{
		DatabaseURL:    cfg.DatabaseURL,
		MaxOpenConns:   1,
		MaxIdleConns:   1,
		ConnMaxLifetime: 0,
	})
	if err != nil {
		log.Fatalf("[FATAL] cannot connect to database: %v", err)
	}
	defer pool.Close()

	// Run InitSchema (Phase 1A schema without migration files)
	if err := db.InitSchema(ctx, pool, nil); err != nil {
		log.Fatalf("[FATAL] schema init failed: %v", err)
	}
	fmt.Println("[OK] schema initialized")
}
