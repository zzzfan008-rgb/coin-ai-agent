package rbac

import (
	"testing"
)

func TestRBAC_Check_Admin(t *testing.T) {
	r, err := New()
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}

	tests := []struct {
		role   string
		path   string
		method string
		want   bool
	}{
		{"admin", "/auth/login", "POST", true},
		{"admin", "/v1/chat/completions", "POST", true},
		{"admin", "/api/users", "GET", true},
		{"admin", "/api/sessions", "POST", true},
	}

	for _, tt := range tests {
		err := r.Check(tt.role, tt.path, tt.method)
		if (err == nil) != tt.want {
			t.Errorf("Check(%q,%q,%q) = %v, want nil=%v", tt.role, tt.path, tt.method, err, tt.want)
		}
	}
}

func TestRBAC_Check_Designer(t *testing.T) {
	r, err := New()
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}

	// Designer can chat
	if err := r.Check("designer", "/v1/chat/completions", "POST"); err != nil {
		t.Errorf("designer should access /v1/chat/completions POST: %v", err)
	}

	// Designer cannot access /api/users (admin only)
	if err := r.Check("designer", "/api/users", "GET"); err == nil {
		t.Error("designer should NOT access /api/users")
	}
}

func TestRBAC_Check_Viewer(t *testing.T) {
	r, err := New()
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}

	// Viewer can read sessions
	if err := r.Check("viewer", "/api/sessions", "GET"); err != nil {
		t.Errorf("viewer should access /api/sessions GET: %v", err)
	}

	// Viewer cannot create sessions
	if err := r.Check("viewer", "/api/sessions", "POST"); err == nil {
		t.Error("viewer should NOT create sessions")
	}
}

func TestRBAC_Check_Unknown(t *testing.T) {
	r, err := New()
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}

	if err := r.Check("unknown_role", "/v1/models", "GET"); err == nil {
		t.Error("unknown role should be denied")
	}
}

func TestRBAC_AddPolicy(t *testing.T) {
	r, err := New()
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}

	// Initially denied
	if err := r.Check("viewer", "/api/roles", "POST"); err == nil {
		t.Error("viewer should not initially have POST /api/roles")
	}

	// Add policy
	if err := r.AddPolicy("viewer", "/api/roles", "POST"); err != nil {
		t.Fatalf("AddPolicy() error = %v", err)
	}

	// Now allowed
	if err := r.Check("viewer", "/api/roles", "POST"); err != nil {
		t.Errorf("viewer should now have POST /api/roles: %v", err)
	}
}
