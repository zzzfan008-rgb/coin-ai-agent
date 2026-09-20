package handler

import (
	"encoding/json"
	"io"
	"log"
	"net/http"
	"strings"
	"time"

	"github.com/gorilla/websocket"
	"fashionai/api-gateway/internal/model"
	"fashionai/api-gateway/internal/service"
)

// ChatHandler handles OpenAI-compatible chat endpoints and WebSocket.
type ChatHandler struct {
	chatSvc *service.ChatService
}

func NewChatHandler(chatSvc *service.ChatService) *ChatHandler {
	return &ChatHandler{chatSvc: chatSvc}
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
func (h *ChatHandler) Completions(w http.ResponseWriter, r *http.Request) {
	var req model.ChatCompletionRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "BAD_REQUEST", "invalid request body")
		return
	}
	if req.Model == "" {
		req.Model = "gpt-4o"
	}

	// Proxy to Rust core
	resp, err := h.chatSvc.ProxyRequest(r.Context(), req)
	if err != nil {
		log.Printf("[chat] proxy error: %v", err)
		writeError(w, http.StatusBadGateway, "UPSTREAM_ERROR", "failed to reach core service")
		return
	}
	defer resp.Body.Close()

	// Check stream flag (pointer so may be nil)
	isStream := req.Stream != nil && *req.Stream
	if isStream {
		// Set SSE headers
		w.Header().Set("Content-Type", "text/event-stream; charset=utf-8")
		w.Header().Set("Cache-Control", "no-cache")
		w.Header().Set("Connection", "keep-alive")
		w.Header().Set("Transfer-Encoding", "chunked")
		w.WriteHeader(http.StatusOK)

		// Flush chunks as they arrive from Rust core
		flusher, ok := w.(http.Flusher)
		if !ok {
			writeError(w, http.StatusInternalServerError, "INTERNAL_ERROR", "streaming not supported")
			return
		}

		buf := make([]byte, 4096)
		for {
			n, err := resp.Body.Read(buf)
			if n > 0 {
				_, _ = w.Write(buf[:n])
				flusher.Flush()
			}
			if err != nil {
				break
			}
		}
	} else {
		// Non-streaming: forward the JSON response
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

	ws, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		log.Printf("[ws] upgrade error: %v", err)
		return
	}
	defer ws.Close()

	// Handle incoming messages (JSON frames)
	for {
		_, msgBytes, err := ws.ReadMessage()
		if err != nil {
			if websocket.IsUnexpectedCloseError(err, websocket.CloseGoingAway, websocket.CloseAbnormalClosure) {
				log.Printf("[ws] read error: %v", err)
			}
			break
		}

		// Expect a ChatCompletionRequest JSON frame
		var req model.ChatCompletionRequest
		if err := json.Unmarshal(msgBytes, &req); err != nil {
			ws.WriteJSON(map[string]string{"error": "bad_request: invalid JSON frame"})
			continue
		}

		// Force non-streaming for WebSocket simplicity
		falseVal := false
		req.Stream = &falseVal

		resp, err := h.chatSvc.ProxyRequest(r.Context(), req)
		if err != nil {
			ws.WriteJSON(map[string]string{"error": "upstream_error: core service unreachable"})
			continue
		}
		defer resp.Body.Close()

		// Forward the response back over WebSocket
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

// parseExtraBody extracts session_id, skill_ids, etc. from extra_body.
func parseExtraBody(body []byte) (*model.ExtraBody, error) {
	var raw struct {
		ExtraBody *model.ExtraBody `json:"extra_body"`
	}
	if err := json.Unmarshal(body, &raw); err != nil {
		return nil, err
	}
	return raw.ExtraBody, nil
}

// normalizePath converts paths like /api/sessions/{id} to /api/sessions/* for RBAC matching.
func normalizePath(template string) string {
	return strings.ReplaceAll(template, "{id}", "*")
}
