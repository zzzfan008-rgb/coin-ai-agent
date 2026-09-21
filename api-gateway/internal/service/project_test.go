package service

import (
	"context"
	"regexp"
	"testing"
	"time"

	"github.com/DATA-DOG/go-sqlmock"
	"github.com/google/uuid"
	"github.com/jmoiron/sqlx"

	"fashionai/api-gateway/internal/model"
)

func newMockDB(t *testing.T) (*sqlx.DB, sqlmock.Sqlmock) {
	t.Helper()
	db, mock, err := sqlmock.New(sqlmock.QueryMatcherOption(sqlmock.QueryMatcherRegexp))
	if err != nil {
		t.Fatalf("sqlmock.New: %v", err)
	}
	t.Cleanup(func() { db.Close() })
	return sqlx.NewDb(db, "sqlmock"), mock
}

var (
	testOrgID   = uuid.MustParse("00000000-0000-0000-0000-000000000001")
	testDeptID  = uuid.MustParse("00000000-0000-0000-0000-000000000010")
	testOwnerID = uuid.MustParse("00000000-0000-0000-0000-000000000020")
	testProjID  = uuid.MustParse("00000000-0000-0000-0000-000000000030")
	testSessID  = uuid.MustParse("00000000-0000-0000-0000-000000000040")
)

func projectRow(id uuid.UUID, name string, archived bool, now time.Time) *sqlmock.Rows {
	return sqlmock.NewRows([]string{
		"id", "org_id", "dept_id", "owner_id", "name", "description",
		"cover_color", "is_archived", "created_at", "updated_at",
	}).AddRow(id, testOrgID, testDeptID, testOwnerID, name, nil,
		"#6366F1", archived, now, now)
}

func TestProjectService_Create(t *testing.T) {
	db, mock := newMockDB(t)
	svc := NewProjectService(db)
	now := time.Now()

	desc := "夏装系列"
	mock.ExpectQuery(regexp.QuoteMeta("INSERT INTO projects")).
		WithArgs(testOrgID, testDeptID, testOwnerID, "夏季项目", &desc, "#10B981").
		WillReturnRows(projectRow(testProjID, "夏季项目", false, now))

	proj, err := svc.Create(context.Background(), testOrgID, testDeptID, testOwnerID,
		model.CreateProjectRequest{Name: "夏季项目", Description: &desc, CoverColor: "#10B981"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	if proj.ID != testProjID.String() || proj.Name != "夏季项目" || proj.CoverColor != "#6366F1" {
		t.Errorf("unexpected project: %+v", proj)
	}
	if proj.IsArchived {
		t.Errorf("new project should not be archived")
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}

func TestProjectService_Create_DefaultColor(t *testing.T) {
	db, mock := newMockDB(t)
	svc := NewProjectService(db)
	now := time.Now()

	mock.ExpectQuery(regexp.QuoteMeta("INSERT INTO projects")).
		WithArgs(testOrgID, testDeptID, testOwnerID, "P", nil, "#6366F1").
		WillReturnRows(projectRow(testProjID, "P", false, now))

	if _, err := svc.Create(context.Background(), testOrgID, testDeptID, testOwnerID,
		model.CreateProjectRequest{Name: "P"}); err != nil {
		t.Fatalf("Create: %v", err)
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}

func TestProjectService_List_DeptIsolation(t *testing.T) {
	db, mock := newMockDB(t)
	svc := NewProjectService(db)
	now := time.Now()

	otherID := uuid.MustParse("00000000-0000-0000-0000-000000000099")
	mock.ExpectQuery("SELECT id, org_id, dept_id").
		WithArgs(testDeptID, false).
		WillReturnRows(
			projectRow(testProjID, "活跃项目", false, now).
				AddRow(otherID, testOrgID, testDeptID, testOwnerID, "第二个", nil,
					"#F59E0B", false, now, now))

	list, err := svc.List(context.Background(), testOrgID, testDeptID, false)
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if list.Total != 2 || len(list.Projects) != 2 {
		t.Fatalf("want 2 projects, got %d", list.Total)
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}

func TestProjectService_List_ArchivedFilter(t *testing.T) {
	db, mock := newMockDB(t)
	svc := NewProjectService(db)
	now := time.Now()

	mock.ExpectQuery("SELECT id, org_id, dept_id").
		WithArgs(testDeptID, true).
		WillReturnRows(projectRow(testProjID, "归档项目", true, now))

	list, err := svc.List(context.Background(), testOrgID, testDeptID, true)
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if list.Total != 1 || !list.Projects[0].IsArchived {
		t.Errorf("want 1 archived project, got %+v", list)
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}

func TestProjectService_Get_WithSessions(t *testing.T) {
	db, mock := newMockDB(t)
	svc := NewProjectService(db)
	now := time.Now()

	mock.ExpectQuery("SELECT id, org_id, dept_id, owner_id, name, description, cover_color").
		WithArgs(testProjID, testDeptID).
		WillReturnRows(projectRow(testProjID, "项目", false, now))

	sessRows := sqlmock.NewRows([]string{
		"id", "org_id", "user_id", "title", "is_archived",
		"message_count", "created_at", "updated_at",
	}).AddRow(testSessID, testOrgID, testOwnerID, "会话1", false, 3, now, now)

	mock.ExpectQuery("SELECT s.id, s.org_id, s.user_id").
		WithArgs(testProjID).
		WillReturnRows(sessRows)

	detail, err := svc.Get(context.Background(), testDeptID, testProjID)
	if err != nil {
		t.Fatalf("Get: %v", err)
	}
	if detail.Name != "项目" || len(detail.Sessions) != 1 {
		t.Fatalf("unexpected detail: %+v", detail)
	}
	if detail.Sessions[0].MessageCount != 3 || detail.Sessions[0].Title != "会话1" {
		t.Errorf("unexpected session: %+v", detail.Sessions[0])
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}

func TestProjectService_Get_NotFound(t *testing.T) {
	db, mock := newMockDB(t)
	svc := NewProjectService(db)

	mock.ExpectQuery("SELECT id, org_id, dept_id, owner_id, name, description, cover_color").
		WithArgs(testProjID, testDeptID).
		WillReturnRows(sqlmock.NewRows([]string{"id"}))

	if _, err := svc.Get(context.Background(), testDeptID, testProjID); err != ErrProjectNotFound {
		t.Fatalf("want ErrProjectNotFound, got %v", err)
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}

func TestProjectService_Archive(t *testing.T) {
	db, mock := newMockDB(t)
	svc := NewProjectService(db)
	now := time.Now()

	mock.ExpectQuery("UPDATE projects SET is_archived").
		WithArgs(true, testProjID, testDeptID).
		WillReturnRows(projectRow(testProjID, "项目", true, now))

	proj, err := svc.Archive(context.Background(), testDeptID, testProjID)
	if err != nil {
		t.Fatalf("Archive: %v", err)
	}
	if !proj.IsArchived {
		t.Errorf("project should be archived")
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}

func TestProjectService_Unarchive(t *testing.T) {
	db, mock := newMockDB(t)
	svc := NewProjectService(db)
	now := time.Now()

	mock.ExpectQuery("UPDATE projects SET is_archived").
		WithArgs(false, testProjID, testDeptID).
		WillReturnRows(projectRow(testProjID, "项目", false, now))

	proj, err := svc.Unarchive(context.Background(), testDeptID, testProjID)
	if err != nil {
		t.Fatalf("Unarchive: %v", err)
	}
	if proj.IsArchived {
		t.Errorf("project should not be archived")
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}

func TestProjectService_Delete(t *testing.T) {
	db, mock := newMockDB(t)
	svc := NewProjectService(db)

	mock.ExpectExec("UPDATE projects SET is_archived=true").
		WithArgs(testProjID, testDeptID).
		WillReturnResult(sqlmock.NewResult(0, 1))

	if err := svc.Delete(context.Background(), testDeptID, testProjID); err != nil {
		t.Fatalf("Delete: %v", err)
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}

func TestProjectService_Delete_NotFound(t *testing.T) {
	db, mock := newMockDB(t)
	svc := NewProjectService(db)

	mock.ExpectExec("UPDATE projects SET is_archived=true").
		WithArgs(testProjID, testDeptID).
		WillReturnResult(sqlmock.NewResult(0, 0))

	if err := svc.Delete(context.Background(), testDeptID, testProjID); err != ErrProjectNotFound {
		t.Fatalf("want ErrProjectNotFound, got %v", err)
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}

func TestProjectService_AddSession(t *testing.T) {
	db, mock := newMockDB(t)
	svc := NewProjectService(db)

	mock.ExpectExec("INSERT INTO session_project").
		WithArgs(testSessID, testProjID).
		WillReturnResult(sqlmock.NewResult(0, 1))

	if err := svc.AddSession(context.Background(), testProjID, testSessID); err != nil {
		t.Fatalf("AddSession: %v", err)
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}

func TestProjectService_RemoveSession(t *testing.T) {
	db, mock := newMockDB(t)
	svc := NewProjectService(db)

	mock.ExpectExec("DELETE FROM session_project").
		WithArgs(testSessID, testProjID).
		WillReturnResult(sqlmock.NewResult(0, 1))

	if err := svc.RemoveSession(context.Background(), testProjID, testSessID); err != nil {
		t.Fatalf("RemoveSession: %v", err)
	}
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Error(err)
	}
}
