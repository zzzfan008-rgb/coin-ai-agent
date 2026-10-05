package handler

import (
	"bytes"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"regexp"
	"testing"

	"github.com/DATA-DOG/go-sqlmock"
	"github.com/google/uuid"
	"github.com/jmoiron/sqlx"
	"github.com/lib/pq"

	"fashionai/api-gateway/internal/model"
	"fashionai/api-gateway/internal/service"
	"fashionai/api-gateway/pkg/auth"
)

// TestAuthHandler_Register_DuplicateUsername_Conflict 覆盖 T-020 的目标路径：
// 相同 username 二次注册 → service 返回 ErrUserExists → handler 返回 409
// "username already exists"。handler 依赖的是 *service.AuthService 具型
// （未导出字段），无法直接注入 mock；这里用 sqlmock 构造真实 AuthService
// （EXISTS 返回 true）驱动 handler→service 完整路径，不触碰真实 PostgreSQL。
func TestAuthHandler_Register_DuplicateUsername_Conflict(t *testing.T) {
	db, mock, err := sqlmock.New()
	if err != nil {
		t.Fatalf("sqlmock.New: %v", err)
	}
	t.Cleanup(func() { db.Close() })

	svc := service.NewAuthService(sqlx.NewDb(db, "sqlmock"), auth.NewJWTService("test-secret", 24), false)
	h := &AuthHandler{svc: svc}

	mock.ExpectBegin()
	mock.ExpectQuery(regexp.QuoteMeta("SELECT EXISTS(SELECT 1 FROM users WHERE username=$1)")).
		WithArgs("alice").
		WillReturnRows(sqlmock.NewRows([]string{"exists"}).AddRow(true))
	mock.ExpectRollback()

	body := `{"username":"alice","password":"password123","org_name":"acme","dept_name":"design"}`
	req := httptest.NewRequest(http.MethodPost, "/auth/register", bytes.NewBufferString(body))
	req.Header.Set("Content-Type", "application/json")
	rr := httptest.NewRecorder()

	h.Register(rr, req)

	if rr.Code != http.StatusConflict {
		t.Fatalf("got status %d, want %d", rr.Code, http.StatusConflict)
	}

	var resp model.ErrorResponse
	if err := json.NewDecoder(rr.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode error response: %v", err)
	}
	if resp.Error.Code != "CONFLICT" {
		t.Errorf("error.code = %q, want %q", resp.Error.Code, "CONFLICT")
	}
	if resp.Error.Message != "username already exists" {
		t.Errorf("error.message = %q, want %q", resp.Error.Message, "username already exists")
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}

// guard: the handler must surface a mock-constructed service error as 500,
// proving the 409 above comes from the ErrUserExists branch, not a fallback.
func TestAuthHandler_Register_UnexpectedServiceError_500(t *testing.T) {
	db, mock, err := sqlmock.New()
	if err != nil {
		t.Fatalf("sqlmock.New: %v", err)
	}
	t.Cleanup(func() { db.Close() })

	svc := service.NewAuthService(sqlx.NewDb(db, "sqlmock"), auth.NewJWTService("test-secret", 24), false)
	h := &AuthHandler{svc: svc}

	mock.ExpectBegin()
	mock.ExpectQuery(regexp.QuoteMeta("SELECT EXISTS(SELECT 1 FROM users WHERE username=$1)")).
		WithArgs("alice").
		WillReturnError(errors.New("check user: simulated db failure"))
	mock.ExpectRollback()

	body := `{"username":"alice","password":"password123","org_name":"acme","dept_name":"design"}`
	req := httptest.NewRequest(http.MethodPost, "/auth/register", bytes.NewBufferString(body))
	req.Header.Set("Content-Type", "application/json")
	rr := httptest.NewRecorder()

	h.Register(rr, req)

	if rr.Code != http.StatusInternalServerError {
		t.Fatalf("got status %d, want %d", rr.Code, http.StatusInternalServerError)
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}

// TestAuthHandler_Register_UniqueViolation_GlobalConstraint_Conflict 覆盖
// T-020 并发收口在完整 handler 路径上的表现。021 之前，INSERT 阶段的 23505
// 会走通用 writeError 变成 500；现在 uq_users_username_global 命中必须
// 映射为 409（code CONFLICT，message "username already exists"）。
// 同样用 sqlmock 驱动 handler→service 全路径，不触碰真实 PostgreSQL。
func TestAuthHandler_Register_UniqueViolation_GlobalConstraint_Conflict(t *testing.T) {
	db, mock, err := sqlmock.New(sqlmock.QueryMatcherOption(sqlmock.QueryMatcherRegexp))
	if err != nil {
		t.Fatalf("sqlmock.New: %v", err)
	}
	t.Cleanup(func() { db.Close() })

	svc := service.NewAuthService(sqlx.NewDb(db, "sqlmock"), auth.NewJWTService("test-secret", 24), false)
	h := &AuthHandler{svc: svc}

	orgID := uuid.MustParse("00000000-0000-0000-0000-0000000000a1")
	deptID := uuid.MustParse("00000000-0000-0000-0000-0000000000b2")

	mock.ExpectBegin()
	mock.ExpectQuery(regexp.QuoteMeta("SELECT EXISTS(SELECT 1 FROM users WHERE username=$1)")).
		WithArgs("alice").
		WillReturnRows(sqlmock.NewRows([]string{"exists"}).AddRow(false))
	// org + dept created successfully; the racer lost at the users INSERT.
	mock.ExpectQuery(regexp.QuoteMeta("INSERT INTO orgs(name, slug) VALUES($1,$2) RETURNING id")).
		WithArgs("acme", sqlmock.AnyArg()).
		WillReturnRows(sqlmock.NewRows([]string{"id"}).AddRow(orgID))
	mock.ExpectQuery(regexp.QuoteMeta("INSERT INTO depts(org_id, name, path) VALUES($1,$2,$3) RETURNING id")).
		WithArgs(orgID, "design", "/design").
		WillReturnRows(sqlmock.NewRows([]string{"id"}).AddRow(deptID))
	mock.ExpectQuery(regexp.QuoteMeta("INSERT INTO users(org_id, dept_id, username, password_hash, display_name, email, role)")).
		WithArgs(orgID, deptID, "alice", sqlmock.AnyArg(), "alice", sqlmock.AnyArg()).
		WillReturnError(&pq.Error{Code: "23505", Constraint: "uq_users_username_global"})
	mock.ExpectRollback()

	body := `{"username":"alice","password":"password123","org_name":"acme","dept_name":"design"}`
	req := httptest.NewRequest(http.MethodPost, "/auth/register", bytes.NewBufferString(body))
	req.Header.Set("Content-Type", "application/json")
	rr := httptest.NewRecorder()

	h.Register(rr, req)

	if rr.Code != http.StatusConflict {
		t.Fatalf("got status %d, want %d", rr.Code, http.StatusConflict)
	}
	var resp model.ErrorResponse
	if err := json.NewDecoder(rr.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode error response: %v", err)
	}
	if resp.Error.Code != "CONFLICT" {
		t.Errorf("error.code = %q, want %q", resp.Error.Code, "CONFLICT")
	}
	if resp.Error.Message != "username already exists" {
		t.Errorf("error.message = %q, want %q", resp.Error.Message, "username already exists")
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}
