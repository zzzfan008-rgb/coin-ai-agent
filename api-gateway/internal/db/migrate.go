package db

import (
	"context"
	"fmt"
	"log"
	"log/slog"
	"os"

	"github.com/jmoiron/sqlx"
)

// InitSchema applies the Phase 1A schema directly via the connection.
// Used when migration files are not yet available (development).
func InitSchema(ctx context.Context, db *sqlx.DB, logger *slog.Logger) error {
	schema := `
	CREATE TABLE IF NOT EXISTS orgs (
	  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
	  name VARCHAR(255) NOT NULL,
	  created_at TIMESTAMPTZ DEFAULT NOW()
	);

	CREATE TABLE IF NOT EXISTS depts (
	  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
	  org_id UUID NOT NULL REFERENCES orgs(id),
	  parent_id UUID REFERENCES depts(id),
	  name VARCHAR(255) NOT NULL,
	  path VARCHAR(1000) NOT NULL,
	  created_at TIMESTAMPTZ DEFAULT NOW()
	);

	CREATE TABLE IF NOT EXISTS users (
	  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
	  org_id UUID NOT NULL REFERENCES orgs(id),
	  dept_id UUID NOT NULL REFERENCES depts(id),
	  username VARCHAR(100) NOT NULL UNIQUE,
	  password_hash VARCHAR(255) NOT NULL,
	  display_name VARCHAR(255),
	  email VARCHAR(255),
	  role VARCHAR(50) NOT NULL DEFAULT 'designer',
	  is_active BOOLEAN DEFAULT true,
	  created_at TIMESTAMPTZ DEFAULT NOW()
	);

	CREATE TABLE IF NOT EXISTS sessions (
	  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
	  org_id UUID NOT NULL REFERENCES orgs(id),
	  user_id UUID NOT NULL REFERENCES users(id),
	  title VARCHAR(500),
	  is_archived BOOLEAN DEFAULT false,
	  created_at TIMESTAMPTZ DEFAULT NOW(),
	  updated_at TIMESTAMPTZ DEFAULT NOW()
	);

	CREATE TABLE IF NOT EXISTS messages (
	  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
	  session_id UUID NOT NULL REFERENCES sessions(id),
	  role VARCHAR(50) NOT NULL,
	  content TEXT NOT NULL,
	  model VARCHAR(100),
	  token_count INT,
	  metadata JSONB,
	  created_at TIMESTAMPTZ DEFAULT NOW()
	);

	CREATE TABLE IF NOT EXISTS roles (
	  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
	  org_id UUID NOT NULL REFERENCES orgs(id),
	  name VARCHAR(100) NOT NULL,
	  permissions JSONB NOT NULL DEFAULT '[]',
	  created_at TIMESTAMPTZ DEFAULT NOW()
	);

	CREATE TABLE IF NOT EXISTS audit_logs (
	  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
	  org_id UUID NOT NULL REFERENCES orgs(id),
	  user_id UUID REFERENCES users(id),
	  action VARCHAR(100) NOT NULL,
	  resource_type VARCHAR(100),
	  resource_id UUID,
	  details JSONB,
	  ip_address INET,
	  created_at TIMESTAMPTZ DEFAULT NOW()
	);

	CREATE INDEX IF NOT EXISTS idx_sessions_org_user ON sessions(org_id, user_id);
	CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id, created_at);
	CREATE INDEX IF NOT EXISTS idx_audit_org_user ON audit_logs(org_id, user_id, created_at);
	CREATE INDEX IF NOT EXISTS idx_depts_path ON depts(org_id, path);
	CREATE INDEX IF NOT EXISTS idx_users_org ON users(org_id);
	`

	if _, err := db.ExecContext(ctx, schema); err != nil {
		return fmt.Errorf("init schema: %w", err)
	}
	if logger != nil {
		logger.Info("schema initialized")
	}
	return nil
}

// EnsureDB attempts to initialize the schema; logs but does not fail on error.
// This lets the service start in degraded mode when DB is not ready.
func EnsureDB(ctx context.Context, db *sqlx.DB, logger *slog.Logger) {
	if os.Getenv("SKIP_DB_INIT") == "1" {
		return
	}
	if err := InitSchema(ctx, db, logger); err != nil {
		log.Printf("[WARN] schema init failed (will retry on request): %v", err)
	}
}
