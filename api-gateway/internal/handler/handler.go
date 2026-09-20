package handler

import (
	"encoding/json"
	"errors"
	"net/http"

	"github.com/google/uuid"
	"github.com/gorilla/mux"
	"fashionai/api-gateway/internal/middleware"
	"fashionai/api-gateway/internal/model"
	"fashionai/api-gateway/internal/service"
)

type AuthHandler struct {
	svc      *service.AuthService
	auditSvc *service.AuditService
}

func NewAuthHandler(svc *service.AuthService, auditSvc *service.AuditService) *AuthHandler {
	return &AuthHandler{svc: svc, auditSvc: auditSvc}
}

// Register godoc
// @Summary 用户注册（同时创建组织和默认部门）
// @Tags auth
// @Accept json
// @Produce json
// @Param request body model.RegisterRequest true "注册信息"
// @Success 201 {object} model.AuthResponse
// @Failure 400 {object} model.ErrorResponse
// @Failure 409 {object} model.ErrorResponse
// @Router /auth/register [post]
func (h *AuthHandler) Register(w http.ResponseWriter, r *http.Request) {
	var req model.RegisterRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "invalid request body")
		return
	}
	if req.Username == "" || req.Password == "" || req.OrgName == "" || req.DeptName == "" {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "username, password, org_name, dept_name are required")
		return
	}
	if len(req.Password) < 6 {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "password must be at least 6 characters")
		return
	}

	resp, err := h.svc.Register(r.Context(), req)
	if err != nil {
		if errors.Is(err, service.ErrUserExists) {
			writeError(w, http.StatusConflict, "CONFLICT", "username already exists")
			return
		}
		writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", err.Error())
		return
	}
	if h.auditSvc != nil {
		userID, _ := uuid.Parse(resp.UserID)
		orgID, _ := uuid.Parse(resp.OrgID)
		go h.auditSvc.Log(r.Context(), orgID, userID, "auth.register", "user", &userID, middleware.GetClientIP(r))
	}
	writeJSON(w, http.StatusCreated, resp)
}

// Login godoc
// @Summary 用户登录
// @Tags auth
// @Accept json
// @Produce json
// @Param request body model.LoginRequest true "登录信息"
// @Success 200 {object} model.AuthResponse
// @Failure 401 {object} model.ErrorResponse
// @Router /auth/login [post]
func (h *AuthHandler) Login(w http.ResponseWriter, r *http.Request) {
	var req model.LoginRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "invalid request body")
		return
	}
	if req.Username == "" || req.Password == "" {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "username and password are required")
		return
	}

	resp, err := h.svc.Login(r.Context(), req)
	if err != nil {
		if errors.Is(err, service.ErrInvalidCreds) {
			writeError(w, http.StatusUnauthorized, "UNAUTHORIZED", "invalid username or password")
			return
		}
		writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", err.Error())
		return
	}
	if h.auditSvc != nil {
		userID, _ := uuid.Parse(resp.UserID)
		orgID, _ := uuid.Parse(resp.OrgID)
		go h.auditSvc.Log(r.Context(), orgID, userID, "auth.login", "user", &userID, middleware.GetClientIP(r))
	}
	writeJSON(w, http.StatusOK, resp)
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(v)
}

func writeError(w http.ResponseWriter, status int, code, msg string) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(model.ErrorResponse{
		Error: model.APIError{Code: code, Message: msg},
	})
}

// UserHandler handles user management endpoints.
type UserHandler struct {
	svc *service.UserService
}

func NewUserHandler(svc *service.UserService) *UserHandler {
	return &UserHandler{svc: svc}
}

// List godoc
// @Summary 用户列表（仅管理员）
// @Tags users
// @Produce json
// @Success 200 {object} model.UserList
// @Failure 403 {object} model.ErrorResponse
// @Router /api/users [get]
func (h *UserHandler) List(w http.ResponseWriter, r *http.Request) {
	claims := middleware.GetClaims(r.Context())
	if claims == nil {
		writeError(w, http.StatusUnauthorized, "UNAUTHORIZED", "missing claims")
		return
	}
	if claims.Role != "admin" {
		writeError(w, http.StatusForbidden, "FORBIDDEN", "admin role required")
		return
	}

	orgID, _ := parseUUID(claims.OrgID)
	list, err := h.svc.List(r.Context(), orgID)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", err.Error())
		return
	}
	writeJSON(w, http.StatusOK, list)
}

// DeptHandler handles department endpoints.
type DeptHandler struct {
	svc *service.DeptService
}

func NewDeptHandler(svc *service.DeptService) *DeptHandler {
	return &DeptHandler{svc: svc}
}

// List godoc
// @Summary 部门列表
// @Tags depts
// @Produce json
// @Success 200 {object} model.DeptList
// @Router /api/depts [get]
func (h *DeptHandler) List(w http.ResponseWriter, r *http.Request) {
	claims := middleware.GetClaims(r.Context())
	orgID, _ := parseUUID(claims.OrgID)
	list, err := h.svc.List(r.Context(), orgID)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", err.Error())
		return
	}
	writeJSON(w, http.StatusOK, list)
}

// Create godoc
// @Summary 创建部门
// @Tags depts
// @Accept json
// @Produce json
// @Param request body model.CreateDeptRequest true "部门信息"
// @Success 201 {object} model.Dept
// @Router /api/depts [post]
func (h *DeptHandler) Create(w http.ResponseWriter, r *http.Request) {
	var req model.CreateDeptRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "invalid request body")
		return
	}
	if req.Name == "" {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "name is required")
		return
	}

	claims := middleware.GetClaims(r.Context())
	orgID, _ := parseUUID(claims.OrgID)
	dept, err := h.svc.Create(r.Context(), orgID, req)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", err.Error())
		return
	}
	writeJSON(w, http.StatusCreated, dept)
}

// RoleHandler handles role endpoints.
type RoleHandler struct {
	svc *service.RoleService
}

func NewRoleHandler(svc *service.RoleService) *RoleHandler {
	return &RoleHandler{svc: svc}
}

// List godoc
// @Summary 角色列表
// @Tags roles
// @Produce json
// @Success 200 {object} model.RoleList
// @Router /api/roles [get]
func (h *RoleHandler) List(w http.ResponseWriter, r *http.Request) {
	claims := middleware.GetClaims(r.Context())
	orgID, _ := parseUUID(claims.OrgID)
	list, err := h.svc.List(r.Context(), orgID)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", err.Error())
		return
	}
	writeJSON(w, http.StatusOK, list)
}

// Create godoc
// @Summary 创建角色
// @Tags roles
// @Accept json
// @Produce json
// @Param request body model.CreateRoleRequest true "角色信息"
// @Success 201 {object} model.Role
// @Router /api/roles [post]
func (h *RoleHandler) Create(w http.ResponseWriter, r *http.Request) {
	var req model.CreateRoleRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "invalid request body")
		return
	}
	if req.Name == "" {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "name is required")
		return
	}

	claims := middleware.GetClaims(r.Context())
	orgID, _ := parseUUID(claims.OrgID)
	role, err := h.svc.Create(r.Context(), orgID, req)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", err.Error())
		return
	}
	writeJSON(w, http.StatusCreated, role)
}

// SessionHandler handles session endpoints.
type SessionHandler struct {
	svc     *service.SessionService
	chatSvc *service.ChatService
}

func NewSessionHandler(svc *service.SessionService, chatSvc *service.ChatService) *SessionHandler {
	return &SessionHandler{svc: svc, chatSvc: chatSvc}
}

// List godoc
// @Summary 会话列表
// @Tags sessions
// @Produce json
// @Param archived query bool false "仅归档会话"
// @Param limit query int false "每页数量" default(20)
// @Param offset query int false "偏移量" default(0)
// @Success 200 {object} model.SessionList
// @Router /api/sessions [get]
func (h *SessionHandler) List(w http.ResponseWriter, r *http.Request) {
	claims := middleware.GetClaims(r.Context())
	orgID, _ := parseUUID(claims.OrgID)
	userID, _ := parseUUID(claims.Subject)

	archived := r.URL.Query().Get("archived") == "true"
	limit := parseInt(r.URL.Query().Get("limit"), 20)
	offset := parseInt(r.URL.Query().Get("offset"), 0)

	list, err := h.svc.List(r.Context(), orgID, userID, archived, limit, offset)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", err.Error())
		return
	}
	writeJSON(w, http.StatusOK, list)
}

// Create godoc
// @Summary 创建会话
// @Tags sessions
// @Accept json
// @Produce json
// @Param request body model.CreateSessionRequest true "会话信息"
// @Success 201 {object} model.Session
// @Router /api/sessions [post]
func (h *SessionHandler) Create(w http.ResponseWriter, r *http.Request) {
	var req model.CreateSessionRequest
	json.NewDecoder(r.Body).Decode(&req)

	claims := middleware.GetClaims(r.Context())
	orgID, _ := parseUUID(claims.OrgID)
	userID, _ := parseUUID(claims.Subject)

	session, err := h.svc.Create(r.Context(), orgID, userID, req)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", err.Error())
		return
	}
	writeJSON(w, http.StatusCreated, session)
}

// Get godoc
// @Summary 获取会话详情
// @Tags sessions
// @Produce json
// @Param id path string true "会话ID"
// @Success 200 {object} model.Session
// @Failure 404 {object} model.ErrorResponse
// @Router /api/sessions/{id} [get]
func (h *SessionHandler) Get(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	sessionID, err := parseUUID(vars["id"])
	if err != nil {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "invalid session id")
		return
	}

	claims := middleware.GetClaims(r.Context())
	orgID, _ := parseUUID(claims.OrgID)

	session, err := h.svc.Get(r.Context(), orgID, sessionID)
	if err != nil {
		if errors.Is(err, service.ErrSessionNotFound) {
			writeError(w, http.StatusNotFound, "NOT_FOUND", "session not found")
			return
		}
		writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", err.Error())
		return
	}
	writeJSON(w, http.StatusOK, session)
}

// Update godoc
// @Summary 更新会话（如归档）
// @Tags sessions
// @Accept json
// @Produce json
// @Param id path string true "会话ID"
// @Param request body model.UpdateSessionRequest true "更新信息"
// @Success 200 {object} model.Session
// @Router /api/sessions/{id} [patch]
func (h *SessionHandler) Update(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	sessionID, err := parseUUID(vars["id"])
	if err != nil {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "invalid session id")
		return
	}

	var req model.UpdateSessionRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "invalid request body")
		return
	}

	claims := middleware.GetClaims(r.Context())
	orgID, _ := parseUUID(claims.OrgID)

	session, err := h.svc.Update(r.Context(), orgID, sessionID, req)
	if err != nil {
		if errors.Is(err, service.ErrSessionNotFound) {
			writeError(w, http.StatusNotFound, "NOT_FOUND", "session not found")
			return
		}
		writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", err.Error())
		return
	}
	writeJSON(w, http.StatusOK, session)
}

// GetMessages godoc
// @Summary 获取会话消息历史
// @Tags sessions
// @Produce json
// @Param id path string true "会话ID"
// @Param limit query int false "消息数量" default(50)
// @Param before query string false "游标（最后一条消息的id）"
// @Success 200 {object} model.MessageList
// @Failure 404 {object} model.ErrorResponse
// @Router /api/sessions/{id}/messages [get]
func (h *SessionHandler) GetMessages(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	sessionID, err := parseUUID(vars["id"])
	if err != nil {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "invalid session id")
		return
	}

	limit := parseInt(r.URL.Query().Get("limit"), 50)
	before := r.URL.Query().Get("before")

	list, err := h.svc.GetMessages(r.Context(), sessionID, limit, before)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", err.Error())
		return
	}
	writeJSON(w, http.StatusOK, list)
}

// HealthHandler handles health check.
type HealthHandler struct {
	chatSvc *service.ChatService
}

func NewHealthHandler(chatSvc *service.ChatService) *HealthHandler {
	return &HealthHandler{chatSvc: chatSvc}
}

// Health godoc
// @Summary 健康检查
// @Tags health
// @Produce json
// @Success 200 {object} model.HealthResponse
// @Router /health [get]
func (h *HealthHandler) Health(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, http.StatusOK, model.HealthResponse{
		Status:  "ok",
		Version: "1.0.0",
		Services: map[string]model.ServiceStatus{
			"database":  {Status: "ok"},
			"redis":     {Status: "ok"},
			"rust_core": {Status: "ok"},
		},
	})
}

// Utility helpers

func parseUUID(s string) (uuid.UUID, error) {
	return uuid.Parse(s)
}

func parseInt(s string, fallback int) int {
	var n int
	if err := json.Unmarshal([]byte(s), &n); err != nil {
		return fallback
	}
	return n
}
