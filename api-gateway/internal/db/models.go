// Package db provides database models matching the SQL schema.
package db

import (
	"encoding/json"
	"time"

	"github.com/google/uuid"
)

// Org represents an organization/tenant.
type Org struct {
	ID        uuid.UUID `db:"id" json:"id"`
	Name      string    `db:"name" json:"name"`
	Slug      string    `db:"slug" json:"slug"`
	IsActive  bool      `db:"is_active" json:"is_active"`
	CreatedAt time.Time `db:"created_at" json:"created_at"`
	UpdatedAt time.Time `db:"updated_at" json:"updated_at"`
}

// Dept represents a department with hierarchical path.
type Dept struct {
	ID        uuid.UUID  `db:"id" json:"id"`
	OrgID     uuid.UUID  `db:"org_id" json:"org_id"`
	ParentID  *uuid.UUID `db:"parent_id" json:"parent_id"`
	Name      string     `db:"name" json:"name"`
	Path      string     `db:"path" json:"path"`
	CreatedAt time.Time  `db:"created_at" json:"created_at"`
	UpdatedAt time.Time  `db:"updated_at" json:"updated_at"`
}

// User represents a platform user.
type User struct {
	ID           uuid.UUID  `db:"id" json:"id"`
	OrgID        uuid.UUID  `db:"org_id" json:"org_id"`
	DeptID       uuid.UUID  `db:"dept_id" json:"dept_id"`
	Username     string     `db:"username" json:"username"`
	PasswordHash string     `db:"password_hash" json:"-"`
	DisplayName  *string    `db:"display_name" json:"display_name"`
	Email        *string    `db:"email" json:"email"`
	AvatarURL    *string    `db:"avatar_url" json:"avatar_url"`
	Role         string     `db:"role" json:"role"`
	IsActive     bool       `db:"is_active" json:"is_active"`
	LastLoginAt  *time.Time `db:"last_login_at" json:"last_login_at"`
	CreatedAt    time.Time  `db:"created_at" json:"created_at"`
	UpdatedAt    time.Time  `db:"updated_at" json:"updated_at"`
}

// Skill represents a registered skill/tool.
type Skill struct {
	ID               uuid.UUID       `db:"id" json:"id"`
	OrgID            uuid.UUID       `db:"org_id" json:"org_id"`
	Name             string          `db:"name" json:"name"`
	Version          string          `db:"version" json:"version"`
	Description      *string         `db:"description" json:"description"`
	PromptTemplate   *string         `db:"prompt_template" json:"prompt_template"`
	ToolDefinitions  json.RawMessage `db:"tool_definitions" json:"tool_definitions"`
	IsActive        bool            `db:"is_active" json:"is_active"`
	CreatedBy        *uuid.UUID      `db:"created_by" json:"created_by"`
	CreatedAt        time.Time       `db:"created_at" json:"created_at"`
	UpdatedAt        time.Time       `db:"updated_at" json:"updated_at"`
}

// MCPServer represents an MCP server configuration.
type MCPServer struct {
	ID          uuid.UUID       `db:"id" json:"id"`
	OrgID       uuid.UUID       `db:"org_id" json:"org_id"`
	Name        string          `db:"name" json:"name"`
	ServerType  string          `db:"server_type" json:"server_type"`
	Endpoint    *string         `db:"endpoint" json:"endpoint"`
	AuthToken   *string         `db:"auth_token" json:"-"`
	EnvVars     json.RawMessage `db:"env_vars" json:"env_vars"`
	IsActive    bool            `db:"is_active" json:"is_active"`
	CreatedBy   *uuid.UUID      `db:"created_by" json:"created_by"`
	CreatedAt   time.Time       `db:"created_at" json:"created_at"`
	UpdatedAt   time.Time       `db:"updated_at" json:"updated_at"`
}

// Role represents a custom role definition.
type Role struct {
	ID          uuid.UUID       `db:"id" json:"id"`
	OrgID       uuid.UUID       `db:"org_id" json:"org_id"`
	Name        string          `db:"name" json:"name"`
	Permissions json.RawMessage `db:"permissions" json:"permissions"`
	Description *string         `db:"description" json:"description"`
	IsSystem    bool            `db:"is_system" json:"is_system"`
	CreatedAt   time.Time       `db:"created_at" json:"created_at"`
	UpdatedAt   time.Time       `db:"updated_at" json:"updated_at"`
}

// Session represents a chat session.
type Session struct {
	ID            uuid.UUID       `db:"id" json:"id"`
	OrgID         uuid.UUID       `db:"org_id" json:"org_id"`
	UserID        uuid.UUID       `db:"user_id" json:"user_id"`
	DeptID        uuid.UUID       `db:"dept_id" json:"dept_id"`
	Title         *string         `db:"title" json:"title"`
	Model         *string         `db:"model" json:"model"`
	SkillIDs      json.RawMessage `db:"skill_ids" json:"skill_ids"`
	MCPServerIDs  json.RawMessage `db:"mcp_server_ids" json:"mcp_server_ids"`
	KBIDs         json.RawMessage `db:"kb_ids" json:"kb_ids"`
	IsArchived    bool            `db:"is_archived" json:"is_archived"`
	LastMessageAt *time.Time      `db:"last_message_at" json:"last_message_at"`
	CreatedAt     time.Time       `db:"created_at" json:"created_at"`
	UpdatedAt     time.Time       `db:"updated_at" json:"updated_at"`
}

// Message represents a single chat message.
type Message struct {
	ID           uuid.UUID       `db:"id" json:"id"`
	SessionID    uuid.UUID       `db:"session_id" json:"session_id"`
	Role         string          `db:"role" json:"role"`
	Content      string          `db:"content" json:"content"`
	Model        *string         `db:"model" json:"model"`
	FinishReason *string         `db:"finish_reason" json:"finish_reason"`
	TokenCount   *int            `db:"token_count" json:"token_count"`
	InputTokens  *int            `db:"input_tokens" json:"input_tokens"`
	OutputTokens *int            `db:"output_tokens" json:"output_tokens"`
	Metadata     json.RawMessage `db:"metadata" json:"metadata"`
	CreatedAt    time.Time       `db:"created_at" json:"created_at"`
}

// Project represents a fashion collection project.
type Project struct {
	ID          uuid.UUID  `db:"id" json:"id"`
	OrgID       uuid.UUID  `db:"org_id" json:"org_id"`
	DeptID      uuid.UUID  `db:"dept_id" json:"dept_id"`
	OwnerID     uuid.UUID  `db:"owner_id" json:"owner_id"`
	Name        string     `db:"name" json:"name"`
	Description  *string    `db:"description" json:"description"`
	CoverColor  string     `db:"cover_color" json:"cover_color"`
	IsArchived  bool       `db:"is_archived" json:"is_archived"`
	CreatedAt   time.Time  `db:"created_at" json:"created_at"`
	UpdatedAt   time.Time  `db:"updated_at" json:"updated_at"`
}

// SessionProject represents a session-project membership.
type SessionProject struct {
	SessionID uuid.UUID `db:"session_id" json:"session_id"`
	ProjectID uuid.UUID `db:"project_id" json:"project_id"`
	AddedAt   time.Time `db:"added_at" json:"added_at"`
}

// KnowledgeCollection represents a vector knowledge base.
type KnowledgeCollection struct {
	ID                uuid.UUID  `db:"id" json:"id"`
	OrgID             uuid.UUID  `db:"org_id" json:"org_id"`
	DeptID            *uuid.UUID `db:"dept_id" json:"dept_id"`
	Name              string     `db:"name" json:"name"`
	Description       *string    `db:"description" json:"description"`
	CollectionType    string     `db:"collection_type" json:"collection_type"`
	EmbeddingModel    *string    `db:"embedding_model" json:"embedding_model"`
	VectorDim         *int       `db:"vector_dim" json:"vector_dim"`
	QdrantCollection  *string    `db:"qdrant_collection" json:"qdrant_collection"`
	IsPublic          bool       `db:"is_public" json:"is_public"`
	CreatedBy         *uuid.UUID `db:"created_by" json:"created_by"`
	CreatedAt         time.Time  `db:"created_at" json:"created_at"`
	UpdatedAt         time.Time  `db:"updated_at" json:"updated_at"`
}

// AuditLog represents an audit trail entry.
type AuditLog struct {
	ID           uuid.UUID       `db:"id" json:"id"`
	OrgID        uuid.UUID       `db:"org_id" json:"org_id"`
	UserID       *uuid.UUID      `db:"user_id" json:"user_id"`
	Action       string          `db:"action" json:"action"`
	ResourceType *string         `db:"resource_type" json:"resource_type"`
	ResourceID   *uuid.UUID      `db:"resource_id" json:"resource_id"`
	Details      json.RawMessage `db:"details" json:"details"`
	IPAddress    *string         `db:"ip_address" json:"ip_address"`
	UserAgent    *string         `db:"user_agent" json:"user_agent"`
	SessionID    *uuid.UUID      `db:"session_id" json:"session_id"`
	CreatedAt    time.Time       `db:"created_at" json:"created_at"`
}
