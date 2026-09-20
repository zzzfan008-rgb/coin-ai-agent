package main

import (
	"context"
	"fmt"
	"log"
	"net/http"
	"os"
	"os/signal"
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
	_ = godotenv.Load()

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
		auditSvc     *service.AuditService
		chatH        *handler.ChatHandler
	)
	if pool != nil {
		authSvc     = service.NewAuthService(pool, jwtSvc)
		userSvc     = service.NewUserService(pool)
		deptSvc     = service.NewDeptService(pool)
		roleSvc     = service.NewRoleService(pool)
		sessionSvc  = service.NewSessionService(pool)
		auditSvc    = service.NewAuditService(pool)
	}

	chatSvc := service.NewChatService(cfg.RustCoreURL)

	// ── Handlers ─────────────────────────────────────────────────────────────
	var authH    *handler.AuthHandler
	var userH    *handler.UserHandler
	var deptH    *handler.DeptHandler
	var roleH    *handler.RoleHandler
	var sessionH *handler.SessionHandler
	var healthH  *handler.HealthHandler

	if pool != nil {
		authH    = handler.NewAuthHandler(authSvc, auditSvc)
		userH    = handler.NewUserHandler(userSvc)
		deptH    = handler.NewDeptHandler(deptSvc)
		roleH    = handler.NewRoleHandler(roleSvc)
		sessionH = handler.NewSessionHandler(sessionSvc, chatSvc)
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
		handlers.AllowedHeaders([]string{"Authorization", "Content-Type", "X-Request-ID"}),
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
	if userH != nil && deptH != nil && roleH != nil && sessionH != nil {
		api := r.PathPrefix("/api").Subrouter()
		api.Use(jwtMw)
		api.Use(rlMw)
		api.Use(rbacMw)
		if auditSvc != nil {
			api.Use(middleware.AuditLogger(auditSvc))
		}

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

		// OpenAI-compatible routes
		v1 := r.PathPrefix("/v1").Subrouter()
		v1.Use(jwtMw)
		v1.Use(rlMw)
		v1.Use(rbacMw)

		v1.HandleFunc("/models", chatH.Models).Methods(http.MethodGet)
		v1.HandleFunc("/chat/completions", chatH.Completions).Methods(http.MethodPost)

		// WebSocket (JWT via query param)
		ws := r.PathPrefix("/ws").Subrouter()
		ws.HandleFunc("/chat", chatH.WSChat).Methods(http.MethodGet)
	}

	// ── Server ───────────────────────────────────────────────────────────────
	srv := &http.Server{
		Addr:         ":" + cfg.Port,
		Handler:      r,
		ReadTimeout:  15 * time.Second,
		WriteTimeout: 60 * time.Second,
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
