package middleware

import (
	"context"
	"encoding/json"
	"net/http"
	"time"

	"github.com/jmoiron/sqlx"
)

// DBGuard answers 503 {"status":"degraded"} when the primary database is
// unreachable, so protected routes surface a clear degraded signal instead of
// opaque 500s from handlers failing on a dead pool. A nil pool (gateway
// started without DB) is also treated as degraded.
func DBGuard(pool *sqlx.DB) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if pool == nil {
				writeDegraded(w)
				return
			}
			ctx, cancel := context.WithTimeout(r.Context(), 2*time.Second)
			defer cancel()
			if err := pool.PingContext(ctx); err != nil {
				writeDegraded(w)
				return
			}
			next.ServeHTTP(w, r)
		})
	}
}

func writeDegraded(w http.ResponseWriter) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusServiceUnavailable)
	_ = json.NewEncoder(w).Encode(map[string]string{"status": "degraded"})
}
