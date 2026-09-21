-- =============================================================================
-- Migration 009: session_project 多对多关联表
-- =============================================================================

CREATE TABLE session_project (
    session_id  UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    project_id  UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    added_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (session_id, project_id)
);

CREATE INDEX IF NOT EXISTS idx_sp_session_id ON session_project(session_id);
CREATE INDEX IF NOT EXISTS idx_sp_project_id ON session_project(project_id);
