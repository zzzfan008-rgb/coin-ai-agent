package handler

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"log"
	"net/http"
	"strings"
	"time"

	"github.com/gorilla/websocket"
	"fashionai/api-gateway/internal/middleware"
	"fashionai/api-gateway/internal/model"
	"fashionai/api-gateway/internal/service"
	"fashionai/api-gateway/pkg/auth"
)

// ChatHandler handles OpenAI-compatible chat endpoints and WebSocket.
type ChatHandler struct {
	chatSvc *service.ChatService
	jwtSvc  *auth.JWTService
}

func NewChatHandler(chatSvc *service.ChatService, jwtSvc *auth.JWTService) *ChatHandler {
	return &ChatHandler{chatSvc: chatSvc, jwtSvc: jwtSvc}
}

var upgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool {
		return true // Allow all origins in dev; restrict in production
	},
	ReadBufferSize:  1024,
	WriteBufferSize: 1024,
}

// Completions handles POST /v1/chat/completions.
// Supports both stream=true (SSE) and stream=false (JSON).
// JWT claims are available via context (v1 router mounts JWTMiddleware).
func (h *ChatHandler) Completions(w http.ResponseWriter, r *http.Request) {
	var req model.ChatCompletionRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "invalid request body")
		return
	}

	if req.Model == "" {
		req.Model = "qwen-plus"
	}
	switch req.Model {
	case "gpt-4o", "fashion-ai-default":
		req.Model = "qwen-plus"
	}

	isStream := req.Stream != nil && *req.Stream
	log.Printf("[chat completions] model=%s stream=%v cookies=%v", req.Model, isStream, r.Cookies())
	log.Printf("[chat completions] rustCoreURL=%s", h.chatSvc.RustCoreURL())

	claims := middleware.GetClaims(r.Context())
	userID, orgID, deptID, role := "", "", "", ""
	if claims != nil {
		userID, orgID, deptID, role = claims.Subject, claims.OrgID, claims.DeptID, claims.Role
	}

	// Use a detached context with 90s timeout so browser cancel/refresh
	// doesn't abort the upstream core request mid-stream.
	ctx, cancel := context.WithTimeout(context.Background(), 90*time.Second)
	defer cancel()

	resp, err := h.chatSvc.ProxyRequestWithContext(ctx, req, userID, orgID, deptID, role, middleware.GetClientIP(r))
	if err != nil {
		log.Printf("[chat completions] ProxyRequestWithContext error: %v", err)
		writeError(w, http.StatusBadGateway, "UPSTREAM_ERROR", "failed to reach core service")
		return
	}
	defer resp.Body.Close()

	log.Printf("[chat completions] core status=%d", resp.StatusCode)
	if resp.StatusCode != http.StatusOK {
		bodyBytes, _ := io.ReadAll(io.LimitReader(resp.Body, 1024))
		log.Printf("[chat completions] core non-200: status=%d body=%s", resp.StatusCode, string(bodyBytes))
		resp.Body = io.NopCloser(bytes.NewReader(bodyBytes))
	}

	if isStream {
		w.Header().Set("Content-Type", "text/event-stream; charset=utf-8")
		w.Header().Set("Cache-Control", "no-cache")
		w.Header().Set("X-Accel-Buffering", "no")
		w.WriteHeader(http.StatusOK)

		flusher, ok := w.(http.Flusher)
		if !ok {
			writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", "streaming not supported")
			return
		}

		buf := make([]byte, 4096)
		first := true
		for {
			n, err := resp.Body.Read(buf)
			if n > 0 {
				log.Printf("[chat completions] streaming chunk len=%d first=%v", n, first)
				_, _ = w.Write(buf[:n])
				flusher.Flush()
				first = false
			}
			if err != nil {
				log.Printf("[chat completions] stream ended err=%v", err)
				break
			}
		}
	} else {
		w.Header().Set("Content-Type", "application/json")
		io.Copy(w, resp.Body)
	}
}

// Models handles GET /v1/models.
func (h *ChatHandler) Models(w http.ResponseWriter, r *http.Request) {
	ml, err := h.chatSvc.GetModels(r.Context())
	if err != nil {
		writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", err.Error())
		return
	}
	writeJSON(w, http.StatusOK, ml)
}

// WSChat handles WebSocket upgrade for real-time chat.
// Query param: ?token=<jwt>
func (h *ChatHandler) WSChat(w http.ResponseWriter, r *http.Request) {
	token := r.URL.Query().Get("token")
	if token == "" {
		writeError(w, http.StatusUnauthorized, "UNAUTHORIZED", "missing token")
		return
	}

	claims, err := h.jwtSvc.Validate(token)
	if err != nil {
		writeError(w, http.StatusUnauthorized, "UNAUTHORIZED", "invalid or expired token")
		return
	}

	userID, orgID, deptID, role := claims.Subject, claims.OrgID, claims.DeptID, claims.Role
	clientIP := middleware.GetClientIP(r)

	ws, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		log.Printf("[ws] upgrade error: %v", err)
		return
	}
	defer ws.Close()

	for {
		_, msgBytes, err := ws.ReadMessage()
		if err != nil {
			if websocket.IsUnexpectedCloseError(err, websocket.CloseGoingAway, websocket.CloseAbnormalClosure) {
				log.Printf("[ws] read error: %v", err)
			}
			break
		}

		var req model.ChatCompletionRequest
		if err := json.Unmarshal(msgBytes, &req); err != nil {
			ws.WriteJSON(map[string]string{"error": "bad_request: invalid JSON frame"})
			continue
		}

		falseVal := false
		req.Stream = &falseVal

		resp, err := h.chatSvc.ProxyRequestWithContext(r.Context(), req, userID, orgID, deptID, role, clientIP)
		if err != nil {
			ws.WriteJSON(map[string]string{"error": "upstream_error: core service unreachable"})
			continue
		}
		defer resp.Body.Close()

		var chatResp model.ChatCompletionResponse
		if err := json.NewDecoder(resp.Body).Decode(&chatResp); err != nil {
			ws.WriteJSON(map[string]string{"error": "parse_error: failed to parse core response"})
			continue
		}

		if err := ws.WriteJSON(chatResp); err != nil {
			log.Printf("[ws] write error: %v", err)
			break
		}
	}
}

// PingPongHandler responds to WebSocket ping/pong health checks.
type PingPongHandler struct{}

func NewPingPongHandler() *PingPongHandler {
	return &PingPongHandler{}
}

func (h *PingPongHandler) Ping(w http.ResponseWriter, r *http.Request) {
	ws, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		return
	}
	defer ws.Close()
	ws.SetReadDeadline(time.Now().Add(60 * time.Second))
	ws.SetWriteDeadline(time.Now().Add(10 * time.Second))
	for {
		msgType, _, err := ws.ReadMessage()
		if err != nil {
			break
		}
		if msgType == websocket.PingMessage {
			ws.WriteMessage(websocket.PongMessage, nil)
		}
	}
}

// normalizePath converts paths like /api/sessions/{id} to /api/sessions/* for RBAC matching.
func normalizePath(template string) string {
	return strings.ReplaceAll(template, "{id}", "*")
}


