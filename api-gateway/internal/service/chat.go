package service

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"

	"fashionai/api-gateway/internal/model"
)

// ChatService proxies to the Rust core service.
type ChatService struct {
	rustCoreURL string
	httpClient  *http.Client
}

func NewChatService(rustCoreURL string) *ChatService {
	return &ChatService{
		rustCoreURL: rustCoreURL,
		httpClient:  &http.Client{Timeout: 60 * time.Second}, // 60s
	}
}

// ProxyRequest forwards a chat completion request to the Rust core.
// For stream=false: returns the full response body.
// For stream=true: caller reads from the returned ReadCloser (SSE).
func (s *ChatService) ProxyRequest(ctx context.Context, req model.ChatCompletionRequest) (*http.Response, error) {
	body, err := json.Marshal(req)
	if err != nil {
		return nil, fmt.Errorf("marshal request: %w", err)
	}

	httpReq, err := http.NewRequestWithContext(ctx, http.MethodPost,
		s.rustCoreURL+"/v1/chat/completions", bytes.NewReader(body))
	if err != nil {
		return nil, fmt.Errorf("create request: %w", err)
	}
	httpReq.Header.Set("Content-Type", "application/json")

	resp, err := s.httpClient.Do(httpReq)
	if err != nil {
		return nil, fmt.Errorf("call rust core: %w", err)
	}
	return resp, nil
}

// ProxyRequestWithContext forwards a chat completion request to the Rust core,
// injecting user_context built from JWT claims.
func (s *ChatService) ProxyRequestWithContext(ctx context.Context, req model.ChatCompletionRequest, userID, orgID, deptID, role, ipAddress string) (*http.Response, error) {
	req.UserContext = &model.UserContext{
		UserID:    userID,
		OrgID:     orgID,
		DeptID:    deptID,
		Role:      role,
		IPAddress: ipAddress,
	}
	return s.ProxyRequest(ctx, req)
}

// GetModels returns the available model list from Rust core (or returns defaults).
func (s *ChatService) GetModels(ctx context.Context) (*model.ModelList, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, s.rustCoreURL+"/v1/models", nil)
	if err != nil {
		return nil, err
	}

	resp, err := s.httpClient.Do(req)
	if err != nil || resp.StatusCode != http.StatusOK {
		// Fallback: return configured models
		return s.defaultModels(), nil
	}
	defer resp.Body.Close()

	var ml model.ModelList
	if err := json.NewDecoder(resp.Body).Decode(&ml); err != nil {
		return s.defaultModels(), nil
	}
	return &ml, nil
}

func (s *ChatService) defaultModels() *model.ModelList {
	return &model.ModelList{
		Object: "list",
		Data: []model.ModelEntry{
			{ID: "gpt-4o", Object: "model", Created: 1712361441, OwnedBy: "fashion-ai"},
			{ID: "deepseek-chat", Object: "model", Created: 1712361441, OwnedBy: "fashion-ai"},
		},
	}
}

// ReadBody reads and closes the response body.
func ReadBody(resp *http.Response) ([]byte, error) {
	defer resp.Body.Close()
	return io.ReadAll(resp.Body)
}
