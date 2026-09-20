package rbac

import (
	"errors"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/casbin/casbin/v2"
)

var ErrForbidden = errors.New("access denied: insufficient role")

type RBAC struct {
	enforcer *casbin.Enforcer
}

func New() (*RBAC, error) {
	// Write a temporary model.conf that enables our custom pathMatch function.
	dir, err := os.MkdirTemp("", "casbin-*")
	if err != nil {
		return nil, err
	}
	modelPath := filepath.Join(dir, "model.conf")
	modelConf := `[request_definition]
r = role, path, method

[policy_definition]
p = role, path, method

[policy_effect]
e = some(where (p.eft == allow))

[matchers]
m = r.role == p.role && pathMatch(r.path, p.path) && (p.method == "*" || r.method == p.method)
`
	if err := os.WriteFile(modelPath, []byte(modelConf), 0600); err != nil {
		return nil, err
	}

	e, err := casbin.NewEnforcer(modelPath)
	if err != nil {
		return nil, err
	}

	// Defer cleanup; the file stays valid for the lifetime of the process.
	defer os.RemoveAll(dir)

	// Register custom path-matching function:
	// converts policy patterns like "/auth/*" → regex "/auth/.*"
	// and "/api/{id}" → regex "/api/[^/]+"
	e.AddFunction("pathMatch", func(args ...interface{}) (interface{}, error) {
		if len(args) != 2 {
			return false, nil
		}
		path, ok1 := args[0].(string)
		pattern, ok2 := args[1].(string)
		if !ok1 || !ok2 {
			return false, nil
		}
		re := patternToRegex(pattern)
		m, err := regexp.Compile(re)
		if err != nil {
			return false, nil
		}
		return m.MatchString(path), nil
	})

	policies := [][3]string{
		// Admin: full access
		{"admin", "/auth/*", "*"},
		{"admin", "/v1/*", "*"},
		{"admin", "/api/*", "*"},
		// Designer: chat + session CRUD
		{"designer", "/v1/chat/completions", "POST"},
		{"designer", "/v1/models", "GET"},
		{"designer", "/ws/chat", "GET"},
		{"designer", "/api/sessions", "GET"},
		{"designer", "/api/sessions", "POST"},
		{"designer", "/api/sessions/{id}", "GET"},
		{"designer", "/api/sessions/{id}", "PATCH"},
		{"designer", "/api/sessions/{id}/messages", "GET"},
		// Viewer: read-only
		{"viewer", "/v1/chat/completions", "POST"},
		{"viewer", "/v1/models", "GET"},
		{"viewer", "/ws/chat", "GET"},
		{"viewer", "/api/sessions", "GET"},
		{"viewer", "/api/sessions/{id}/messages", "GET"},
	}
	for _, p := range policies {
		e.AddPolicy(p[0], p[1], p[2])
	}

	return &RBAC{enforcer: e}, nil
}

// patternToRegex converts a Casbin-style path pattern to a regex.
// "*" becomes ".*" and "{id}" becomes "[^/]+".
func patternToRegex(pattern string) string {
	// Escape special regex chars except our wildcards
	re := "^"
	for _, ch := range pattern {
		switch ch {
		case '*':
			re += ".*"
		case '{', '}':
			// skip
		default:
			re += string(ch)
		}
	}
	re += "$"
	// {id} segments are replaced by [^/]+
	re = strings.ReplaceAll(re, "[^/]+", "[^/]+")
	// Normalize remaining asterisks
	re = strings.ReplaceAll(re, ".*", ".*")
	return re
}

// Check verifies whether the role can access the given path+method.
func (r *RBAC) Check(role, path, method string) error {
	allowed, err := r.enforcer.Enforce(role, path, method)
	if err != nil {
		return err
	}
	if !allowed {
		return ErrForbidden
	}
	return nil
}

// AddPolicy adds a custom policy at runtime.
func (r *RBAC) AddPolicy(role, path, method string) error {
	_, err := r.enforcer.AddPolicy(role, path, method)
	return err
}
