package handler

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"log"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"

	"fashionai/api-gateway/internal/middleware"
	"fashionai/api-gateway/internal/model"
	"fashionai/api-gateway/internal/service"
	"fashionai/api-gateway/pkg/auth"
	"github.com/google/uuid"
	"github.com/gorilla/websocket"
)

// ChatHandler handles OpenAI-compatible chat endpoints and WebSocket.
type ChatHandler struct {
	chatSvc *service.ChatService
	jwtSvc  *auth.JWTService
	sessSvc *service.SessionService
}

func NewChatHandler(chatSvc *service.ChatService, jwtSvc *auth.JWTService, sessSvc *service.SessionService) *ChatHandler {
	return &ChatHandler{chatSvc: chatSvc, jwtSvc: jwtSvc, sessSvc: sessSvc}
}

// allowedOrigins is comma-separated; falls back to a single entry for local dev.
func isOriginAllowed(origin string) bool {
	env := os.Getenv("ALLOWED_ORIGINS")
	if env == "" {
		// Local dev fallback — restrict to localhost variants only.
		return strings.HasPrefix(origin, "http://localhost") ||
			strings.HasPrefix(origin, "http://127.0.0.1")
	}
	for _, allowed := range strings.Split(env, ",") {
		allowed = strings.TrimSpace(allowed)
		if allowed != "" && (origin == allowed || origin == allowed+"/") {
			return true
		}
	}
	return false
}

var upgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool {
		return isOriginAllowed(r.Header.Get("Origin"))
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
		req.Model = "qwen3.8-flash"
	}
	switch req.Model {
	case "gpt-4o", "qwen3.8", "qwen3.8-flash", "qwen3.8-max", "qwen-plus", "fashion-ai-default":
		req.Model = "deepseek-v4-pro"
	}

	isStream := req.Stream != nil && *req.Stream
	log.Printf("[chat completions] model=%s stream=%v", req.Model, isStream)

	claims := middleware.GetClaims(r.Context())
	userID, orgID, deptID, role := "", "", "", ""
	if claims != nil {
		userID, orgID, deptID, role = claims.Subject, claims.OrgID, claims.DeptID, claims.Role
	}

	// ── Message persistence: extract session_id from ExtraBody ──
	sessionIDStr, _ := req.ExtraBody["session_id"].(string)
	var sessionID uuid.UUID
	hasSession := sessionIDStr != ""
	if hasSession {
		sessionID, _ = uuid.Parse(sessionIDStr)
		hasSession = sessionID != uuid.Nil
	}

	// T-022: SSE timeout. Read SSE_WRITE_TIMEOUT_SECS from env (default 600s) so long
	// streams are not cut by the server. WriteTimeout on the http.Server stays 0.
	sseWriteSecs := 600
	if v, err := strconv.Atoi(os.Getenv("SSE_WRITE_TIMEOUT_SECS")); err == nil && v > 0 {
		sseWriteSecs = v
	}
	ctx, cancel := context.WithTimeout(context.Background(), time.Duration(sseWriteSecs)*time.Second)
	defer cancel()

	var resp *http.Response
	var err error

	// ── Direct LLM call: skip Rust core, call BerryPi token-plan gateway ──
	berrypiKey := os.Getenv("BERRYPI_API_KEY")
	berrypiURL := os.Getenv("BERRYPI_BASE_URL")
	if berrypiKey != "" && berrypiURL != "" {
		// rewrite model name
		switch req.Model {
		case "deepseek-v4-pro", "deepseek-v4-flash", "qwen3.8-max", "qwen-plus", "fashion-ai-default", "gpt-4o", "qwen3.8", "qwen3.8-flash":
			req.Model = "deepseek-v4-flash"
		}
		bodyBytes, _ := json.Marshal(req)
		coreReq, _ := http.NewRequestWithContext(ctx, http.MethodPost, berrypiURL+"/v1/chat/completions", bytes.NewReader(bodyBytes))
		coreReq.Header.Set("Content-Type", "application/json")
		coreReq.Header.Set("Authorization", "Bearer "+berrypiKey)
		resp, err = http.DefaultClient.Do(coreReq)
		if err != nil {
			log.Printf("[chat completions] BerryPi error: %v", err)
			writeError(w, http.StatusBadGateway, "UPSTREAM_ERROR", "failed to reach BerryPi")
			return
		}
		defer resp.Body.Close()
		log.Printf("[chat completions] BerryPi status=%d", resp.StatusCode)
		if resp.StatusCode != http.StatusOK {
			bodyBytes, _ := io.ReadAll(io.LimitReader(resp.Body, 1024))
			log.Printf("[chat completions] BerryPi non-200: status=%d body=%s", resp.StatusCode, string(bodyBytes))
			resp.Body = io.NopCloser(bytes.NewReader(bodyBytes))
		}
	} else {
		// fallback to Rust core
		resp, err = h.chatSvc.ProxyRequestWithContext(ctx, req, userID, orgID, deptID, role, middleware.GetClientIP(r))
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

		// Persist user message (last user msg in history)
		if hasSession && h.sessSvc != nil {
			for i := len(req.Messages) - 1; i >= 0; i-- {
				if req.Messages[i].Role == "user" && req.Messages[i].Content != "" {
					if err := h.sessSvc.SaveMessage(ctx, sessionID, "user", req.Messages[i].Content, req.Model); err != nil {
						log.Printf("[chat completions] SaveMessage(user) error: %v", err)
					}
					break
				}
			}
		}

		buf := make([]byte, 4096)
		first := true
		var assistantBuf bytes.Buffer
		for {
			n, readErr := resp.Body.Read(buf)
			if n > 0 {
				log.Printf("[chat completions] streaming chunk len=%d first=%v", n, first)
				_, _ = w.Write(buf[:n])
				flusher.Flush()
				first = false
				assistantBuf.Write(buf[:n])
			}
			if readErr != nil {
				log.Printf("[chat completions] stream ended err=%v", readErr)
				break
			}
		}

		// Parse accumulated SSE → extract assistant content
		if hasSession && h.sessSvc != nil {
			fullText := parseSSEAssistantContent(assistantBuf.String())
			if fullText != "" {
				if err := h.sessSvc.SaveMessage(ctx, sessionID, "assistant", fullText, req.Model); err != nil {
					log.Printf("[chat completions] SaveMessage(assistant) error: %v", err)
				}
			}
		}
	} else {
		if hasSession && h.sessSvc != nil {
			for i := len(req.Messages) - 1; i >= 0; i-- {
				if req.Messages[i].Role == "user" && req.Messages[i].Content != "" {
					if err := h.sessSvc.SaveMessage(ctx, sessionID, "user", req.Messages[i].Content, req.Model); err != nil {
						log.Printf("[chat completions] SaveMessage(user) error: %v", err)
					}
					break
				}
			}
		}

		var buf bytes.Buffer
		io.Copy(&buf, resp.Body)
		w.Header().Set("Content-Type", "application/json")
		w.Write(buf.Bytes())

		if hasSession && h.sessSvc != nil {
			var chatResp model.ChatCompletionResponse
			if json.Unmarshal(buf.Bytes(), &chatResp) == nil && len(chatResp.Choices) > 0 {
				assistantText := chatResp.Choices[0].Message.Content
				if assistantText != "" {
					if err := h.sessSvc.SaveMessage(ctx, sessionID, "assistant", assistantText, req.Model); err != nil {
						log.Printf("[chat completions] SaveMessage(assistant) error: %v", err)
					}
				}
			}
		}
	}
}

// parseSSEAssistantContent parses OpenAI SSE stream and extracts assistant message content.
func parseSSEAssistantContent(sseData string) string {
	var sb strings.Builder
	lines := strings.Split(sseData, "\n")
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if !strings.HasPrefix(line, "data:") {
			continue
		}
		payload := strings.TrimSpace(strings.TrimPrefix(line, "data:"))
		if payload == "[DONE]" {
			break
		}
		var chunk model.ChatStreamResponse
		if json.Unmarshal([]byte(payload), &chunk) == nil && len(chunk.Choices) > 0 {
			if chunk.Choices[0].Delta.Content != "" {
				sb.WriteString(chunk.Choices[0].Delta.Content)
			}
		}
	}
	return sb.String()
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

	// Verify Origin before WebSocket upgrade (same policy as HTTP CheckOrigin).
	if !isOriginAllowed(r.Header.Get("Origin")) {
		writeError(w, http.StatusForbidden, "ORIGIN_FORBIDDEN", "disallowed origin")
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
