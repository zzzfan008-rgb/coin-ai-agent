package main

import (
	"context"
	"fmt"
	"log"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"syscall"
	"time"

	"github.com/gorilla/handlers"
	"github.com/gorilla/mux"
	"github.com/jmoiron/sqlx"
	"github.com/joho/godotenv"
	"github.com/redis/go-redis/v9"

	"fashionai/api-gateway/internal/config"
	"fashionai/api-gateway/internal/db"
	"fashionai/api-gateway/internal/handler"
	"fashionai/api-gateway/internal/middleware"
	"fashionai/api-gateway/internal/service"
	"fashionai/api-gateway/pkg/auth"
	"fashionai/api-gateway/pkg/rbac"
)

func main() {
	// godotenv 不会返回已设置的变量覆盖，依次尝试常见位置（从项目根或 api-gateway/ 启动都能命中）
	for _, p := range []string{".env", "api-gateway/.env", "../api-gateway/.env"} {
		if err := godotenv.Load(p); err == nil {
			log.Println("[ENV] loaded", p)
			break
		}
	}

	cfg := config.Load()

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// ── Database ────────────────────────────────────────────────────────────────
	var pool *sqlx.DB
	var dbErr error
	if cfg.DatabaseURL != "" {
		pool, dbErr = db.Open(db.Config{
			DatabaseURL:    cfg.DatabaseURL,
			MaxOpenConns:   25,
			MaxIdleConns:   5,
			ConnMaxLifetime: 5 * time.Minute,
		})
		if dbErr != nil {
			log.Printf("[WARN] database not reachable: %v — starting in degraded mode", dbErr)
		} else {
			log.Println("[OK] database connected")
			defer pool.Close()

			// 自动执行 migrations/（唯一权威 schema 路径）；
			// SKIP_MIGRATIONS=1 可关闭（如容器 entrypoint 已单独迁移）。
			if os.Getenv("SKIP_MIGRATIONS") != "1" {
				migRoot, err := db.ResolveMigrationsRoot()
				if err != nil {
					log.Printf("[ERROR] %v — set MIGRATIONS_DIR or fix working directory (continuing in degraded mode)", err)
				} else if err := db.Migrate(ctx, pool, db.Config{MigrationsRoot: migRoot}, slog.Default()); err != nil {
					log.Printf("[WARN] auto-migrate failed: %v (continuing in degraded mode)", err)
				}
			}
		}
	}

	// ── Redis ──────────────────────────────────────────────────────────────────
	rdb := redis.NewClient(&redis.Options{
		Addr: strings.TrimPrefix(cfg.RedisURL, "redis://"),
	})
	if _, err := rdb.Ping(ctx).Result(); err != nil {
		log.Printf("[WARN] redis not reachable: %v — rate limiting disabled", err)
	} else {
		log.Println("[OK] redis connected")
		defer rdb.Close()
	}

	// ── Services ───────────────────────────────────────────────────────────────
	jwtSvc := auth.NewJWTService(cfg.JWTSecret, cfg.JWTExpHours)
	rbacSvc, err := rbac.New()
	if err != nil {
		log.Fatalf("[FATAL] rbac init: %v", err)
	}

	var (
		authSvc     *service.AuthService
		userSvc      *service.UserService
		deptSvc      *service.DeptService
		roleSvc      *service.RoleService
		sessionSvc    *service.SessionService
		projectSvc    *service.ProjectService
		auditSvc     *service.AuditService
		mcpSvc       *service.McpService
		chatH        *handler.ChatHandler
	)
	if pool != nil {
		authSvc     = service.NewAuthService(pool, jwtSvc, cfg.CookieSecure)
		userSvc     = service.NewUserService(pool)
		deptSvc     = service.NewDeptService(pool)
		roleSvc     = service.NewRoleService(pool)
		sessionSvc  = service.NewSessionService(pool)
		projectSvc  = service.NewProjectService(pool)
		auditSvc    = service.NewAuditService(pool)
		mcpSvc      = service.NewMcpService(pool)
	}

	chatSvc := service.NewChatService(cfg.RustCoreURL)

	// ── Handlers ─────────────────────────────────────────────────────────────
	var authH    *handler.AuthHandler
	var userH    *handler.UserHandler
	var deptH    *handler.DeptHandler
	var roleH    *handler.RoleHandler
	var sessionH *handler.SessionHandler
	var projectH *handler.ProjectHandler
	var mcpH2    *handler.McpHandler
	var healthH  *handler.HealthHandler

	if pool != nil {
		authH    = handler.NewAuthHandler(authSvc, auditSvc)
		userH    = handler.NewUserHandler(userSvc)
		deptH    = handler.NewDeptHandler(deptSvc)
		roleH    = handler.NewRoleHandler(roleSvc)
		sessionH = handler.NewSessionHandler(sessionSvc, chatSvc)
		projectH = handler.NewProjectHandler(projectSvc)
		mcpH2    = handler.NewMcpHandler(mcpSvc)
	}
	chatH = handler.NewChatHandler(chatSvc, jwtSvc)
	healthH = handler.NewHealthHandler(chatSvc)

	// ── Router ───────────────────────────────────────────────────────────────
	r := mux.NewRouter()

	// Recovery + Request ID middleware (func(http.Handler) http.Handler)
	r.Use(func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
			defer func() {
				if rec := recover(); rec != nil {
					log.Printf("[PANIC] %v", rec)
					w.Header().Set("Content-Type", "application/json")
					w.WriteHeader(http.StatusInternalServerError)
					w.Write([]byte(`{"error":{"code":"INTERNAL_ERROR","message":"internal server error"}}`))
				}
			}()
			next.ServeHTTP(w, req)
		})
	})

	r.Use(func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
			reqID := req.Header.Get("X-Request-ID")
			if reqID == "" {
				reqID = fmt.Sprintf("%d", time.Now().UnixNano())
			}
			w.Header().Set("X-Request-ID", reqID)
			next.ServeHTTP(w, req)
		})
	})

	r.Use(handlers.CORS(
		handlers.AllowedOrigins([]string{"*"}),
		handlers.AllowedMethods([]string{"GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"}),
		handlers.AllowedHeaders([]string{"Authorization", "Content-Type", "X-Request-ID", "X-Requested-With"}),
	))

	// Global middleware chain for authenticated routes
	jwtMw := middleware.JWTMiddleware(jwtSvc)
	rbacMw := middleware.RBACMiddleware(rbacSvc)
	rlMw := middleware.NewRateLimiter(rdb, 60, time.Minute).Middleware()

	// Public routes
	r.HandleFunc("/health", healthH.Health).Methods(http.MethodGet)

	if authH != nil {
		authRouter := r.PathPrefix("/auth").Subrouter()
		authRouter.HandleFunc("/register", authH.Register).Methods(http.MethodPost)
		authRouter.HandleFunc("/login", authH.Login).Methods(http.MethodPost)
	}

	// Authenticated API routes
	if userH != nil && deptH != nil && roleH != nil && sessionH != nil && projectH != nil {
		api := r.PathPrefix("/api").Subrouter()
		api.Use(jwtMw)
		api.Use(rlMw)
		api.Use(rbacMw)
		api.Use(middleware.CSRFProtection()) // CSRF guard for POST/PUT/PATCH/DELETE (skips /v1/*)
		if auditSvc != nil {
			api.Use(middleware.AuditLogger(auditSvc))
		}

		// Auth logout (requires JWT via cookie or Bearer; CSRF-protected POST)
		api.HandleFunc("/auth/logout", authH.Logout).Methods(http.MethodPost)

		api.HandleFunc("/users", userH.List).Methods(http.MethodGet)
		api.HandleFunc("/depts", deptH.List).Methods(http.MethodGet)
		api.HandleFunc("/depts", deptH.Create).Methods(http.MethodPost)
		api.HandleFunc("/roles", roleH.List).Methods(http.MethodGet)
		api.HandleFunc("/roles", roleH.Create).Methods(http.MethodPost)
		api.HandleFunc("/sessions", sessionH.List).Methods(http.MethodGet)
		api.HandleFunc("/sessions", sessionH.Create).Methods(http.MethodPost)
		api.HandleFunc("/sessions/{id}", sessionH.Get).Methods(http.MethodGet)
		api.HandleFunc("/sessions/{id}", sessionH.Update).Methods(http.MethodPatch)
		api.HandleFunc("/sessions/{id}/messages", sessionH.GetMessages).Methods(http.MethodGet)

		// Projects
		api.HandleFunc("/projects", projectH.List).Methods(http.MethodGet)
		api.HandleFunc("/projects", projectH.Create).Methods(http.MethodPost)
		api.HandleFunc("/projects/{id}", projectH.Get).Methods(http.MethodGet)
		api.HandleFunc("/projects/{id}", projectH.Update).Methods(http.MethodPut)
		api.HandleFunc("/projects/{id}", projectH.Delete).Methods(http.MethodDelete)
		api.HandleFunc("/projects/{id}/archive", projectH.Archive).Methods(http.MethodPost)
		api.HandleFunc("/projects/{id}/unarchive", projectH.Unarchive).Methods(http.MethodPost)
		api.HandleFunc("/projects/{id}/sessions", projectH.AddSession).Methods(http.MethodPost)
		api.HandleFunc("/projects/{id}/sessions/{session_id}", projectH.RemoveSession).Methods(http.MethodDelete)
		api.HandleFunc("/sessions/{id}/projects", projectH.ListForSession).Methods(http.MethodGet)

		// MCP servers (T-016)
		api.HandleFunc("/mcp/servers", mcpH2.Register).Methods(http.MethodPost)
		api.HandleFunc("/mcp/servers", mcpH2.List).Methods(http.MethodGet)
		api.HandleFunc("/mcp/servers/{id}", mcpH2.Delete).Methods(http.MethodDelete)
		api.HandleFunc("/mcp/servers/{id}/toggle", mcpH2.ToggleUser).Methods(http.MethodPost)

		// Core-backed endpoints (transparent reverse proxy; identity injected
		// from JWT as X-Auth-* headers). These proxy to the same paths on core.
		coreProxy := handler.CoreReverseProxy(cfg.RustCoreURL)
		api.PathPrefix("/knowledge/documents").Handler(coreProxy)
		api.PathPrefix("/styles").Handler(coreProxy)
		// Search endpoints: public path → core internal path.
		api.HandleFunc("/knowledge/search",
			func(w http.ResponseWriter, r *http.Request) {
				handler.CorePathRewriteProxy(cfg.RustCoreURL, "/internal/knowledge/search").
					ServeHTTP(w, r)
			}).Methods(http.MethodPost)
		// Core MCP tool discovery (single sub-path under the Go-managed group).
		api.HandleFunc("/mcp/servers/{id}/tools",
			func(w http.ResponseWriter, r *http.Request) { coreProxy.ServeHTTP(w, r) }).
			Methods(http.MethodGet)
		// Public-path image search → core internal endpoint.
		api.HandleFunc("/images/similar",
			func(w http.ResponseWriter, r *http.Request) {
				handler.CorePathRewriteProxy(cfg.RustCoreURL, "/internal/images/similar").
					ServeHTTP(w, r)
			}).Methods(http.MethodPost)

		// OpenAI-compatible routes
		v1 := r.PathPrefix("/v1").Subrouter()
		v1.Use(jwtMw)
		v1.Use(rlMw)
		v1.Use(rbacMw)

		v1.HandleFunc("/models", chatH.Models).Methods(http.MethodGet)
		v1.HandleFunc("/chat/completions", chatH.Completions).Methods(http.MethodPost)
		v1.HandleFunc("/chat/upload-image",
			func(w http.ResponseWriter, r *http.Request) {
				coreProxy.ServeHTTP(w, r)
			}).Methods(http.MethodPost)

		// WebSocket (JWT via query param)
		ws := r.PathPrefix("/ws").Subrouter()
		ws.HandleFunc("/chat", chatH.WSChat).Methods(http.MethodGet)
	}

	// ── Shared: frontend dist path ────────────────────────────────────────────
	webDist := os.Getenv("WEB_DIST")

	// ── Uploads static file serving ────────────────────────────────────────────
	// Serve uploaded chat images so the frontend can embed them in chat messages.
	// Images are saved relative to the project root (where api-core runs from).
	if webDist != "" {
		uploadsDir := filepath.Join(filepath.Dir(webDist), "..", "uploads")
		uploadsDir, _ = filepath.EvalSymlinks(uploadsDir)
		if _, err := os.Stat(uploadsDir); err == nil {
			r.PathPrefix("/uploads/").Handler(
				http.StripPrefix("/uploads/", http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
					http.ServeFile(w, r, uploadsDir+"/"+filepath.Clean(r.URL.Path))
				})),
			)
			log.Println("[UPLOADS] serving uploads from:", uploadsDir, "at /uploads/")
		}
	}

	// ── SPA static file serving ───────────────────────────────────────────────
	// Serve the built web-client from the same origin as the API.
	// This eliminates the Vite proxy layer entirely and makes cookies and SSE
	// work correctly in development without CORS issues.
	// ── SPA fallback (catch-all) ─────────────────────────────────────────────
	// Must be LAST so API routes always take priority.
	// Falls back to index.html for any path without a file extension
	// (supports client-side routing: /, /login, /project/:id, etc.)
	if webDist != "" {
		r.PathPrefix("/").Handler(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			clean := filepath.Clean(r.URL.Path)
			// Serve real files as-is (assets, fonts, favicon, etc.)
			if !strings.Contains(clean, ".") == false {
				// Path has an extension — try to serve as static file
				info, err := os.Stat(webDist + clean)
				if err == nil && !info.IsDir() {
					http.ServeFile(w, r, webDist+clean)
					return
				}
			}
			// SPA fallback: index.html for client-side routing
			http.ServeFile(w, r, webDist+"/index.html")
		}))
		log.Println("[SPA] serving built frontend from:", webDist, "at / (fallback)")
	}

	// ── Server ────────────────────────────────────────────────────────────────
	srv := &http.Server{
		Addr:         ":" + cfg.Port,
		Handler:      r,
		ReadTimeout:  15 * time.Second,
		// WriteTimeout 需覆盖 dreamina 生图/生视频全流程（提交+轮询最长 120s+LLM 总结），
		// 0 = 不限制（由前端 abort 控制生命周期）
		WriteTimeout: 0,
		IdleTimeout:  120 * time.Second,
	}

	go func() {
		sigCh := make(chan os.Signal, 1)
		signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)
		<-sigCh
		log.Println("[SHUTDOWN] signal received, stopping server...")
		cancel()
		shutdownCtx, shutdownCancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer shutdownCancel()
		if err := srv.Shutdown(shutdownCtx); err != nil {
			log.Printf("[SHUTDOWN] error: %v", err)
		}
	}()

	log.Printf("[START] api-gateway listening on :%s", cfg.Port)
	if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
		log.Fatalf("[FATAL] server error: %v", err)
	}
	log.Println("[STOP] server stopped")
}
