package middleware

import (
	"context"
	"fmt"
	"log"
	"net/http"
	"strings"

	"github.com/google/uuid"
	"github.com/gorilla/mux"
	"fashionai/api-gateway/internal/service"
	"fashionai/api-gateway/pkg/auth"
	"fashionai/api-gateway/pkg/rbac"
)

// ─── Shared types and context key ─────────────────────────────────────────────

type contextKeyT string

const ClaimsKey contextKeyT = "claims"

// Claims holds JWT payload fields used throughout the gateway.
type Claims struct {
	Subject string `json:"sub"`
	OrgID   string `json:"org_id"`
	DeptID  string `json:"dept_id"`
	Role    string `json:"role"`
}

// GetClaims retrieves JWT claims from the request context.
func GetClaims(ctx context.Context) *Claims {
	if c, ok := ctx.Value(ClaimsKey).(*Claims); ok {
		return c
	}
	return nil
}

// GetUserID extracts the user UUID from context.
func GetUserID(ctx context.Context) uuid.UUID {
	if claims := GetClaims(ctx); claims != nil {
		if id, err := uuid.Parse(claims.Subject); err == nil {
			return id
		}
	}
	return uuid.Nil
}

// GetClientIP extracts the real client IP.
func GetClientIP(r *http.Request) string {
	if fwd := r.Header.Get("X-Forwarded-For"); fwd != "" {
		return strings.Split(fwd, ",")[0]
	}
	if fwd := r.Header.Get("X-Real-IP"); fwd != "" {
		return fwd
	}
	return r.RemoteAddr
}

// writeError writes a JSON error response.
func writeError(w http.ResponseWriter, status int, code, msg string) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	w.Write([]byte(`{"error":{"code":"` + code + `","message":"` + msg + `"}}`))
}

// responseWriter wraps http.ResponseWriter to capture status code.
type responseWriter struct {
	http.ResponseWriter
	status int
}

func (rw *responseWriter) WriteHeader(code int) {
	rw.status = code
	rw.ResponseWriter.WriteHeader(code)
}

// muxMiddleware matches gorilla/mux.MiddlewareFunc.
type muxMiddleware func(http.Handler) http.Handler

// ─── Audit entry (shared) ─────────────────────────────────────────────────────

// AuditEntry represents a structured audit log record.
type AuditEntry struct {
	Timestamp  string `json:"timestamp"`
	RequestID  string `json:"request_id"`
	UserID     string `json:"user_id"`
	OrgID      string `json:"org_id"`
	Method     string `json:"method"`
	Path       string `json:"path"`
	StatusCode int    `json:"status_code"`
	LatencyMs  int64  `json:"latency_ms"`
	ClientIP   string `json:"client_ip"`
}

// ─── Middlewares ──────────────────────────────────────────────────────────────

// JWTMiddleware extracts and validates JWT from Cookie (preferred) or Authorization header.
// Supports both browser cookie-based auth (XSS-resistant) and Bearer token API clients.
func JWTMiddleware(jwtSvc *auth.JWTService) mux.MiddlewareFunc {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var tokenString string

			// Track 1: read from httpOnly cookie (browser sessions)
			if cookie, err := r.Cookie("token"); err == nil && cookie.Value != "" {
				tokenString = cookie.Value
			}

			// Track 2: fall back to Authorization: Bearer header (API clients)
			if tokenString == "" {
				authHeader := r.Header.Get("Authorization")
				if authHeader != "" {
					parts := strings.SplitN(authHeader, " ", 2)
					if len(parts) == 2 && strings.ToLower(parts[0]) == "bearer" {
						tokenString = parts[1]
					}
				}
			}

			if tokenString == "" {
				writeError(w, http.StatusUnauthorized, "UNAUTHORIZED", "missing authentication token")
				return
			}

			claims, err := jwtSvc.Validate(tokenString)
			if err != nil {
				writeError(w, http.StatusUnauthorized, "UNAUTHORIZED", "invalid or expired token")
				return
			}

			ctxClaims := &Claims{
				Subject: claims.Subject,
				OrgID:   claims.OrgID,
				DeptID:  claims.DeptID,
				Role:    claims.Role,
			}
			ctx := context.WithValue(r.Context(), ClaimsKey, ctxClaims)
			next.ServeHTTP(w, r.WithContext(ctx))
		})
	}
}

// CSRFProtection middleware guards state-changing operations (POST/PUT/PATCH/DELETE)
// against cross-site request forgery. It passes requests that have:
//   - X-Requested-With: XMLHttpRequest (AJAX/React/Vue/Svelte clients), OR
//   - a SameSite=Lax (or Strict) cookie (modern browsers auto-set this for top-level navigations)
//
// Explicitly whitelisted: /v1/* (OpenAI-compatible API, uses Bearer tokens instead).
func CSRFProtection() mux.MiddlewareFunc {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			method := r.Method
			if method == http.MethodGet || method == http.MethodHead || method == http.MethodOptions || method == http.MethodTrace {
				next.ServeHTTP(w, r)
				return
			}
			// Skip /v1/* — those use Bearer token auth instead
			if strings.HasPrefix(r.URL.Path, "/v1/") {
				next.ServeHTTP(w, r)
				return
			}
			// Require X-Requested-With header
			if r.Header.Get("X-Requested-With") != "XMLHttpRequest" {
				writeError(w, http.StatusForbidden, "FORBIDDEN", "missing X-Requested-With header")
				return
			}
			next.ServeHTTP(w, r)
		})
	}
}

// RBACMiddleware checks route-level permissions using Casbin.
func RBACMiddleware(rbacSvc *rbac.RBAC) mux.MiddlewareFunc {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			claims := GetClaims(r.Context())
			if claims == nil {
				writeError(w, http.StatusUnauthorized, "UNAUTHORIZED", "no claims")
				return
			}

			route := mux.CurrentRoute(r)
			path, _ := route.GetPathTemplate()
			method := r.Method

			if err := rbacSvc.Check(claims.Role, path, method); err != nil {
				writeError(w, http.StatusForbidden, "FORBIDDEN", "insufficient permissions")
				return
			}
			next.ServeHTTP(w, r)
		})
	}
}

// AuditLogger returns a middleware that logs every request to DB via AuditService.
func AuditLogger(auditSvc *service.AuditService) mux.MiddlewareFunc {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			reqID := r.Header.Get("X-Request-ID")
			if reqID == "" {
				reqID = uuid.New().String()
			}
			w.Header().Set("X-Request-ID", reqID)

			rw := &responseWriter{ResponseWriter: w, status: http.StatusOK}
			next.ServeHTTP(rw, r)

			claims := GetClaims(r.Context())
			if claims == nil {
				return
			}
			userUUID, _ := uuid.Parse(claims.Subject)
			orgUUID, _ := uuid.Parse(claims.OrgID)
			clientIP := GetClientIP(r)

			// Write audit record to DB asynchronously to avoid blocking response
			go auditSvc.Log(r.Context(), orgUUID, userUUID,
				fmt.Sprintf("%s %s", r.Method, r.URL.Path),
				"http_request", nil, clientIP)
		})
	}
}

// RequestIDMiddleware adds a unique request ID to every response.
func RequestIDMiddleware() muxMiddleware {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			reqID := r.Header.Get("X-Request-ID")
			if reqID == "" {
				reqID = uuid.New().String()
			}
			w.Header().Set("X-Request-ID", reqID)
			next.ServeHTTP(w, r)
		})
	}
}

// RecoveryMiddleware recovers from panics.
func RecoveryMiddleware() muxMiddleware {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			defer func() {
				if rec := recover(); rec != nil {
					writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", "internal server error")
					log.Printf("[PANIC] %v", rec)
				}
			}()
			next.ServeHTTP(w, r)
		})
	}
}
