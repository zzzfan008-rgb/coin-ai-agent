package main

import (
	"context"
	"fmt"
	"log"
	"os"

	"github.com/joho/godotenv"

	"fashionai/api-gateway/internal/config"
	"fashionai/api-gateway/internal/db"
)

func main() {
	_ = godotenv.Load()

	cfg := config.Load()

	ctx := context.Background()

	// Run migrations
	if err := db.RunMigrations(ctx, cfg.DatabaseURL); err != nil {
		log.Fatalf("[FATAL] migrations failed: %v", err)
	}

	v, err := db.CurrentVersion(cfg.DatabaseURL)
	if err != nil {
		log.Printf("[WARN] could not read migration version: %v", err)
	} else {
		fmt.Printf("[OK] current migration version: %d\n", v)
	}
}
