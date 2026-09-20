package auth

import (
	"testing"
	"time"

	"github.com/golang-jwt/jwt/v5"
)

func TestJWTService_Generate(t *testing.T) {
	svc := NewJWTService("test-secret-key-at-least-32-chars!!", 72)

	token, err := svc.Generate("user-uuid-123", "org-uuid-456", "dept-uuid-789", "designer")
	if err != nil {
		t.Fatalf("Generate() error = %v", err)
	}
	if token == "" {
		t.Fatal("Generate() returned empty token")
	}
}

func TestJWTService_Validate(t *testing.T) {
	svc := NewJWTService("test-secret-key-at-least-32-chars!!", 72)

	token, _ := svc.Generate("user-uuid-123", "org-uuid-456", "dept-uuid-789", "designer")

	claims, err := svc.Validate(token)
	if err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
	if claims.Subject != "user-uuid-123" {
		t.Errorf("Subject = %q, want %q", claims.Subject, "user-uuid-123")
	}
	if claims.OrgID != "org-uuid-456" {
		t.Errorf("OrgID = %q, want %q", claims.OrgID, "org-uuid-456")
	}
	if claims.DeptID != "dept-uuid-789" {
		t.Errorf("DeptID = %q, want %q", claims.DeptID, "dept-uuid-789")
	}
	if claims.Role != "designer" {
		t.Errorf("Role = %q, want %q", claims.Role, "designer")
	}
}

func TestJWTService_Validate_Invalid(t *testing.T) {
	svc := NewJWTService("test-secret-key-at-least-32-chars!!", 72)

	_, err := svc.Validate("not-a-valid-token")
	if err == nil {
		t.Error("Validate() expected error for invalid token, got nil")
	}
}

func TestJWTService_Validate_WrongSecret(t *testing.T) {
	svc1 := NewJWTService("secret-one-at-least-32-characters!!", 72)
	svc2 := NewJWTService("secret-two-at-least-32-characters!!", 72)

	token, _ := svc1.Generate("user-1", "org-1", "dept-1", "admin")

	_, err := svc2.Validate(token)
	if err == nil {
		t.Error("Validate() expected error for wrong secret, got nil")
	}
}

func TestJWTService_Validate_Expired(t *testing.T) {
	// Create service with 0 hour expiry
	svc := NewJWTService("test-secret-key-at-least-32-chars!!", 0)

	// Manually create an expired token
	claims := &Claims{
		RegisteredClaims: jwt.RegisteredClaims{
			Subject:   "user-123",
			ExpiresAt: jwt.NewNumericDate(time.Now().Add(-1 * time.Hour)),
			IssuedAt:  jwt.NewNumericDate(time.Now().Add(-2 * time.Hour)),
		},
		OrgID:  "org-1",
		DeptID: "dept-1",
		Role:   "designer",
	}
	token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
	tokenStr, _ := token.SignedString([]byte("test-secret-key-at-least-32-chars!!"))

	_, err := svc.Validate(tokenStr)
	if err == nil {
		t.Error("Validate() expected error for expired token, got nil")
	}
}

func TestHashPassword(t *testing.T) {
	hash, err := HashPassword("testpassword123")
	if err != nil {
		t.Fatalf("HashPassword() error = %v", err)
	}
	if hash == "" {
		t.Fatal("HashPassword() returned empty hash")
	}
	if hash == "testpassword123" {
		t.Error("HashPassword() hash should not equal plain password")
	}
}

func TestCheckPassword(t *testing.T) {
	hash, _ := HashPassword("mypassword")

	if !CheckPassword("mypassword", hash) {
		t.Error("CheckPassword() should return true for correct password")
	}
	if CheckPassword("wrongpassword", hash) {
		t.Error("CheckPassword() should return false for wrong password")
	}
}
