package service

import (
	"context"
	"errors"
	"regexp"
	"testing"

	"github.com/DATA-DOG/go-sqlmock"
	"github.com/google/uuid"
	"github.com/lib/pq"

	"fashionai/api-gateway/internal/model"
	"fashionai/api-gateway/pkg/auth"
)

var (
	testUserOrgID  = uuid.MustParse("00000000-0000-0000-0000-000000000001")
	testUserDeptID = uuid.MustParse("00000000-0000-0000-0000-000000000010")
	testUserID     = uuid.MustParse("00000000-0000-0000-0000-000000000020")
)

// TestAuthService_Register_DuplicateUsername 覆盖 T-020 的 409 前置：相同
// username 二次注册时，Register 必须返回 ErrUserExists（handler 据此映射 409），
// 且不执行任何 INSERT（事务整体回滚）。DB 用 sqlmock 模拟 EXISTS 返回 true，
// 不触碰真实 PostgreSQL。
func TestAuthService_Register_DuplicateUsername(t *testing.T) {
	db, mock := newMockDB(t)
	svc := NewAuthService(db, auth.NewJWTService("test-secret", 24), false)

	mock.ExpectBegin()
	mock.ExpectQuery(regexp.QuoteMeta("SELECT EXISTS(SELECT 1 FROM users WHERE username=$1)")).
		WithArgs("alice").
		WillReturnRows(sqlmock.NewRows([]string{"exists"}).AddRow(true))
	mock.ExpectRollback()

	resp, err := svc.Register(context.Background(), model.RegisterRequest{
		OrgName:  "acme",
		DeptName: "design",
		Username: "alice",
		Password: "password123",
	})
	if !errors.Is(err, ErrUserExists) {
		t.Fatalf("want ErrUserExists, got %v", err)
	}
	if resp != nil {
		t.Errorf("want nil response on duplicate username, got %+v", resp)
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}

// registerUniqueViolationFlow 推送 Register 完整路径的 sqlmock 期望：
// 应用层 EXISTS 查重返回 false（模拟并发窗口——两个同名注册都通过了查重），
// org/dept 创建成功，最终 users INSERT 返回指定错误。所有前置步骤的 SQL
// 都显式声明，依赖未执行即会由 ExpectationsWereMet 暴露。
func registerUniqueViolationFlow(mock sqlmock.Sqlmock, username string, insertErr error) {
	mock.ExpectBegin()
	mock.ExpectQuery(regexp.QuoteMeta("SELECT EXISTS(SELECT 1 FROM users WHERE username=$1)")).
		WithArgs(username).
		WillReturnRows(sqlmock.NewRows([]string{"exists"}).AddRow(false))

	mock.ExpectQuery(regexp.QuoteMeta("INSERT INTO orgs(name, slug) VALUES($1,$2) RETURNING id")).
		WithArgs("acme", sqlmock.AnyArg()).
		WillReturnRows(sqlmock.NewRows([]string{"id"}).AddRow(testUserOrgID))

	mock.ExpectQuery(regexp.QuoteMeta("INSERT INTO depts(org_id, name, path) VALUES($1,$2,$3) RETURNING id")).
		WithArgs(testUserOrgID, "design", "/design").
		WillReturnRows(sqlmock.NewRows([]string{"id"}).AddRow(testUserDeptID))

	mock.ExpectQuery(regexp.QuoteMeta("INSERT INTO users(org_id, dept_id, username, password_hash, display_name, email, role)")).
		WithArgs(testUserOrgID, testUserDeptID, username, sqlmock.AnyArg(), username, sqlmock.AnyArg()).
		WillReturnError(insertErr)
	mock.ExpectRollback()
}

// TestAuthService_Register_UniqueViolation_GlobalConstraint 覆盖并发边界收口：
// INSERT 触发全局唯一索引 uq_users_username_global 的 23505 冲突时，
// Register 必须返回 ErrUserExists（handler 自动 409），不能漏成 500。
func TestAuthService_Register_UniqueViolation_GlobalConstraint(t *testing.T) {
	db, mock := newMockDB(t)
	svc := NewAuthService(db, auth.NewJWTService("test-secret", 24), false)

	registerUniqueViolationFlow(mock, "alice", &pq.Error{Code: "23505", Constraint: "uq_users_username_global"})

	resp, err := svc.Register(context.Background(), model.RegisterRequest{
		OrgName:  "acme",
		DeptName: "design",
		Username: "alice",
		Password: "password123",
	})
	if !errors.Is(err, ErrUserExists) {
		t.Fatalf("want ErrUserExists, got %v", err)
	}
	if resp != nil {
		t.Errorf("want nil response on unique violation, got %+v", resp)
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}

// TestAuthService_Register_UniqueViolation_OrgScopedConstraint 覆盖 001 的组合
// 约束 uq_user_username_org 触发冲突时同样映射 ErrUserExists。
func TestAuthService_Register_UniqueViolation_OrgScopedConstraint(t *testing.T) {
	db, mock := newMockDB(t)
	svc := NewAuthService(db, auth.NewJWTService("test-secret", 24), false)

	registerUniqueViolationFlow(mock, "alice", &pq.Error{Code: "23505", Constraint: "uq_user_username_org"})

	_, err := svc.Register(context.Background(), model.RegisterRequest{
		OrgName:  "acme",
		DeptName: "design",
		Username: "alice",
		Password: "password123",
	})
	if !errors.Is(err, ErrUserExists) {
		t.Fatalf("want ErrUserExists, got %v", err)
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}

// TestAuthService_Register_UniqueViolation_UnrelatedConstraint 证明映射是
// 约束名过滤而非"任何 23505 都算用户名重复"：其它约束（如
// uq_organizations_slug）的冲突不得被吞成 ErrUserExists，必须原样上抛。
func TestAuthService_Register_UniqueViolation_UnrelatedConstraint(t *testing.T) {
	db, mock := newMockDB(t)
	svc := NewAuthService(db, auth.NewJWTService("test-secret", 24), false)

	want := &pq.Error{Code: "23505", Constraint: "uq_organizations_slug"}
	registerUniqueViolationFlow(mock, "alice", want)

	_, err := svc.Register(context.Background(), model.RegisterRequest{
		OrgName:  "acme",
		DeptName: "design",
		Username: "alice",
		Password: "password123",
	})
	if err == nil {
		t.Fatal("want non-nil error for unrelated constraint violation")
	}
	if errors.Is(err, ErrUserExists) {
		t.Fatalf("unrelated constraint must NOT map to ErrUserExists, got %v", err)
	}
	var gotPQ *pq.Error
	if !errors.As(err, &gotPQ) {
		t.Fatalf("want wrapped *pq.Error preserved, got %T: %v", err, err)
	}
	if gotPQ.Constraint != want.Constraint || gotPQ.Code != want.Code {
		t.Errorf("got constraint %q code %q, want %q %q", gotPQ.Constraint, gotPQ.Code, want.Constraint, want.Code)
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}

// TestIsUsernameUniqueViolation 白盒锁定映射规则的完整边界：
// 仅 23505 + 两个 username 约束名返回 true；空约束名、其它约束名、
// 其它 SQLSTATE、非 pq 错误一律 false。
func TestIsUsernameUniqueViolation(t *testing.T) {
	cases := []struct {
		name string
		err  error
		want bool
	}{
		{"nil error", nil, false},
		{"non-pq error", errors.New("boom"), false},
		{"other sqlstate", &pq.Error{Code: "23502", Constraint: "uq_users_username_global"}, false},
		{"missing constraint name", &pq.Error{Code: "23505"}, false},
		{"global index", &pq.Error{Code: "23505", Constraint: "uq_users_username_global"}, true},
		{"org-scoped constraint", &pq.Error{Code: "23505", Constraint: "uq_user_username_org"}, true},
		{"unrelated unique constraint", &pq.Error{Code: "23505", Constraint: "uq_organizations_slug"}, false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := isUsernameUniqueViolation(tc.err); got != tc.want {
				t.Errorf("isUsernameUniqueViolation(%v) = %v, want %v", tc.err, got, tc.want)
			}
		})
	}
}
