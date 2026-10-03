package middleware

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/gorilla/mux"

	"fashionai/api-gateway/pkg/auth"
)

const testJWTSecret = "test-secret-key-at-least-32-chars!!"

// newProtectedRouter wraps a probe handler with the production
// JWTMiddleware and records whether it was reached plus the role seen
// in the request context.
func newProtectedRouter(svc *auth.JWTService, hit *bool, roleOut *string) *mux.Router {
	r := mux.NewRouter()
	r.Use(JWTMiddleware(svc))
	r.HandleFunc("/protected", func(w http.ResponseWriter, req *http.Request) {
		*hit = true
		if c, ok := req.Context().Value(ClaimsKey).(*Claims); ok && c != nil {
			*roleOut = c.Role
		}
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"ok":true}`))
	}).Methods(http.MethodGet)
	return r
}

// D1 promise: a request without any token must be rejected with 401
// before the protected handler runs.
func TestJWTMiddleware_MissingToken_Returns401(t *testing.T) {
	svc := auth.NewJWTService(testJWTSecret, 72)
	hit := false
	role := ""
	r := newProtectedRouter(svc, &hit, &role)

	req := httptest.NewRequest(http.MethodGet, "/protected", nil)
	rr := httptest.NewRecorder()
	r.ServeHTTP(rr, req)

	if rr.Code != http.StatusUnauthorized {
		t.Fatalf("status = %d, want 401; body = %s", rr.Code, rr.Body.String())
	}
	if !strings.Contains(rr.Body.String(), "missing authentication token") {
		t.Errorf("body = %q, want missing authentication token", rr.Body.String())
	}
	if hit {
		t.Error("protected handler ran despite missing token")
	}
}

// A malformed Bearer token must be rejected with 401, not forwarded.
func TestJWTMiddleware_InvalidBearerToken_Returns401(t *testing.T) {
	svc := auth.NewJWTService(testJWTSecret, 72)
	hit := false
	role := ""
	r := newProtectedRouter(svc, &hit, &role)

	req := httptest.NewRequest(http.MethodGet, "/protected", nil)
	req.Header.Set("Authorization", "Bearer not-a-valid-token")
	rr := httptest.NewRecorder()
	r.ServeHTTP(rr, req)

	if rr.Code != http.StatusUnauthorized {
		t.Fatalf("status = %d, want 401; body = %s", rr.Code, rr.Body.String())
	}
	if !strings.Contains(rr.Body.String(), "invalid or expired token") {
		t.Errorf("body = %q, want invalid or expired token", rr.Body.String())
	}
	if hit {
		t.Error("protected handler ran despite invalid token")
	}
}

// A token signed with a different secret must also be rejected (401).
func TestJWTMiddleware_WrongSecret_Returns401(t *testing.T) {
	otherSvc := auth.NewJWTService("another-secret-key-also-32-chars!", 72)
	token, err := otherSvc.Generate("user-1", "org-1", "dept-1", "designer")
	if err != nil {
		t.Fatalf("Generate() error = %v", err)
	}

	svc := auth.NewJWTService(testJWTSecret, 72)
	hit := false
	role := ""
	r := newProtectedRouter(svc, &hit, &role)

	req := httptest.NewRequest(http.MethodGet, "/protected", nil)
	req.Header.Set("Authorization", "Bearer "+token)
	rr := httptest.NewRecorder()
	r.ServeHTTP(rr, req)

	if rr.Code != http.StatusUnauthorized {
		t.Fatalf("status = %d, want 401; body = %s", rr.Code, rr.Body.String())
	}
	if hit {
		t.Error("protected handler ran despite wrong-secret token")
	}
}

// A valid Bearer token must reach the handler with claims in context.
func TestJWTMiddleware_ValidBearerToken_PassesWithClaims(t *testing.T) {
	svc := auth.NewJWTService(testJWTSecret, 72)
	token, err := svc.Generate("user-uuid-123", "org-uuid-456", "dept-uuid-789", "designer")
	if err != nil {
		t.Fatalf("Generate() error = %v", err)
	}

	hit := false
	role := ""
	r := newProtectedRouter(svc, &hit, &role)

	req := httptest.NewRequest(http.MethodGet, "/protected", nil)
	req.Header.Set("Authorization", "Bearer "+token)
	rr := httptest.NewRecorder()
	r.ServeHTTP(rr, req)

	if rr.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200; body = %s", rr.Code, rr.Body.String())
	}
	if !hit {
		t.Fatal("protected handler did not run for valid token")
	}
	if role != "designer" {
		t.Errorf("context role = %q, want designer", role)
	}
}

// Browser track: the httpOnly "jwt" cookie must authenticate too.
func TestJWTMiddleware_ValidCookie_PassesWithClaims(t *testing.T) {
	svc := auth.NewJWTService(testJWTSecret, 72)
	token, err := svc.Generate("user-uuid-123", "org-uuid-456", "dept-uuid-789", "viewer")
	if err != nil {
		t.Fatalf("Generate() error = %v", err)
	}

	hit := false
	role := ""
	r := newProtectedRouter(svc, &hit, &role)

	req := httptest.NewRequest(http.MethodGet, "/protected", nil)
	req.AddCookie(&http.Cookie{Name: "jwt", Value: token})
	rr := httptest.NewRecorder()
	r.ServeHTTP(rr, req)

	if rr.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200; body = %s", rr.Code, rr.Body.String())
	}
	if !hit {
		t.Fatal("protected handler did not run for valid cookie")
	}
	if role != "viewer" {
		t.Errorf("context role = %q, want viewer", role)
	}
}
