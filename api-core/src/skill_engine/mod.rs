//! SKILL Engine — core runtime for skill discovery, registration, and execution.
//!
//! Architecture:
//! - `loader`   — scans skills/ dir, parses SKILL.md YAML frontmatter, hot-reloads on change
//! - `registry`  — in-memory map of skill_id → ToolDefinition (OpenAI format)
//! - `executor` — dispatches tool_name → mock data (Phase 1B) / run.py (Phase 2)
//! - `permissions` — reads skill:{id} permission from UserContext; returns 403 if absent
//!
//! Public API surface (this module):
//! - `SkillEngine::new()`   — load all skills at startup
//! - `list_skills()`        — returns serialised skill list for GET /internal/skills/list
//! - `execute_skill()`      — permission check + execution for POST /internal/skills/execute
//! - `route_tool_call()`    — called from Agent Loop to handle LLM tool_calls

pub mod color_theory;
pub mod executor;
pub mod fashion_db;
pub mod loader;
pub mod permissions;
pub mod registry;

use std::sync::RwLock;

use serde::{Deserialize, Serialize};

pub use executor::{execute_skill, route_tool_call};
pub use fashion_db::FashionStore;
pub use loader::SkillLoader;
pub use permissions::check_skill_permission;
pub use registry::ToolRegistry;

/// Global skill engine — loaded once at startup, shared across all request handlers.
pub static SKILL_ENGINE: once_cell::sync::Lazy<RwLock<Option<SkillEngine>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(None));

/// Global fashion DB store — initialised in main, used by detached executor.
pub static FASHION_STORE: once_cell::sync::OnceCell<FashionStore> =
    once_cell::sync::OnceCell::new();

pub struct SkillEngine {
    pub loader: SkillLoader,
    pub registry: ToolRegistry,
}

// ── Public API ────────────────────────────────────────────────────────────────

impl SkillEngine {
    /// Initialise from the `skills/` directory relative to the project root.
    pub fn load_from_dir(skills_dir: &std::path::Path) -> anyhow::Result<Self> {
        let mut loader = SkillLoader::new();
        loader.scan(skills_dir)?;

        let mut registry = ToolRegistry::default();
        for skill in loader.list() {
            for tool in &skill.tools {
                registry.register(skill.id.clone(), tool.clone());
            }
        }

        Ok(Self { loader, registry })
    }

    /// Serialised list of all skills for GET /internal/skills/list.
    pub fn list_skills(&self) -> Vec<SkillSummary> {
        self.loader
            .list()
            .into_iter()
            .map(|m| SkillSummary {
                id: m.id.clone(),
                name: m.name.clone(),
                version: m.version.clone(),
                description: m.description.clone(),
                tools: m.tools.iter().map(|t| t.name.clone()).collect(),
            })
            .collect()
    }

    /// Get the raw metadata for one skill.
    pub fn get_skill(&self, skill_id: &str) -> Option<&SkillMetadata> {
        self.loader.get(skill_id)
    }
}

/// Lightweight summary returned by the list endpoint.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub tools: Vec<String>,
}

/// Skill metadata — mirrors the YAML frontmatter fields we care about.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub long_description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    pub tools: Vec<ToolSpec>,
}

/// A tool defined inside a SKILL.md `tools:` list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub parameters: Vec<ParameterSpec>,
    #[serde(default)]
    pub returns: Option<ReturnsSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterSpec {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub enum_values: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReturnsSpec {
    #[serde(rename = "type")]
    pub return_type: String,
    #[serde(default)]
    pub description: Option<String>,
}
