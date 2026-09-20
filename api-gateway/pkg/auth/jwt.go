package auth

import (
	"errors"
	"time"

	"github.com/golang-jwt/jwt/v5"
	"github.com/google/uuid"
)

type Claims struct {
	jwt.RegisteredClaims
	OrgID  string `json:"org_id"`
	DeptID string `json:"dept_id"`
	Role   string `json:"role"`
}

type JWTService struct {
	secret   []byte
	expHours int
}

func NewJWTService(secret string, expHours int) *JWTService {
	return &JWTService{secret: []byte(secret), expHours: expHours}
}

func (s *JWTService) Generate(userID, orgID, deptID, role string) (string, error) {
	now := time.Now()
	claims := &Claims{
		RegisteredClaims: jwt.RegisteredClaims{
			Subject:   userID,
			ExpiresAt: jwt.NewNumericDate(now.Add(time.Duration(s.expHours) * time.Hour)),
			IssuedAt:  jwt.NewNumericDate(now),
		},
		OrgID:  orgID,
		DeptID: deptID,
		Role:   role,
	}
	token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
	return token.SignedString(s.secret)
}

func (s *JWTService) Validate(tokenStr string) (*Claims, error) {
	token, err := jwt.ParseWithClaims(tokenStr, &Claims{}, func(t *jwt.Token) (interface{}, error) {
		if _, ok := t.Method.(*jwt.SigningMethodHMAC); !ok {
			return nil, errors.New("unexpected signing method")
		}
		return s.secret, nil
	})
	if err != nil {
		return nil, err
	}
	claims, ok := token.Claims.(*Claims)
	if !ok || !token.Valid {
		return nil, errors.New("invalid token")
	}
	return claims, nil
}

func (s *JWTService) ExtractUserID(claims *Claims) uuid.UUID {
	id, _ := uuid.Parse(claims.Subject)
	return id
}

func (s *JWTService) ExtractOrgID(claims *Claims) uuid.UUID {
	id, _ := uuid.Parse(claims.OrgID)
	return id
}

func (s *JWTService) ExtractDeptID(claims *Claims) uuid.UUID {
	id, _ := uuid.Parse(claims.DeptID)
	return id
}
