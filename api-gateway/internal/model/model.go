// Package model defines API request/response DTOs.
// DB models live in the db package (db/models.go).
package model

import "time"

// ─── Auth ───────────────────────────────────────────────────────────────────

type RegisterRequest struct {
	OrgName   string `json:"org_name" validate:"required"`
	DeptName  string `json:"dept_name" validate:"required"`
	Username  string `json:"username" validate:"required,min=3,max=50"`
	Password  string `json:"password" validate:"required,min=8"`
	Email     string `json:"email" validate:"omitempty,email"`
	DisplayName string `json:"display_name"`
	Role      string `json:"role"`
}

type LoginRequest struct {
	Username string `json:"username"`
	Password string `json:"password"`
}

type AuthResponse struct {
	UserID      string `json:"user_id"`
	OrgID       string `json:"org_id"`
	DeptID      string `json:"dept_id"`
	Username    string `json:"username"`
	DisplayName string `json:"display_name"`
	Role        string `json:"role"`
	Token       string `json:"token"`
}

// ─── Chat ────────────────────────────────────────────────────────────────────

// ChatCompletionRequest mirrors the OpenAI chat completions request body.
// Extra fields (skill_ids, mcp_server_ids, etc.) live in ExtraBody.
type ChatCompletionRequest struct {
	Model       string                  `json:"model"`
	Messages    []ChatMessage           `json:"messages"`
	Stream      *bool                   `json:"stream,omitempty"`
	ExtraBody   map[string]interface{}  `json:"extra_body,omitempty"`
	MaxTokens   *int                    `json:"max_tokens,omitempty"`
	Temperature *float64                `json:"temperature,omitempty"`
	TopP        *float64                `json:"top_p,omitempty"`
	// UserContext is injected by the gateway from JWT claims before forwarding to core.
	UserContext *UserContext            `json:"user_context,omitempty"`
}

// UserContext carries identity info from the JWT for core service authorization.
type UserContext struct {
	UserID   string `json:"user_id"`
	OrgID    string `json:"org_id"`
	DeptID   string `json:"dept_id"`
	Role     string `json:"role"`
	IPAddress string `json:"ip_address,omitempty"`
}

type ChatMessage struct {
	Role    string `json:"role"`
	Content string `json:"content"`
	Name    string `json:"name,omitempty"`
}

type Usage struct {
	PromptTokens     int `json:"prompt_tokens"`
	CompletionTokens int `json:"completion_tokens"`
	TotalTokens     int `json:"total_tokens"`
}

type ModelEntry struct {
	ID       string `json:"id"`
	Object   string `json:"object"`
	Created  int64  `json:"created"`
	OwnedBy  string `json:"owned_by"`
	Root     string `json:"root,omitempty"`
}

type ModelList struct {
	Object string       `json:"object"`
	Data   []ModelEntry `json:"data"`
}

type ChatResponse struct {
	ID      string        `json:"id"`
	Object  string        `json:"object"`
	Created int64         `json:"created"`
	Model   string        `json:"model"`
	Choices []ChatCompletionChoice `json:"choices"`
	Usage   *Usage        `json:"usage,omitempty"`
}

type ChatCompletionChoice struct {
	Index        int          `json:"index"`
	Message      ChatMessage  `json:"message"`
	FinishReason string       `json:"finish_reason,omitempty"`
}

type ChatCompletionResponse struct {
	ID      string                  `json:"id"`
	Object  string                  `json:"object"`
	Created int64                   `json:"created"`
	Model   string                  `json:"model"`
	Choices []ChatCompletionChoice  `json:"choices"`
	Usage   *Usage                 `json:"usage,omitempty"`
}

type ChatStreamChoice struct {
	Index        int          `json:"index"`
	Delta        ChatMessage  `json:"delta"`
	FinishReason string       `json:"finish_reason,omitempty"`
}

type ChatStreamResponse struct {
	ID      string             `json:"id"`
	Object  string             `json:"object"`
	Created int64              `json:"created"`
	Model   string             `json:"model"`
	Choices []ChatStreamChoice `json:"choices"`
	Usage   *Usage            `json:"usage,omitempty"`
}

// ExtraBody holds gateway-specific fields from the chat request's `extra_body`.
type ExtraBody struct {
	SkillIDs             []string `json:"skill_ids,omitempty"`
	MCPServerIDs         []string `json:"mcp_server_ids,omitempty"`
	SessionID            string   `json:"session_id,omitempty"`
	KnowledgeCollections []string `json:"knowledge_collections,omitempty"`
}

// HealthResponse is the response body for GET /health.
type HealthResponse struct {
	Status   string                   `json:"status"`
	Version  string                  `json:"version,omitempty"`
	Services map[string]ServiceStatus `json:"services,omitempty"`
}

// ServiceStatus describes the health of a single dependency.
type ServiceStatus struct {
	Status string `json:"status"`
	OK     bool   `json:"ok"`
	Latency string `json:"latency,omitempty"`
	Error  string `json:"error,omitempty"`
}

// APIError is a structured error response.
type APIError struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

// ErrorResponse wraps APIError for JSON serialization.
type ErrorResponse struct {
	Error APIError `json:"error"`
}

// ─── Sessions ────────────────────────────────────────────────────────────────

type CreateSessionRequest struct {
	Title     string   `json:"title"`
	Model     string   `json:"model"`
	SkillIDs  []string `json:"skill_ids"`
	MCPServerIDs []string `json:"mcp_server_ids"`
	KBIDs     []string `json:"kb_ids"`
}

type UpdateSessionRequest struct {
	Title       *string `json:"title"`
	IsArchived  *bool   `json:"is_archived"`
}

type SessionWithCount struct {
	ID            string     `db:"id" json:"id"`
	OrgID         string     `db:"org_id" json:"org_id"`
	DeptID        string     `db:"dept_id" json:"dept_id"`
	UserID        string     `db:"user_id" json:"user_id"`
	Title         string     `db:"title" json:"title"`
	Model         string     `db:"model" json:"model"`
	IsArchived    bool       `db:"is_archived" json:"is_archived"`
	LastMessageAt *time.Time `db:"last_message_at" json:"last_message_at"`
	MessageCount  int        `db:"message_count" json:"message_count"`
	CreatedAt     time.Time  `db:"created_at" json:"created_at"`
	UpdatedAt     time.Time  `db:"updated_at" json:"updated_at"`
}

type SessionList struct {
	Sessions []SessionWithCount `json:"sessions"`
	Total    int                `json:"total"`
}

type MessageList struct {
	Messages   []MessageDTO `json:"messages"`
	HasMore    bool         `json:"has_more"`
	NextCursor *string      `json:"next_cursor"`
}

type MessageDTO struct {
	ID           string    `db:"id" json:"id"`
	Role         string    `db:"role" json:"role"`
	Content      string    `db:"content" json:"content"`
	Model        string    `db:"model" json:"model"`
	FinishReason string    `db:"finish_reason" json:"finish_reason"`
	TokenCount   int       `db:"token_count" json:"token_count"`
	CreatedAt    time.Time `db:"created_at" json:"created_at"`
}

// ─── Projects ─────────────────────────────────────────────────────────────────

type CreateProjectRequest struct {
	Name       string  `json:"name" validate:"required"`
	Description *string `json:"description"`
	CoverColor string  `json:"cover_color"`
}

type UpdateProjectRequest struct {
	Name        *string `json:"name"`
	Description *string `json:"description"`
	CoverColor  *string `json:"cover_color"`
}

type AddSessionToProjectRequest struct {
	SessionID string `json:"session_id" validate:"required"`
}

type ProjectDTO struct {
	ID          string    `json:"id"`
	OrgID       string    `json:"org_id"`
	DeptID      string    `json:"dept_id"`
	OwnerID     string    `json:"owner_id"`
	Name        string   `json:"name"`
	Description *string   `json:"description"`
	CoverColor  string   `json:"cover_color"`
	IsArchived  bool     `json:"is_archived"`
	CreatedAt   time.Time `json:"created_at"`
	UpdatedAt   time.Time `json:"updated_at"`
}

type ProjectDetailDTO struct {
	ProjectDTO
	Sessions []SessionWithCount `json:"sessions"`
}

type ProjectList struct {
	Projects []ProjectDTO `json:"projects"`
	Total    int           `json:"total"`
}

// ─── MCP servers (T-016) ─────────────────────────────────────────────────────

type McpServerRequest struct {
	Name       string            `json:"name" validate:"required"`
	ServerType string            `json:"server_type"` // stdio | http
	Endpoint   string            `json:"endpoint"`
	AuthToken  string            `json:"auth_token,omitempty"`
	EnvVars    map[string]string `json:"env_vars"`
}

type McpServerDTO struct {
	ID         string `db:"id" json:"id"`
	OrgID      string `db:"org_id" json:"org_id"`
	Name       string `db:"name" json:"name"`
	ServerType string `db:"server_type" json:"server_type"`
	Endpoint   *string `db:"endpoint" json:"endpoint"`
	IsActive   bool   `db:"is_active" json:"is_active"`
	CreatedAt  string `db:"created_at" json:"created_at"`
}

type McpServerList struct {
	Servers []McpServerDTO `json:"servers"`
	Total   int            `json:"total"`
}

type McpUserToggleRequest struct {
	Enabled bool `json:"enabled"`
}

type McpUserServersResponse struct {
	McpServerIDs []string `json:"mcp_server_ids"`
}

// ─── Users ───────────────────────────────────────────────────────────────────

type UserProfileResponse struct {
	ID          string     `json:"id"`
	OrgID       string     `json:"org_id"`
	DeptID      string     `json:"dept_id"`
	Username    string     `json:"username"`
	DisplayName *string    `json:"display_name"`
	Email       *string    `json:"email"`
	Role        string     `json:"role"`
	IsActive    bool       `json:"is_active"`
	CreatedAt   time.Time  `json:"created_at"`
}

type UpdateProfileRequest struct {
	DisplayName *string `json:"display_name"`
	Email       *string `json:"email"`
}

type ChangePasswordRequest struct {
	OldPassword string `json:"old_password" validate:"required"`
	NewPassword string `json:"new_password" validate:"required,min=8"`
}

type UserList struct {
	Users []UserProfileResponse `json:"users"`
	Total int                  `json:"total"`
}

// ─── Depts ───────────────────────────────────────────────────────────────────

type CreateDeptRequest struct {
	Name     string  `json:"name" validate:"required"`
	ParentID *string `json:"parent_id"`
}

type UpdateDeptRequest struct {
	Name string `json:"name"`
}

type DeptList struct {
	Depts []DeptDTO `json:"depts"`
}

type DeptDTO struct {
	ID        string  `json:"id"`
	OrgID     string  `json:"org_id"`
	ParentID  *string `json:"parent_id"`
	Name      string  `json:"name"`
	Path      string  `json:"path"`
	CreatedAt time.Time `json:"created_at"`
}

// ─── Roles ───────────────────────────────────────────────────────────────────

type CreateRoleRequest struct {
	Name        string   `json:"name" validate:"required"`
	Permissions []string `json:"permissions"`
	Description *string  `json:"description"`
}

type RoleList struct {
	Roles []RoleDTO `json:"roles"`
}

type RoleDTO struct {
	ID          string   `json:"id"`
	Name        string   `json:"name"`
	Permissions []string `json:"permissions"`
	Description *string  `json:"description"`
	IsSystem    bool     `json:"is_system"`
	CreatedAt   time.Time `json:"created_at"`
}
