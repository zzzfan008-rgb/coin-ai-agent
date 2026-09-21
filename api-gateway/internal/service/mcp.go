package service

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"

	"github.com/google/uuid"
	"github.com/jmoiron/sqlx"

	"fashionai/api-gateway/internal/model"
)

// McpService manages MCP server configuration stored in PostgreSQL
// (mcp_servers) and per-user enablement (users.mcp_server_ids).
type McpService struct {
	db *sqlx.DB
}

func NewMcpService(db *sqlx.DB) *McpService {
	return &McpService{db: db}
}

// McpServerRow mirrors the mcp_servers table for scans.
type McpServerRow struct {
	ID         uuid.UUID      `db:"id" json:"id"`
	OrgID      uuid.UUID      `db:"org_id" json:"org_id"`
	Name       string         `db:"name" json:"name"`
	ServerType string         `db:"server_type" json:"server_type"`
	Endpoint   sql.NullString `db:"endpoint" json:"endpoint"`
	EnvVars    []byte         `db:"env_vars" json:"-"`
	IsActive   bool           `db:"is_active" json:"is_active"`
	CreatedAt  string         `db:"created_at" json:"created_at"`
}

// Register inserts a new MCP server configuration.
func (s *McpService) Register(ctx context.Context, orgID, createdBy uuid.UUID, req model.McpServerRequest) (*model.McpServerDTO, error) {
	if req.Name == "" {
		return nil, fmt.Errorf("name is required")
	}
	if req.ServerType == "" {
		req.ServerType = "http"
	}
	envRaw, err := json.Marshal(req.EnvVars)
	if err != nil {
		return nil, fmt.Errorf("marshal env_vars: %w", err)
	}

	var row model.McpServerDTO
	var endpoint sql.NullString
	if req.Endpoint != "" {
		endpoint = sql.NullString{String: req.Endpoint, Valid: true}
	}
	err = s.db.GetContext(ctx, &row,
		`INSERT INTO mcp_servers (org_id, name, server_type, endpoint, auth_token, env_vars, is_active, created_by)
		 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
		 RETURNING id, org_id, name, server_type, endpoint, is_active, created_at`,
		orgID, req.Name, req.ServerType, endpoint, nullableToken(req.AuthToken), envRaw, true, createdBy)
	if err != nil {
		return nil, fmt.Errorf("insert mcp server: %w", err)
	}
	return &row, nil
}

func nullableToken(t string) any {
	if t == "" {
		return nil
	}
	return t
}

// List returns all MCP servers configured in the org.
func (s *McpService) List(ctx context.Context, orgID uuid.UUID) ([]model.McpServerDTO, error) {
	var rows []model.McpServerDTO
	err := s.db.SelectContext(ctx, &rows,
		`SELECT id, org_id, name, server_type, endpoint, is_active, created_at
		 FROM mcp_servers WHERE org_id = $1 ORDER BY created_at DESC`, orgID)
	if err != nil {
		return nil, fmt.Errorf("list mcp servers: %w", err)
	}
	return rows, nil
}

// Delete removes an MCP server (org scoped).
func (s *McpService) Delete(ctx context.Context, orgID, id uuid.UUID) error {
	res, err := s.db.ExecContext(ctx,
		"DELETE FROM mcp_servers WHERE id = $1 AND org_id = $2", id, orgID)
	if err != nil {
		return fmt.Errorf("delete mcp server: %w", err)
	}
	n, _ := res.RowsAffected()
	if n == 0 {
		return ErrProjectNotFound // reuse generic not-found; message clarifies in handler
	}
	return nil
}

// SetUserEnabled enables/disables a server for a specific user by
// mutating users.mcp_server_ids (JSON array of server UUID strings).
func (s *McpService) SetUserEnabled(ctx context.Context, orgID, userID, serverID uuid.UUID, enabled bool) ([]string, error) {
	// Verify the server belongs to the caller's org.
	var exists bool
	if err := s.db.GetContext(ctx, &exists,
		"SELECT EXISTS(SELECT 1 FROM mcp_servers WHERE id=$1 AND org_id=$2)",
		serverID, orgID); err != nil {
		return nil, fmt.Errorf("check mcp server: %w", err)
	}
	if !exists {
		return nil, fmt.Errorf("mcp server not found")
	}

	var rawIDs []byte
	if err := s.db.GetContext(ctx, &rawIDs,
		`SELECT mcp_server_ids FROM users WHERE id = $1 FOR UPDATE`, userID); err != nil {
		return nil, fmt.Errorf("load user mcp ids: %w", err)
	}
	var ids []string
	if len(rawIDs) > 0 {
		_ = json.Unmarshal(rawIDs, &ids)
	}

	ids = toggleString(ids, serverID.String(), enabled)

	newRaw, _ := json.Marshal(ids)
	if _, err := s.db.ExecContext(ctx,
		"UPDATE users SET mcp_server_ids = $1 WHERE id = $2", newRaw, userID); err != nil {
		return nil, fmt.Errorf("update user mcp ids: %w", err)
	}
	return ids, nil
}

func toggleString(items []string, val string, present bool) []string {
	idx := -1
	for i, v := range items {
		if v == val {
			idx = i
			break
		}
	}
	if present && idx == -1 {
		items = append(items, val)
	}
	if !present && idx != -1 {
		items = append(items[:idx], items[idx+1:]...)
	}
	return items
}
