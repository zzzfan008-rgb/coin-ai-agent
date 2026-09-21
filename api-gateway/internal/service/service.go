package service

import (
	"context"
	"crypto/rand"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"time"

	"github.com/google/uuid"
	"github.com/jmoiron/sqlx"
	"fashionai/api-gateway/internal/db"
	"fashionai/api-gateway/internal/model"
	"fashionai/api-gateway/pkg/auth"
)

var (
	ErrUserExists      = errors.New("username already exists")
	ErrInvalidCreds    = errors.New("invalid credentials")
	ErrUserNotFound     = errors.New("user not found")
	ErrSessionNotFound  = errors.New("session not found")
	ErrDeptNotFound     = errors.New("department not found")
	ErrProjectNotFound  = errors.New("project not found")
)

type AuthService struct {
	db    *sqlx.DB
	jwtSvc *auth.JWTService
}

func NewAuthService(db *sqlx.DB, jwtSvc *auth.JWTService) *AuthService {
	return &AuthService{db: db, jwtSvc: jwtSvc}
}

// Register creates org + dept + user in a single transaction.
func (s *AuthService) Register(ctx context.Context, req model.RegisterRequest) (*model.AuthResponse, error) {
	tx, err := s.db.BeginTxx(ctx, nil)
	if err != nil {
		return nil, fmt.Errorf("begin tx: %w", err)
	}
	defer tx.Rollback()

	// Check username uniqueness
	var exists bool
	err = tx.GetContext(ctx, &exists, "SELECT EXISTS(SELECT 1 FROM users WHERE username=$1)", req.Username)
	if err != nil {
		return nil, fmt.Errorf("check user: %w", err)
	}
	if exists {
		return nil, ErrUserExists
	}

	// Create org with a unique slug
	slug := req.Username + "-" + randomString(8)
	var orgID uuid.UUID
	err = tx.GetContext(ctx, &orgID,
		"INSERT INTO orgs(name, slug) VALUES($1,$2) RETURNING id", req.OrgName, slug)
	if err != nil {
		return nil, fmt.Errorf("create org: %w", err)
	}

	// Create root dept
	deptPath := "/" + req.DeptName
	var deptID uuid.UUID
	err = tx.GetContext(ctx, &deptID,
		"INSERT INTO depts(org_id, name, path) VALUES($1,$2,$3) RETURNING id",
		orgID, req.DeptName, deptPath)
	if err != nil {
		return nil, fmt.Errorf("create dept: %w", err)
	}

	pwHash, err := auth.HashPassword(req.Password)
	if err != nil {
		return nil, fmt.Errorf("hash password: %w", err)
	}

	displayName := req.DisplayName
	if displayName == "" {
		displayName = req.Username
	}

	var userID uuid.UUID
	err = tx.GetContext(ctx, &userID,
		`INSERT INTO users(org_id, dept_id, username, password_hash, display_name, email, role)
		 VALUES($1,$2,$3,$4,$5,$6,'designer') RETURNING id`,
		orgID, deptID, req.Username, pwHash, displayName, req.Email)
	if err != nil {
		return nil, fmt.Errorf("create user: %w", err)
	}

	if err := tx.Commit(); err != nil {
		return nil, fmt.Errorf("commit tx: %w", err)
	}

	token, err := s.jwtSvc.Generate(userID.String(), orgID.String(), deptID.String(), "designer")
	if err != nil {
		return nil, fmt.Errorf("sign token: %w", err)
	}

	return &model.AuthResponse{
		UserID:      userID.String(),
		OrgID:       orgID.String(),
		DeptID:      deptID.String(),
		Username:    req.Username,
		DisplayName: displayName,
		Role:        "designer",
		Token:       token,
	}, nil
}

// Login verifies credentials and returns a JWT.
func (s *AuthService) Login(ctx context.Context, req model.LoginRequest) (*model.AuthResponse, error) {
	var user struct {
		ID          uuid.UUID `db:"id"`
		OrgID       uuid.UUID `db:"org_id"`
		DeptID      uuid.UUID `db:"dept_id"`
		Password    string    `db:"password_hash"`
		DisplayName sql.NullString
		Role        string `db:"role"`
	}
	err := s.db.GetContext(ctx, &user,
		`SELECT id, org_id, dept_id, password_hash, display_name, role
		 FROM users WHERE username=$1 AND is_active=true`,
		req.Username)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrInvalidCreds
		}
		return nil, fmt.Errorf("query user: %w", err)
	}

	if !auth.CheckPassword(req.Password, user.Password) {
		return nil, ErrInvalidCreds
	}

	token, err := s.jwtSvc.Generate(user.ID.String(), user.OrgID.String(), user.DeptID.String(), user.Role)
	if err != nil {
		return nil, fmt.Errorf("sign token: %w", err)
	}

	displayName := ""
	if user.DisplayName.Valid {
		displayName = user.DisplayName.String
	}

	return &model.AuthResponse{
		UserID:      user.ID.String(),
		OrgID:       user.OrgID.String(),
		DeptID:      user.DeptID.String(),
		Username:    req.Username,
		DisplayName: displayName,
		Role:        user.Role,
		Token:       token,
	}, nil
}

// UserService handles user CRUD.
type UserService struct {
	db *sqlx.DB
}

func NewUserService(db *sqlx.DB) *UserService {
	return &UserService{db: db}
}

func (s *UserService) List(ctx context.Context, orgID uuid.UUID) (*model.UserList, error) {
	type row struct {
		ID          uuid.UUID `db:"id"`
		OrgID       uuid.UUID `db:"org_id"`
		DeptID      uuid.UUID `db:"dept_id"`
		Username    string    `db:"username"`
		DisplayName *string   `db:"display_name"`
		Email       *string   `db:"email"`
		Role        string    `db:"role"`
		IsActive    bool      `db:"is_active"`
		CreatedAt   time.Time `db:"created_at"`
	}
	var rows []row
	err := s.db.SelectContext(ctx, &rows,
		`SELECT u.id, u.org_id, u.dept_id, u.username, u.display_name, u.email, u.role, u.is_active, u.created_at
		 FROM users u WHERE u.org_id=$1 ORDER BY u.created_at DESC`, orgID)
	if err != nil {
		return nil, err
	}
	users := make([]model.UserProfileResponse, len(rows))
	for i, r := range rows {
		users[i] = model.UserProfileResponse{
			ID:          r.ID.String(),
			OrgID:       r.OrgID.String(),
			DeptID:      r.DeptID.String(),
			Username:    r.Username,
			DisplayName: r.DisplayName,
			Email:       r.Email,
			Role:        r.Role,
			IsActive:    r.IsActive,
			CreatedAt:   r.CreatedAt,
		}
	}
	return &model.UserList{Users: users, Total: len(users)}, nil
}

// DeptService handles department CRUD.
type DeptService struct {
	db *sqlx.DB
}

func NewDeptService(db *sqlx.DB) *DeptService {
	return &DeptService{db: db}
}

func (s *DeptService) List(ctx context.Context, orgID uuid.UUID) (*model.DeptList, error) {
	var depts []db.Dept
	err := s.db.SelectContext(ctx, &depts,
		`SELECT id, name, parent_id, path, created_at FROM depts WHERE org_id=$1 ORDER BY path`, orgID)
	if err != nil {
		return nil, err
	}
	result := make([]model.DeptDTO, len(depts))
	for i, d := range depts {
		var parentID *string
		if d.ParentID != nil {
			s := d.ParentID.String()
			parentID = &s
		}
		result[i] = model.DeptDTO{
			ID:        d.ID.String(),
			Name:      d.Name,
			ParentID:  parentID,
			Path:      d.Path,
			CreatedAt: d.CreatedAt,
		}
	}
	return &model.DeptList{Depts: result}, nil
}

func (s *DeptService) Create(ctx context.Context, orgID uuid.UUID, req model.CreateDeptRequest) (*model.DeptDTO, error) {
	var parentPath string
	if req.ParentID != nil && *req.ParentID != "" {
		var p string
		err := s.db.GetContext(ctx, &p,
			"SELECT path FROM depts WHERE id=$1 AND org_id=$2", *req.ParentID, orgID)
		if err != nil {
			return nil, ErrDeptNotFound
		}
		parentPath = p
	}
	path := parentPath + "/" + req.Name

	var dept db.Dept
	err := s.db.GetContext(ctx, &dept,
		`INSERT INTO depts(org_id, parent_id, name, path) VALUES($1,$2,$3,$4)
		 RETURNING id, name, parent_id, path, created_at`,
		orgID, req.ParentID, req.Name, path)
	if err != nil {
		return nil, fmt.Errorf("create dept: %w", err)
	}
	var parentIDStr *string
	if dept.ParentID != nil {
		s := dept.ParentID.String()
		parentIDStr = &s
	}
	return &model.DeptDTO{
		ID:        dept.ID.String(),
		Name:      dept.Name,
		ParentID:  parentIDStr,
		Path:      dept.Path,
		CreatedAt: dept.CreatedAt,
	}, nil
}

// RoleService handles role CRUD.
type RoleService struct {
	db *sqlx.DB
}

func NewRoleService(db *sqlx.DB) *RoleService {
	return &RoleService{db: db}
}

func (s *RoleService) List(ctx context.Context, orgID uuid.UUID) (*model.RoleList, error) {
	var roles []db.Role
	err := s.db.SelectContext(ctx, &roles,
		`SELECT id, name, permissions, created_at FROM roles WHERE org_id=$1`, orgID)
	if err != nil {
		return nil, err
	}
	result := make([]model.RoleDTO, len(roles))
	for i, r := range roles {
		var perms []string
		_ = json.Unmarshal(r.Permissions, &perms)
		result[i] = model.RoleDTO{
			ID:          r.ID.String(),
			Name:        r.Name,
			Permissions: perms,
			CreatedAt:   r.CreatedAt,
		}
	}
	return &model.RoleList{Roles: result}, nil
}

func (s *RoleService) Create(ctx context.Context, orgID uuid.UUID, req model.CreateRoleRequest) (*model.RoleDTO, error) {
	permsJSON := "[]"
	if len(req.Permissions) > 0 {
		permsJSON = `["` + joinStrings(req.Permissions, `","`) + `"]`
	}

	var role db.Role
	err := s.db.GetContext(ctx, &role,
		`INSERT INTO roles(org_id, name, permissions) VALUES($1,$2,$3::jsonb)
		 RETURNING id, name, permissions, created_at`,
		orgID, req.Name, permsJSON)
	if err != nil {
		return nil, fmt.Errorf("create role: %w", err)
	}
	var perms []string
	_ = json.Unmarshal(role.Permissions, &perms)
	return &model.RoleDTO{
		ID:          role.ID.String(),
		Name:        role.Name,
		Permissions: perms,
		CreatedAt:   role.CreatedAt,
	}, nil
}

// SessionService handles session CRUD + message persistence.
type SessionService struct {
	db *sqlx.DB
}

func NewSessionService(db *sqlx.DB) *SessionService {
	return &SessionService{db: db}
}

func (s *SessionService) Create(ctx context.Context, orgID, userID uuid.UUID, req model.CreateSessionRequest) (*model.SessionWithCount, error) {
	title := req.Title
	if title == "" {
		title = fmt.Sprintf("Session %s", time.Now().Format("2006-01-02 15:04"))
	}
	var row struct {
		ID         uuid.UUID `db:"id"`
		OrgID      uuid.UUID `db:"org_id"`
		UserID     uuid.UUID `db:"user_id"`
		Title      string    `db:"title"`
		IsArchived bool      `db:"is_archived"`
		CreatedAt  time.Time `db:"created_at"`
		UpdatedAt  time.Time `db:"updated_at"`
	}
	err := s.db.GetContext(ctx, &row,
		`INSERT INTO sessions(org_id, user_id, title) VALUES($1,$2,$3)
		 RETURNING id, org_id, user_id, title, is_archived, created_at, updated_at`,
		orgID, userID, title)
	if err != nil {
		return nil, fmt.Errorf("create session: %w", err)
	}
	return &model.SessionWithCount{
		ID:         row.ID.String(),
		OrgID:      row.OrgID.String(),
		UserID:     row.UserID.String(),
		Title:      row.Title,
		IsArchived: row.IsArchived,
		CreatedAt:  row.CreatedAt,
		UpdatedAt:  row.UpdatedAt,
	}, nil
}

func (s *SessionService) Get(ctx context.Context, orgID, sessionID uuid.UUID) (*model.SessionWithCount, error) {
	var row struct {
		ID            uuid.UUID `db:"id"`
		OrgID         uuid.UUID `db:"org_id"`
		UserID        uuid.UUID `db:"user_id"`
		Title         string    `db:"title"`
		IsArchived    bool      `db:"is_archived"`
		MessageCount  int       `db:"message_count"`
		CreatedAt     time.Time `db:"created_at"`
		UpdatedAt     time.Time `db:"updated_at"`
	}
	err := s.db.GetContext(ctx, &row,
		`SELECT s.id, s.org_id, s.user_id, s.title, s.is_archived,
		        (SELECT COUNT(*) FROM messages m WHERE m.session_id=s.id) AS message_count,
		        s.created_at, s.updated_at
		 FROM sessions s WHERE s.id=$1 AND s.org_id=$2`,
		sessionID, orgID)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrSessionNotFound
		}
		return nil, err
	}
	return &model.SessionWithCount{
		ID:            row.ID.String(),
		OrgID:         row.OrgID.String(),
		UserID:        row.UserID.String(),
		Title:         row.Title,
		IsArchived:    row.IsArchived,
		MessageCount:   row.MessageCount,
		CreatedAt:     row.CreatedAt,
		UpdatedAt:     row.UpdatedAt,
	}, nil
}

func (s *SessionService) List(ctx context.Context, orgID, userID uuid.UUID, archived bool, limit, offset int) (*model.SessionList, error) {
	type row struct {
		ID            uuid.UUID `db:"id"`
		OrgID         uuid.UUID `db:"org_id"`
		UserID        uuid.UUID `db:"user_id"`
		Title         string    `db:"title"`
		IsArchived    bool      `db:"is_archived"`
		MessageCount  int       `db:"message_count"`
		CreatedAt     time.Time `db:"created_at"`
		UpdatedAt     time.Time `db:"updated_at"`
	}
	var rows []row
	err := s.db.SelectContext(ctx, &rows,
		`SELECT s.id, s.org_id, s.user_id, s.title, s.is_archived,
		        (SELECT COUNT(*) FROM messages m WHERE m.session_id=s.id) AS message_count,
		        s.created_at, s.updated_at
		 FROM sessions s
		 WHERE s.org_id=$1 AND s.user_id=$2 AND s.is_archived=$3
		 ORDER BY s.updated_at DESC LIMIT $4 OFFSET $5`,
		orgID, userID, archived, limit, offset)
	if err != nil {
		return nil, err
	}

	var total int
	_ = s.db.GetContext(ctx, &total,
		`SELECT COUNT(*) FROM sessions WHERE org_id=$1 AND user_id=$2 AND is_archived=$3`,
		orgID, userID, archived)

	sessions := make([]model.SessionWithCount, len(rows))
	for i, r := range rows {
		sessions[i] = model.SessionWithCount{
			ID:           r.ID.String(),
			OrgID:        r.OrgID.String(),
			UserID:       r.UserID.String(),
			Title:        r.Title,
			IsArchived:   r.IsArchived,
			MessageCount: r.MessageCount,
			CreatedAt:    r.CreatedAt,
			UpdatedAt:    r.UpdatedAt,
		}
	}
	return &model.SessionList{Sessions: sessions, Total: total}, nil
}

func (s *SessionService) Update(ctx context.Context, orgID, sessionID uuid.UUID, req model.UpdateSessionRequest) (*model.SessionWithCount, error) {
	setClauses := []string{"updated_at=NOW()"}
	args := []any{}
	argIdx := 1

	if req.Title != nil {
		setClauses = append(setClauses, fmt.Sprintf("title=$%d", argIdx))
		args = append(args, *req.Title)
		argIdx++
	}
	if req.IsArchived != nil {
		setClauses = append(setClauses, fmt.Sprintf("is_archived=$%d", argIdx))
		args = append(args, *req.IsArchived)
		argIdx++
	}
	args = append(args, sessionID, orgID)

	query := fmt.Sprintf(
		`UPDATE sessions SET %s WHERE id=$%d AND org_id=$%d
		 RETURNING id, org_id, user_id, title, is_archived, created_at, updated_at`,
		joinStrings(setClauses, ", "), argIdx, argIdx+1)

	var row struct {
		ID         uuid.UUID `db:"id"`
		OrgID      uuid.UUID `db:"org_id"`
		UserID     uuid.UUID `db:"user_id"`
		Title      string    `db:"title"`
		IsArchived bool      `db:"is_archived"`
		CreatedAt  time.Time `db:"created_at"`
		UpdatedAt  time.Time `db:"updated_at"`
	}
	err := s.db.GetContext(ctx, &row, query, args...)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrSessionNotFound
		}
		return nil, err
	}
	return &model.SessionWithCount{
		ID:         row.ID.String(),
		OrgID:      row.OrgID.String(),
		UserID:     row.UserID.String(),
		Title:      row.Title,
		IsArchived: row.IsArchived,
		CreatedAt:  row.CreatedAt,
		UpdatedAt:  row.UpdatedAt,
	}, nil
}

func (s *SessionService) GetMessages(ctx context.Context, sessionID uuid.UUID, limit int, before string) (*model.MessageList, error) {
	var rows []model.MessageDTO
	var err error
	if before != "" {
		err = s.db.SelectContext(ctx, &rows,
			`SELECT id, role, content, model, finish_reason, token_count, created_at
			 FROM messages WHERE session_id=$1 AND id < $2
			 ORDER BY created_at DESC LIMIT $3`,
			sessionID, before, limit)
	} else {
		err = s.db.SelectContext(ctx, &rows,
			`SELECT id, role, content, model, finish_reason, token_count, created_at
			 FROM messages WHERE session_id=$1
			 ORDER BY created_at DESC LIMIT $2`,
			sessionID, limit)
	}
	if err != nil {
		return nil, err
	}

	// Reverse to chronological order
	for i, j := 0, len(rows)-1; i < j; i, j = i+1, j-1 {
		rows[i], rows[j] = rows[j], rows[i]
	}

	var hasMore bool
	var nextCursor *string
	if len(rows) == limit {
		hasMore = true
		if len(rows) > 0 {
			lastID := rows[len(rows)-1].ID
			nextCursor = &lastID
		}
	}
	return &model.MessageList{Messages: rows, HasMore: hasMore, NextCursor: nextCursor}, nil
}

func (s *SessionService) SaveMessage(ctx context.Context, sessionID uuid.UUID, role, content, modelName string) error {
	_, err := s.db.ExecContext(ctx,
		`INSERT INTO messages(session_id, role, content, model) VALUES($1,$2,$3,$4)`,
		sessionID, role, content, modelName)
	if err != nil {
		return err
	}
	s.db.ExecContext(ctx, `UPDATE sessions SET updated_at=NOW() WHERE id=$1`, sessionID)
	return nil
}

// AuditService writes audit logs.
type AuditService struct {
	db *sqlx.DB
}

func NewAuditService(db *sqlx.DB) *AuditService {
	return &AuditService{db: db}
}

func (s *AuditService) Log(ctx context.Context, orgID, userID uuid.UUID, action, resourceType string, resourceID *uuid.UUID, ip string) {
	s.db.ExecContext(ctx,
		`INSERT INTO audit_logs(org_id, user_id, action, resource_type, resource_id, ip_address)
		 VALUES($1,$2,$3,$4,$5,$6)`,
		orgID, userID, action, resourceType, resourceID, ip)
}

// ProjectService handles project CRUD.
type ProjectService struct {
	db *sqlx.DB
}

func NewProjectService(db *sqlx.DB) *ProjectService {
	return &ProjectService{db: db}
}

func (s *ProjectService) Create(ctx context.Context, orgID, deptID, ownerID uuid.UUID, req model.CreateProjectRequest) (*model.ProjectDTO, error) {
	coverColor := req.CoverColor
	if coverColor == "" {
		coverColor = "#6366F1"
	}
	var row struct {
		ID          uuid.UUID `db:"id"`
		OrgID       uuid.UUID `db:"org_id"`
		DeptID      uuid.UUID `db:"dept_id"`
		OwnerID     uuid.UUID `db:"owner_id"`
		Name        string    `db:"name"`
		Description *string   `db:"description"`
		CoverColor  string    `db:"cover_color"`
		IsArchived  bool      `db:"is_archived"`
		CreatedAt   time.Time `db:"created_at"`
		UpdatedAt   time.Time `db:"updated_at"`
	}
	err := s.db.GetContext(ctx, &row,
		`INSERT INTO projects(org_id, dept_id, owner_id, name, description, cover_color)
		 VALUES($1,$2,$3,$4,$5,$6)
		 RETURNING id, org_id, dept_id, owner_id, name, description, cover_color,
		           is_archived, created_at, updated_at`,
		orgID, deptID, ownerID, req.Name, req.Description, coverColor)
	if err != nil {
		return nil, fmt.Errorf("create project: %w", err)
	}
	return &model.ProjectDTO{
		ID:          row.ID.String(),
		OrgID:       row.OrgID.String(),
		DeptID:      row.DeptID.String(),
		OwnerID:     row.OwnerID.String(),
		Name:        row.Name,
		Description: row.Description,
		CoverColor:  row.CoverColor,
		IsArchived:  row.IsArchived,
		CreatedAt:   row.CreatedAt,
		UpdatedAt:   row.UpdatedAt,
	}, nil
}

func (s *ProjectService) List(ctx context.Context, orgID, deptID uuid.UUID, archived bool) (*model.ProjectList, error) {
	type row struct {
		ID          uuid.UUID `db:"id"`
		OrgID       uuid.UUID `db:"org_id"`
		DeptID      uuid.UUID `db:"dept_id"`
		OwnerID     uuid.UUID `db:"owner_id"`
		Name        string    `db:"name"`
		Description *string   `db:"description"`
		CoverColor  string    `db:"cover_color"`
		IsArchived  bool      `db:"is_archived"`
		CreatedAt   time.Time `db:"created_at"`
		UpdatedAt   time.Time `db:"updated_at"`
	}
	var rows []row
	err := s.db.SelectContext(ctx, &rows,
		`SELECT id, org_id, dept_id, owner_id, name, description, cover_color,
		        is_archived, created_at, updated_at
		 FROM projects
		 WHERE dept_id=$1 AND is_archived=$2
		 ORDER BY updated_at DESC`,
		deptID, archived)
	if err != nil {
		return nil, err
	}
	projects := make([]model.ProjectDTO, len(rows))
	for i, r := range rows {
		projects[i] = model.ProjectDTO{
			ID:          r.ID.String(),
			OrgID:       r.OrgID.String(),
			DeptID:      r.DeptID.String(),
			OwnerID:     r.OwnerID.String(),
			Name:        r.Name,
			Description: r.Description,
			CoverColor:  r.CoverColor,
			IsArchived:  r.IsArchived,
			CreatedAt:   r.CreatedAt,
			UpdatedAt:   r.UpdatedAt,
		}
	}
	return &model.ProjectList{Projects: projects, Total: len(projects)}, nil
}

func (s *ProjectService) Get(ctx context.Context, deptID, projectID uuid.UUID) (*model.ProjectDetailDTO, error) {
	type row struct {
		ID          uuid.UUID `db:"id"`
		OrgID       uuid.UUID `db:"org_id"`
		DeptID      uuid.UUID `db:"dept_id"`
		OwnerID     uuid.UUID `db:"owner_id"`
		Name        string    `db:"name"`
		Description *string   `db:"description"`
		CoverColor  string    `db:"cover_color"`
		IsArchived  bool      `db:"is_archived"`
		CreatedAt   time.Time `db:"created_at"`
		UpdatedAt   time.Time `db:"updated_at"`
	}
	var p row
	err := s.db.GetContext(ctx, &p,
		`SELECT id, org_id, dept_id, owner_id, name, description, cover_color,
		        is_archived, created_at, updated_at
		 FROM projects WHERE id=$1 AND dept_id=$2`,
		projectID, deptID)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrProjectNotFound
		}
		return nil, err
	}

	var sessions []model.SessionWithCount
	_ = s.db.SelectContext(ctx, &sessions,
		`SELECT s.id, s.org_id, s.user_id, s.title, s.is_archived,
		        (SELECT COUNT(*) FROM messages m WHERE m.session_id=s.id) AS message_count,
		        s.created_at, s.updated_at
		 FROM session_project sp
		 JOIN sessions s ON s.id = sp.session_id
		 WHERE sp.project_id = $1
		 ORDER BY sp.added_at DESC`, projectID)

	return &model.ProjectDetailDTO{
		ProjectDTO: model.ProjectDTO{
			ID:          p.ID.String(),
			OrgID:       p.OrgID.String(),
			DeptID:      p.DeptID.String(),
			OwnerID:     p.OwnerID.String(),
			Name:        p.Name,
			Description: p.Description,
			CoverColor:  p.CoverColor,
			IsArchived:  p.IsArchived,
			CreatedAt:   p.CreatedAt,
			UpdatedAt:   p.UpdatedAt,
		},
		Sessions: sessions,
	}, nil
}

func (s *ProjectService) Update(ctx context.Context, deptID, projectID uuid.UUID, req model.UpdateProjectRequest) (*model.ProjectDTO, error) {
	setClauses := []string{"updated_at=NOW()"}
	args := []any{}
	argIdx := 1

	if req.Name != nil {
		setClauses = append(setClauses, fmt.Sprintf("name=$%d", argIdx))
		args = append(args, *req.Name)
		argIdx++
	}
	if req.Description != nil {
		setClauses = append(setClauses, fmt.Sprintf("description=$%d", argIdx))
		args = append(args, *req.Description)
		argIdx++
	}
	if req.CoverColor != nil {
		setClauses = append(setClauses, fmt.Sprintf("cover_color=$%d", argIdx))
		args = append(args, *req.CoverColor)
		argIdx++
	}
	args = append(args, projectID, deptID)

	query := fmt.Sprintf(
		`UPDATE projects SET %s WHERE id=$%d AND dept_id=$%d
		 RETURNING id, org_id, dept_id, owner_id, name, description, cover_color,
		           is_archived, created_at, updated_at`,
		joinStrings(setClauses, ", "), argIdx, argIdx+1)

	type row struct {
		ID          uuid.UUID `db:"id"`
		OrgID       uuid.UUID `db:"org_id"`
		DeptID      uuid.UUID `db:"dept_id"`
		OwnerID     uuid.UUID `db:"owner_id"`
		Name        string    `db:"name"`
		Description *string   `db:"description"`
		CoverColor  string    `db:"cover_color"`
		IsArchived  bool      `db:"is_archived"`
		CreatedAt   time.Time `db:"created_at"`
		UpdatedAt   time.Time `db:"updated_at"`
	}
	var r row
	err := s.db.GetContext(ctx, &r, query, args...)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrProjectNotFound
		}
		return nil, err
	}
	return &model.ProjectDTO{
		ID:          r.ID.String(),
		OrgID:       r.OrgID.String(),
		DeptID:      r.DeptID.String(),
		OwnerID:     r.OwnerID.String(),
		Name:        r.Name,
		Description: r.Description,
		CoverColor:  r.CoverColor,
		IsArchived:  r.IsArchived,
		CreatedAt:   r.CreatedAt,
		UpdatedAt:   r.UpdatedAt,
	}, nil
}

func (s *ProjectService) Archive(ctx context.Context, deptID, projectID uuid.UUID) (*model.ProjectDTO, error) {
	return s.setArchived(ctx, deptID, projectID, true)
}

func (s *ProjectService) Unarchive(ctx context.Context, deptID, projectID uuid.UUID) (*model.ProjectDTO, error) {
	return s.setArchived(ctx, deptID, projectID, false)
}

func (s *ProjectService) setArchived(ctx context.Context, deptID, projectID uuid.UUID, archived bool) (*model.ProjectDTO, error) {
	type row struct {
		ID          uuid.UUID `db:"id"`
		OrgID       uuid.UUID `db:"org_id"`
		DeptID      uuid.UUID `db:"dept_id"`
		OwnerID     uuid.UUID `db:"owner_id"`
		Name        string    `db:"name"`
		Description *string   `db:"description"`
		CoverColor  string    `db:"cover_color"`
		IsArchived  bool      `db:"is_archived"`
		CreatedAt   time.Time `db:"created_at"`
		UpdatedAt   time.Time `db:"updated_at"`
	}
	var r row
	err := s.db.GetContext(ctx, &r,
		`UPDATE projects SET is_archived=$1, updated_at=NOW()
		 WHERE id=$2 AND dept_id=$3
		 RETURNING id, org_id, dept_id, owner_id, name, description, cover_color,
		           is_archived, created_at, updated_at`,
		archived, projectID, deptID)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrProjectNotFound
		}
		return nil, err
	}
	return &model.ProjectDTO{
		ID:          r.ID.String(),
		OrgID:       r.OrgID.String(),
		DeptID:      r.DeptID.String(),
		OwnerID:     r.OwnerID.String(),
		Name:        r.Name,
		Description: r.Description,
		CoverColor:  r.CoverColor,
		IsArchived:  r.IsArchived,
		CreatedAt:   r.CreatedAt,
		UpdatedAt:   r.UpdatedAt,
	}, nil
}

func (s *ProjectService) Delete(ctx context.Context, deptID, projectID uuid.UUID) error {
	result, err := s.db.ExecContext(ctx,
		`UPDATE projects SET is_archived=true, updated_at=NOW() WHERE id=$1 AND dept_id=$2`,
		projectID, deptID)
	if err != nil {
		return err
	}
	n, _ := result.RowsAffected()
	if n == 0 {
		return ErrProjectNotFound
	}
	return nil
}

func (s *ProjectService) AddSession(ctx context.Context, projectID, sessionID uuid.UUID) error {
	_, err := s.db.ExecContext(ctx,
		`INSERT INTO session_project(session_id, project_id)
		 VALUES($1,$2)
		 ON CONFLICT (session_id, project_id) DO NOTHING`,
		sessionID, projectID)
	return err
}

func (s *ProjectService) RemoveSession(ctx context.Context, projectID, sessionID uuid.UUID) error {
	_, err := s.db.ExecContext(ctx,
		`DELETE FROM session_project WHERE session_id=$1 AND project_id=$2`,
		sessionID, projectID)
	return err
}

func (s *ProjectService) GetProjectsForSession(ctx context.Context, sessionID, deptID uuid.UUID) ([]model.ProjectDTO, error) {
	type row struct {
		ID          uuid.UUID `db:"id"`
		OrgID       uuid.UUID `db:"org_id"`
		DeptID      uuid.UUID `db:"dept_id"`
		OwnerID     uuid.UUID `db:"owner_id"`
		Name        string    `db:"name"`
		Description *string   `db:"description"`
		CoverColor  string    `db:"cover_color"`
		IsArchived  bool      `db:"is_archived"`
		CreatedAt   time.Time `db:"created_at"`
		UpdatedAt   time.Time `db:"updated_at"`
	}
	var rows []row
	_ = s.db.SelectContext(ctx, &rows,
		`SELECT p.id, p.org_id, p.dept_id, p.owner_id, p.name, p.description,
		        p.cover_color, p.is_archived, p.created_at, p.updated_at
		 FROM session_project sp
		 JOIN projects p ON p.id = sp.project_id
		 WHERE sp.session_id=$1 AND p.dept_id=$2`, sessionID, deptID)
	result := make([]model.ProjectDTO, len(rows))
	for i, r := range rows {
		result[i] = model.ProjectDTO{
			ID:          r.ID.String(),
			OrgID:       r.OrgID.String(),
			DeptID:      r.DeptID.String(),
			OwnerID:     r.OwnerID.String(),
			Name:        r.Name,
			Description: r.Description,
			CoverColor:  r.CoverColor,
			IsArchived:  r.IsArchived,
			CreatedAt:   r.CreatedAt,
			UpdatedAt:   r.UpdatedAt,
		}
	}
	return result, nil
}

func joinStrings(parts []string, sep string) string {
	if len(parts) == 0 {
		return ""
	}
	result := parts[0]
	for _, p := range parts[1:] {
		result += sep + p
	}
	return result
}

// randomString generates a random lowercase hex string of n bytes.
func randomString(n int) string {
	b := make([]byte, n)
	_, _ = rand.Read(b)
	return hex.EncodeToString(b)[:n]
}
