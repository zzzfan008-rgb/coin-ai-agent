-- =============================================================================
-- Migration 016: 用户级 MCP server 启用开关（T-016）
-- =============================================================================
-- users.mcp_server_ids 存储该用户已启用的 mcp_servers.id 列表（JSON 数组）。
-- MCP server 配置本身存于 001 创建的 mcp_servers 表；
-- 本列实现"用户启用/停用 Server"，默认空数组（未启用任何 server）。
-- =============================================================================

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS mcp_server_ids JSONB NOT NULL DEFAULT '[]';
