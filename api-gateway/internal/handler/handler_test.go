package handler

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"fashionai/api-gateway/internal/model"
)

func TestAuthHandler_Register_Validation(t *testing.T) {
	tests := []struct {
		name       string
		body       string
		wantStatus int
	}{
		{
			name:       "empty body",
			body:       `{}`,
			wantStatus: http.StatusBadRequest,
		},
		{
			name:       "missing password",
			body:       `{"username":"test","org_name":"acme","dept_name":"design"}`,
			wantStatus: http.StatusBadRequest,
		},
		{
			name:       "password too short",
			body:       `{"username":"test","password":"123","org_name":"acme","dept_name":"design"}`,
			wantStatus: http.StatusBadRequest,
		},
		{
			name:       "missing org_name",
			body:       `{"username":"test","password":"123456","dept_name":"design"}`,
			wantStatus: http.StatusBadRequest,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Handler with nil service (validation happens before service call)
			h := &AuthHandler{svc: nil}
			req := httptest.NewRequest(http.MethodPost, "/auth/register", bytes.NewBufferString(tt.body))
			req.Header.Set("Content-Type", "application/json")
			rr := httptest.NewRecorder()
			h.Register(rr, req)

			if rr.Code != tt.wantStatus {
				t.Errorf("got status %d, want %d", rr.Code, tt.wantStatus)
			}
		})
	}
}

func TestAuthHandler_Login_Validation(t *testing.T) {
	tests := []struct {
		name       string
		body       string
		wantStatus int
	}{
		{
			name:       "empty body",
			body:       `{}`,
			wantStatus: http.StatusBadRequest,
		},
		{
			name:       "missing password",
			body:       `{"username":"test"}`,
			wantStatus: http.StatusBadRequest,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			h := &AuthHandler{svc: nil}
			req := httptest.NewRequest(http.MethodPost, "/auth/login", bytes.NewBufferString(tt.body))
			req.Header.Set("Content-Type", "application/json")
			rr := httptest.NewRecorder()
			h.Login(rr, req)

			if rr.Code != tt.wantStatus {
				t.Errorf("got status %d, want %d", rr.Code, tt.wantStatus)
			}
		})
	}
}

func TestWriteError(t *testing.T) {
	rr := httptest.NewRecorder()
	writeError(rr, http.StatusUnauthorized, "UNAUTHORIZED", "token expired")

	if rr.Code != http.StatusUnauthorized {
		t.Errorf("status = %d, want %d", rr.Code, http.StatusUnauthorized)
	}

	var resp model.ErrorResponse
	if err := json.NewDecoder(rr.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode error response: %v", err)
	}
	if resp.Error.Code != "UNAUTHORIZED" {
		t.Errorf("error.code = %q, want %q", resp.Error.Code, "UNAUTHORIZED")
	}
	if resp.Error.Message != "token expired" {
		t.Errorf("error.message = %q, want %q", resp.Error.Message, "token expired")
	}
}

func TestWriteJSON(t *testing.T) {
	rr := httptest.NewRecorder()
	resp := model.AuthResponse{
		UserID:   "user-123",
		Username: "alice",
		Role:     "designer",
		Token:    "jwt-token-xyz",
	}
	writeJSON(rr, http.StatusOK, resp)

	if rr.Code != http.StatusOK {
		t.Errorf("status = %d, want %d", rr.Code, http.StatusOK)
	}
	if ct := rr.Header().Get("Content-Type"); ct != "application/json" {
		t.Errorf("Content-Type = %q, want %q", ct, "application/json")
	}
}

func TestParseInt(t *testing.T) {
	if got := parseInt("42", 10); got != 42 {
		t.Errorf("parseInt(\"42\", 10) = %d, want 42", got)
	}
	if got := parseInt("invalid", 99); got != 99 {
		t.Errorf("parseInt(\"invalid\", 99) = %d, want 99", got)
	}
	if got := parseInt("", 5); got != 5 {
		t.Errorf("parseInt(\"\", 5) = %d, want 5", got)
	}
}
