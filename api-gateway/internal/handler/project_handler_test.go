package handler

import (
	"bytes"
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/gorilla/mux"

	"fashionai/api-gateway/internal/middleware"
)

func claimsCtx() context.Context {
	return context.WithValue(context.Background(), middleware.ClaimsKey, &middleware.Claims{
		Subject: "00000000-0000-0000-0000-000000000020",
		OrgID:   "00000000-0000-0000-0000-000000000001",
		DeptID:  "00000000-0000-0000-0000-000000000010",
		Role:    "designer",
	})
}

func TestProjectHandler_Create_Validation(t *testing.T) {
	h := &ProjectHandler{svc: nil}

	tests := []struct {
		name       string
		body       string
		wantStatus int
	}{
		{"invalid json", `{not json`, http.StatusBadRequest},
		{"empty name", `{"name":""}`, http.StatusBadRequest},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			req := httptest.NewRequest(http.MethodPost, "/api/projects", bytes.NewBufferString(tt.body))
			req = req.WithContext(claimsCtx())
			rr := httptest.NewRecorder()
			h.Create(rr, req)
			if rr.Code != tt.wantStatus {
				t.Errorf("got %d, want %d", rr.Code, tt.wantStatus)
			}
		})
	}
}

func TestProjectHandler_Get_InvalidID(t *testing.T) {
	h := &ProjectHandler{svc: nil}
	req := httptest.NewRequest(http.MethodGet, "/api/projects/not-a-uuid", nil)
	req = req.WithContext(claimsCtx())
	rr := httptest.NewRecorder()
	h.Get(rr, req)
	if rr.Code != http.StatusBadRequest {
		t.Errorf("got %d, want %d", rr.Code, http.StatusBadRequest)
	}
}

func TestProjectHandler_Update_InvalidID(t *testing.T) {
	h := &ProjectHandler{svc: nil}
	req := httptest.NewRequest(http.MethodPut, "/api/projects/bad", bytes.NewBufferString(`{}`))
	req = req.WithContext(claimsCtx())
	rr := httptest.NewRecorder()
	h.Update(rr, req)
	if rr.Code != http.StatusBadRequest {
		t.Errorf("got %d, want %d", rr.Code, http.StatusBadRequest)
	}
}

func TestProjectHandler_Delete_InvalidID(t *testing.T) {
	h := &ProjectHandler{svc: nil}
	req := httptest.NewRequest(http.MethodDelete, "/api/projects/bad", nil)
	req = req.WithContext(claimsCtx())
	rr := httptest.NewRecorder()
	h.Delete(rr, req)
	if rr.Code != http.StatusBadRequest {
		t.Errorf("got %d, want %d", rr.Code, http.StatusBadRequest)
	}
}

func TestProjectHandler_Archive_InvalidID(t *testing.T) {
	h := &ProjectHandler{svc: nil}
	req := httptest.NewRequest(http.MethodPost, "/api/projects/bad/archive", nil)
	req = req.WithContext(claimsCtx())
	rr := httptest.NewRecorder()
	h.Archive(rr, req)
	if rr.Code != http.StatusBadRequest {
		t.Errorf("got %d, want %d", rr.Code, http.StatusBadRequest)
	}
}

func TestProjectHandler_Unarchive_InvalidID(t *testing.T) {
	h := &ProjectHandler{svc: nil}
	req := httptest.NewRequest(http.MethodPost, "/api/projects/bad/unarchive", nil)
	req = req.WithContext(claimsCtx())
	rr := httptest.NewRecorder()
	h.Unarchive(rr, req)
	if rr.Code != http.StatusBadRequest {
		t.Errorf("got %d, want %d", rr.Code, http.StatusBadRequest)
	}
}

func TestProjectHandler_AddSession_Validation(t *testing.T) {
	h := &ProjectHandler{svc: nil}
	goodProjID := "00000000-0000-0000-0000-000000000030"

	tests := []struct {
		name       string
		vars       map[string]string
		body       string
		wantStatus int
	}{
		{"invalid project id", map[string]string{"id": "bad"}, `{bad`, http.StatusBadRequest},
		{"invalid json", map[string]string{"id": goodProjID}, `{bad`, http.StatusBadRequest},
		{"empty session_id", map[string]string{"id": goodProjID}, `{"session_id":""}`, http.StatusBadRequest},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			req := httptest.NewRequest(http.MethodPost, "/api/projects/x/sessions", bytes.NewBufferString(tt.body))
			req = mux.SetURLVars(req.WithContext(claimsCtx()), tt.vars)
			rr := httptest.NewRecorder()
			h.AddSession(rr, req)
			if rr.Code != tt.wantStatus {
				t.Errorf("got %d, want %d", rr.Code, tt.wantStatus)
			}
		})
	}
}

func TestProjectHandler_RemoveSession_InvalidIDs(t *testing.T) {
	h := &ProjectHandler{svc: nil}
	goodProjID := "00000000-0000-0000-0000-000000000030"

	tests := []struct {
		name string
		vars map[string]string
	}{
		{"bad project id", map[string]string{"id": "bad", "session_id": goodProjID}},
		{"bad session id", map[string]string{"id": goodProjID, "session_id": "also-bad"}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			req := httptest.NewRequest(http.MethodDelete, "/api/projects/x/sessions/y", nil)
			req = mux.SetURLVars(req.WithContext(claimsCtx()), tt.vars)
			rr := httptest.NewRecorder()
			h.RemoveSession(rr, req)
			if rr.Code != http.StatusBadRequest {
				t.Errorf("got %d, want %d", rr.Code, http.StatusBadRequest)
			}
		})
	}
}
