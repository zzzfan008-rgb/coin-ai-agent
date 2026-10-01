package rbac

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"regexp"

	"github.com/casbin/casbin/v2"
)

var ErrForbidden = errors.New("access denied: insufficient role")

type RBAC struct {
	enforcer *casbin.Enforcer
	policies []policy
}

type policy struct {
	role   string
	path   string
	method string
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
		{"designer", "/v1/chat/upload-image", "POST"},
		{"designer", "/v1/models", "GET"},
		{"designer", "/ws/chat", "GET"},
		{"designer", "/api/sessions", "GET"},
		{"designer", "/api/sessions", "POST"},
		{"designer", "/api/sessions/{id}", "GET"},
		{"designer", "/api/sessions/{id}", "PATCH"},
		{"designer", "/api/sessions/{id}/messages", "GET"},
		// Designer: projects (T-018)
		{"designer", "/api/projects", "GET"},
		{"designer", "/api/projects", "POST"},
		{"designer", "/api/projects/*", "*"},
		{"designer", "/api/sessions/{id}/projects", "GET"},
		// Designer: MCP servers list + own enable/disable (T-016)
		{"designer", "/api/mcp/servers", "GET"},
		{"designer", "/api/mcp/servers/*/toggle", "POST"},
		// Viewer: read-only projects + MCP list
		{"viewer", "/api/projects", "GET"},
		{"viewer", "/api/projects/*", "GET"},
		{"viewer", "/api/sessions/{id}/projects", "GET"},
		{"viewer", "/api/mcp/servers", "GET"},
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

	// Save for debug printing
	var saved []policy
	for _, p := range policies {
		saved = append(saved, policy{role: p[0], path: p[1], method: p[2]})
	}
	return &RBAC{enforcer: e, policies: saved}, nil
}

// patternToRegex converts a Casbin-style path pattern to a regex.
// "*" becomes ".*" and "{id}" becomes "[^/]+".
func patternToRegex(pattern string) string {
	re := "^"
	inBrace := false
	for _, ch := range pattern {
		if ch == '}' && inBrace {
			inBrace = false
		} else if ch == '{' {
			re += "[^/]+"
			inBrace = true
		} else if inBrace {
			// skip brace contents (e.g. "id" in "{id}")
		} else if ch == '*' {
			re += ".*"
		} else {
			re += string(ch)
		}
	}
	re += "$"
	return re
}

// Check verifies whether the role can access the given path+method.
func (r *RBAC) Check(role, path, method string) error {
	allowed, err := r.enforcer.Enforce(role, path, method)
	if err != nil {
		return err
	}
	if !allowed {
		// Denied: log the first few patterns for this role to aid debugging.
		shown := 0
		for _, p := range r.policies {
			if p.role == role && shown < 5 {
				re := patternToRegex(p.path)
				m, _ := regexp.MatchString(re, path)
				fmt.Printf("[RBAC] deny role=%s path=%s method=%s → pattern=%s match=%v\n",
					role, path, method, re, m)
				shown++
			}
		}
		return ErrForbidden
	}
	return nil
}

// AddPolicy adds a custom policy at runtime.
func (r *RBAC) AddPolicy(role, path, method string) error {
	_, err := r.enforcer.AddPolicy(role, path, method)
	return err
}
