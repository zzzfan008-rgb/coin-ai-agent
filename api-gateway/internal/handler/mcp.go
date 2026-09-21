package handler

import (
	"encoding/json"
	"net/http"

	"github.com/google/uuid"
	"github.com/gorilla/mux"

	"fashionai/api-gateway/internal/middleware"
	"fashionai/api-gateway/internal/model"
	"fashionai/api-gateway/internal/service"
)

// McpHandler exposes MCP server CRUD + per-user enablement.
type McpHandler struct {
	svc *service.McpService
}

func NewMcpHandler(svc *service.McpService) *McpHandler {
	return &McpHandler{svc: svc}
}

// Register POST /api/mcp/servers — admin only.
func (h *McpHandler) Register(w http.ResponseWriter, r *http.Request) {
	claims := middleware.GetClaims(r.Context())
	if claims == nil {
		writeError(w, http.StatusUnauthorized, "UNAUTHORIZED", "missing claims")
		return
	}
	if claims.Role != "admin" {
		writeError(w, http.StatusForbidden, "FORBIDDEN", "admin role required")
		return
	}

	var req model.McpServerRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "invalid request body")
		return
	}
	if req.Name == "" {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "name is required")
		return
	}

	orgID, _ := uuid.Parse(claims.OrgID)
	userID, _ := uuid.Parse(claims.Subject)
	dto, err := h.svc.Register(r.Context(), orgID, userID, req)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", err.Error())
		return
	}
	writeJSON(w, http.StatusCreated, dto)
}

// List GET /api/mcp/servers.
func (h *McpHandler) List(w http.ResponseWriter, r *http.Request) {
	claims := middleware.GetClaims(r.Context())
	if claims == nil {
		writeError(w, http.StatusUnauthorized, "UNAUTHORIZED", "missing claims")
		return
	}
	orgID, _ := uuid.Parse(claims.OrgID)
	servers, err := h.svc.List(r.Context(), orgID)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", err.Error())
		return
	}
	writeJSON(w, http.StatusOK, model.McpServerList{Servers: servers, Total: len(servers)})
}

// Delete DELETE /api/mcp/servers/{id} — admin only.
func (h *McpHandler) Delete(w http.ResponseWriter, r *http.Request) {
	claims := middleware.GetClaims(r.Context())
	if claims == nil {
		writeError(w, http.StatusUnauthorized, "UNAUTHORIZED", "missing claims")
		return
	}
	if claims.Role != "admin" {
		writeError(w, http.StatusForbidden, "FORBIDDEN", "admin role required")
		return
	}

	id, err := uuid.Parse(mux.Vars(r)["id"])
	if err != nil {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "invalid server id")
		return
	}
	orgID, _ := uuid.Parse(claims.OrgID)
	if err := h.svc.Delete(r.Context(), orgID, id); err != nil {
		writeError(w, http.StatusNotFound, "NOT_FOUND", "mcp server not found")
		return
	}
	writeJSON(w, http.StatusOK, map[string]string{"status": "deleted"})
}

// ToggleUser PUT /api/mcp/servers/{id}/user — current user enable/disable.
func (h *McpHandler) ToggleUser(w http.ResponseWriter, r *http.Request) {
	claims := middleware.GetClaims(r.Context())
	if claims == nil {
		writeError(w, http.StatusUnauthorized, "UNAUTHORIZED", "missing claims")
		return
	}

	serverID, err := uuid.Parse(mux.Vars(r)["id"])
	if err != nil {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "invalid server id")
		return
	}

	var req model.McpUserToggleRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "invalid request body")
		return
	}

	orgID, _ := uuid.Parse(claims.OrgID)
	userID, _ := uuid.Parse(claims.Subject)
	ids, err := h.svc.SetUserEnabled(r.Context(), orgID, userID, serverID, req.Enabled)
	if err != nil {
		writeError(w, http.StatusNotFound, "NOT_FOUND", err.Error())
		return
	}
	writeJSON(w, http.StatusOK, model.McpUserServersResponse{McpServerIDs: ids})
}
