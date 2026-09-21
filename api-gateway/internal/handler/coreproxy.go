package handler

import (
	"log"
	"net/http"
	"net/http/httputil"
	"net/url"
	"strings"

	"fashionai/api-gateway/internal/middleware"
)

// CoreReverseProxy forwards a request to the Rust core unchanged (same path),
// stripping any client-supplied identity headers and re-injecting them from the
// verified JWT claims. The core therefore never has to trust org/dept values
// sent by the browser.
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
	}
	proxy.ErrorHandler = func(w http.ResponseWriter, r *http.Request, err error) {
		log.Printf("[ERROR] core proxy: %v", err)
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusBadGateway)
		_, _ = w.Write([]byte(`{"error":{"code":"CORE_UNREACHABLE","message":"core service unreachable"}}`))
	}
	return proxy
}
