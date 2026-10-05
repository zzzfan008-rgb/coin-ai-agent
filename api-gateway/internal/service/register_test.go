package service

import (
	"context"
	"errors"
	"regexp"
	"testing"

	"github.com/DATA-DOG/go-sqlmock"

	"fashionai/api-gateway/internal/model"
	"fashionai/api-gateway/pkg/auth"
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
