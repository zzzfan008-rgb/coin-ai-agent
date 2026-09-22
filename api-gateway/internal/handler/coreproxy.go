package handler

import (
	"fmt"
	"log"
	"net/http"
	"net/http/httputil"
	"net/url"
	"strconv"
	"strings"

	"fashionai/api-gateway/internal/config"
	"fashionai/api-gateway/internal/middleware"
)

// CoreReverseProxy forwards a request to the Rust core unchanged (same path),
// stripping any client-supplied identity headers and re-injecting them from the
// verified JWT claims. The core therefore never has to trust org/dept values
// sent by the browser.
// For SSE responses it sets X-Accel-Buffering: no to prevent buffering, and
// injects X-SSE-Idle-Timeout / X-SSE-Write-Timeout headers (T-022).
func CoreReverseProxy(coreURL string) http.Handler {
	return buildProxy(coreURL, nil)
}

// CorePathRewriteProxy forwards to a fixed core path regardless of the
// incoming path. Used to expose internal core endpoints at public paths, e.g.
// POST /api/images/similar → /internal/images/similar.
func CorePathRewriteProxy(coreURL, dstPath string) http.Handler {
	return buildProxy(coreURL, func(req *http.Request) {
		req.URL.Path = dstPath
	})
}

func buildProxy(coreURL string, pathRewrite func(*http.Request)) http.Handler {
	target, err := url.Parse(strings.TrimRight(coreURL, "/"))
	if err != nil {
		log.Printf("[ERROR] invalid core URL %q: %v", coreURL, err)
	}
	proxy := httputil.NewSingleHostReverseProxy(target)

	// T-022: SSE timeout config (defaults match core defaults)
	sseIdle := 120
	sseWrite := 300
	if cfg := config.Load(); cfg != nil {
		if cfg.SSEIdleTimeoutSecs > 0 {
			sseIdle = cfg.SSEIdleTimeoutSecs
		}
		if cfg.SSEWriteTimeoutSecs > 0 {
			sseWrite = cfg.SSEWriteTimeoutSecs
		}
	}

	baseDirector := proxy.Director
	proxy.Director = func(req *http.Request) {
		baseDirector(req)
		if pathRewrite != nil {
			pathRewrite(req)
		}
		// Never forward spoofed identity headers.
		req.Header.Del("X-Auth-User-Id")
		req.Header.Del("X-Auth-Org-Id")
		req.Header.Del("X-Auth-Dept-Id")
		req.Header.Del("X-Auth-Role")
		if claims := middleware.GetClaims(req.Context()); claims != nil {
			req.Header.Set("X-Auth-User-Id", claims.Subject)
			req.Header.Set("X-Auth-Org-Id", claims.OrgID)
			req.Header.Set("X-Auth-Dept-Id", claims.DeptID)
			req.Header.Set("X-Auth-Role", claims.Role)
		}
		// T-022: Inject SSE timeout hints so core can configure timeouts
		// even when running behind the gateway (core receives these via env
		// and also via upstream headers).
		req.Header.Set("X-SSE-Idle-Timeout", strconv.Itoa(sseIdle))
		req.Header.Set("X-SSE-Write-Timeout", strconv.Itoa(sseWrite))
	}

	proxy.ModifyResponse = func(resp *http.Response) error {
		// T-022: for SSE responses, disable upstream buffering.
		// Check by Content-Type header to avoid needing to peek at request.
		if strings.Contains(resp.Header.Get("Content-Type"), "text/event-stream") {
			resp.Header.Set("X-Accel-Buffering", "no")
		}
		return nil
	}

	proxy.ErrorHandler = func(w http.ResponseWriter, r *http.Request, err error) {
		log.Printf("[ERROR] core proxy: %v", err)
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusBadGateway)
		_, _ = w.Write([]byte(fmt.Sprintf(
			`{"error":{"code":"CORE_UNREACHABLE","message":"核心服务不可用，请稍后重试"}}`,
		)))
	}
	return proxy
}
