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
	ID         uuid.UUID      `db:"id"`
	OrgID      uuid.UUID      `db:"org_id"`
	Name       string         `db:"name"`
	ServerType string         `db:"server_type"`
	Endpoint   sql.NullString `db:"endpoint"`
	EnvVars    []byte         `db:"env_vars"`
	IsActive   bool           `db:"is_active"`
	CreatedAt  string         `db:"created_at"`
}

// Register inserts a new MCP server configuration.
// `id` is a client-supplied logical label (e.g. "fabric-erp"); the DB primary
// key is a generated UUID. `name` falls back to `id` when empty.
func (s *McpService) Register(ctx context.Context, orgID, createdBy uuid.UUID, req model.McpServerRequest) (*model.McpServerDTO, error) {
	if req.ID == "" && req.Name == "" {
		return nil, fmt.Errorf("id or name is required")
	}
	name := req.Name
	if name == "" {
		name = req.ID
	}

	var endpoint sql.NullString
	if req.EndpointURL != "" {
		endpoint = sql.NullString{String: req.EndpointURL, Valid: true}
	}

	var row McpServerRow
	err := s.db.GetContext(ctx, &row,
		`INSERT INTO mcp_servers (org_id, name, server_type, endpoint, env_vars, is_active, created_by)
		 VALUES ($1, $2, 'http', $3, '{}', $4, $5)
		 RETURNING id, org_id, name, server_type, endpoint, env_vars, is_active, created_at`,
		orgID, name, endpoint, true, createdBy)
	if err != nil {
		return nil, fmt.Errorf("insert mcp server: %w", err)
	}
	dto := toDTO(row, false)
	return &dto, nil
}

// List returns all MCP servers in the org, with per-user enabled state.
func (s *McpService) List(ctx context.Context, orgID, userID uuid.UUID) ([]model.McpServerDTO, error) {
	var rows []McpServerRow
	err := s.db.SelectContext(ctx, &rows,
		`SELECT id, org_id, name, server_type, endpoint, env_vars, is_active, created_at
		 FROM mcp_servers WHERE org_id = $1 ORDER BY created_at DESC`, orgID)
	if err != nil {
		return nil, fmt.Errorf("list mcp servers: %w", err)
	}

	enabled := s.userEnabledServers(ctx, userID)

	servers := make([]model.McpServerDTO, 0, len(rows))
	for _, r := range rows {
		servers = append(servers, toDTO(r, enabled[r.ID.String()]))
	}
	return servers, nil
}

// Get returns one MCP server DTO with the user's enabled state.
func (s *McpService) Get(ctx context.Context, orgID, userID, serverID uuid.UUID) (*model.McpServerDTO, error) {
	var row McpServerRow
	err := s.db.GetContext(ctx, &row,
		`SELECT id, org_id, name, server_type, endpoint, env_vars, is_active, created_at
		 FROM mcp_servers WHERE id = $1 AND org_id = $2`, serverID, orgID)
	if err != nil {
		return nil, fmt.Errorf("mcp server not found: %w", err)
	}
	enabled := s.userEnabledServers(ctx, userID)
	dto := toDTO(row, enabled[serverID.String()])
	return &dto, nil
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
		return ErrProjectNotFound
	}
	return nil
}

// SetUserEnabled enables/disables a server for a specific user by mutating
// users.mcp_server_ids (JSON array of server UUID strings). Returns the
// updated server DTO with the new enabled state.
func (s *McpService) SetUserEnabled(ctx context.Context, orgID, userID, serverID uuid.UUID, enabled bool) (*model.McpServerDTO, error) {
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

	return s.Get(ctx, orgID, userID, serverID)
}

// userEnabledServers returns the set of server IDs the user has enabled.
func (s *McpService) userEnabledServers(ctx context.Context, userID uuid.UUID) map[string]bool {
	var rawIDs []byte
	if err := s.db.GetContext(ctx, &rawIDs,
		"SELECT mcp_server_ids FROM users WHERE id = $1", userID); err != nil {
		return map[string]bool{}
	}
	var ids []string
	if len(rawIDs) > 0 {
		_ = json.Unmarshal(rawIDs, &ids)
	}
	set := make(map[string]bool, len(ids))
	for _, id := range ids {
		set[id] = true
	}
	return set
}

// toDTO maps a DB row to the frontend contract.
func toDTO(r McpServerRow, enabled bool) model.McpServerDTO {
	endpoint := ""
	if r.Endpoint.Valid {
		endpoint = r.Endpoint.String
	}
	health := "unknown"
	if !r.IsActive {
		health = "unhealthy"
	}
	return model.McpServerDTO{
		ID:           r.ID.String(),
		OrgID:        r.OrgID.String(),
		Name:         r.Name,
		EndpointURL:  endpoint,
		HealthStatus: health,
		Enabled:      enabled,
		ToolCount:    0,
		CreatedAt:    r.CreatedAt,
	}
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
