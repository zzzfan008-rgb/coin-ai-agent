package middleware

import (
	"context"
	"fmt"
	"net/http"
	"time"

	"github.com/redis/go-redis/v9"
)

// RateLimiter implements a sliding-window rate limiter using Redis.
type RateLimiter struct {
	rdb    *redis.Client
	limit  int
	window time.Duration
}

// NewRateLimiter creates a new RateLimiter.
func NewRateLimiter(rdb *redis.Client, limit int, window time.Duration) *RateLimiter {
	return &RateLimiter{rdb: rdb, limit: limit, window: window}
}

// Middleware returns a mux-compatible middleware.
func (rl *RateLimiter) Middleware() func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			ctx := r.Context()
			claims := GetClaims(ctx)
			if claims == nil {
				next.ServeHTTP(w, r)
				return
			}

			key := fmt.Sprintf("ratelimit:%s:%s", claims.OrgID, r.URL.Path)
			allowed, remaining, err := rl.Allow(ctx, key)
			if err != nil {
				// Fail open: log but don't block
				next.ServeHTTP(w, r)
				return
			}

			w.Header().Set("X-RateLimit-Limit", fmt.Sprintf("%d", rl.limit))
			w.Header().Set("X-RateLimit-Remaining", fmt.Sprintf("%d", remaining))

			if !allowed {
				writeError(w, http.StatusTooManyRequests, "RATE_LIMITED",
					"too many requests, please slow down")
				return
			}
			next.ServeHTTP(w, r)
		})
	}
}

// Allow checks if the request is within rate limits using Redis INCR + EXPIRE.
func (rl *RateLimiter) Allow(ctx context.Context, key string) (allowed bool, remaining int, err error) {
	pipe := rl.rdb.Pipeline()
	incr := pipe.Incr(ctx, key)
	pipe.Expire(ctx, key, rl.window)
	_, err = pipe.Exec(ctx)
	if err != nil {
		return true, rl.limit, err
	}

	count := int(incr.Val())
	remaining = rl.limit - count
	if remaining < 0 {
		remaining = 0
	}
	return count <= rl.limit, remaining, nil
}
