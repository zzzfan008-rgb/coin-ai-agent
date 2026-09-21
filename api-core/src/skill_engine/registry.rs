//! Tool registry — maps `(skill_id, tool_name)` to `ToolSpec` and OpenAI `ToolDefinition`.
//!
//! The registry is built at startup from the loaded skills and is used by two callers:
//! 1. `GET /internal/skills/list` — to serialise the full tool list
//! 2. Agent Loop — to convert skill tool calls into LLM `ToolDefinition` schemas

use std::collections::HashMap;

use crate::llm::tools::ToolDefinition;
use crate::skill_engine::{SkillMetadata, ToolSpec};

/// Global tool registry. Maps `(skill_id, tool_name)` → `ToolSpec`.
#[derive(Debug, Default)]
pub struct ToolRegistry {
    /// key: `${skill_id}:${tool_name}`, value: ToolSpec
    by_id: HashMap<String, ToolSpec>,
    /// key: `tool_name` (unique within a skill scope), value: skill_id
    by_tool_name: HashMap<String, String>,
}

impl ToolRegistry {
    /// Register a single tool under a skill.
    pub fn register(&mut self, skill_id: String, tool: ToolSpec) {
        let key = format!("{skill_id}:{}", tool.name);
        self.by_id.insert(key, tool.clone());
        self.by_tool_name.insert(tool.name.clone(), skill_id);
    }

    /// Get tool spec by skill_id and tool_name.
    pub fn get(&self, skill_id: &str, tool_name: &str) -> Option<&ToolSpec> {
        let key = format!("{skill_id}:{tool_name}");
        self.by_id.get(&key)
    }

    /// Get tool spec by the unqualified tool name only (assumes name is unique).
    pub fn get_by_name(&self, tool_name: &str) -> Option<(&str, &ToolSpec)> {
        self.by_tool_name
            .get(tool_name)
            .and_then(|sid| self.by_id.get(&format!("{sid}:{tool_name}")).map(|t| (sid.as_str(), t)))
    }

    /// Returns all tool names.
    pub fn tool_names(&self) -> Vec<String> {
        self.by_tool_name.keys().cloned().collect()
    }

    /// Build OpenAI `ToolDefinition` list from all registered tools.
    pub fn to_llm_tools(&self) -> Vec<ToolDefinition> {
        self.by_id.values().map(|t| tool_spec_to_definition(&t.name, t)).collect()
    }

    /// Build tool definitions only for the given skill ids, with fully
    /// qualified names `skill_<skill_snake>_<tool>` so the Agent Loop can
    /// dispatch them (`tool/mod.rs` keys off the `skill_` prefix).
    /// Unknown skill ids are silently skipped.
    pub fn to_llm_tools_for_skills(&self, skill_ids: &[String]) -> Vec<ToolDefinition> {
        let mut out = Vec::new();
        for sid in skill_ids {
            let prefix = format!("{sid}:");
            for (key, spec) in &self.by_id {
                if key.starts_with(&prefix) {
                    let qualified = format!("skill_{}_{}", sid.replace('-', "_"), spec.name);
                    out.push(tool_spec_to_definition(&qualified, spec));
                }
            }
        }
        out
    }

    /// Build OpenAI `ToolDefinition` for a single skill.
    pub fn to_llm_tools_for_skill(&self, _skill_id: &str, skill: &SkillMetadata) -> Vec<ToolDefinition> {
        skill
            .tools
            .iter()
            .map(|t| tool_spec_to_definition(&t.name, t))
            .collect()
    }
}

/// Convert a `ToolSpec` (our internal format) into an OpenAI `ToolDefinition`
/// exposed under `name` (bare or fully-qualified).
fn tool_spec_to_definition(name: &str, spec: &ToolSpec) -> ToolDefinition {
    use crate::llm::tools::JsonSchema;

    let properties: serde_json::Map<String, serde_json::Value> = spec
        .parameters
        .iter()
        .map(|p| {
            let mut obj = serde_json::Map::new();
            obj.insert("type".to_string(), serde_json::Value::String(p.param_type.clone()));
            if let Some(ref desc) = p.description {
                obj.insert("description".to_string(), serde_json::Value::String(desc.clone()));
            }
            if let Some(ref defaults) = p.default {
                obj.insert("default".to_string(), defaults.clone());
            }
            if let Some(ref enum_vals) = p.enum_values {
                obj.insert(
                    "enum".to_string(),
                    serde_json::Value::Array(
                        enum_vals.iter().map(|v| serde_json::Value::String(v.clone())).collect(),
                    ),
                );
            }
            (p.name.clone(), serde_json::Value::Object(obj))
        })
        .collect();

    let required: Vec<String> = spec
        .parameters
        .iter()
        .filter(|p| p.required)
        .map(|p| p.name.clone())
        .collect();

    ToolDefinition::new(
        name,
        &spec.description,
        JsonSchema {
            type_field: "object".to_string(),
            properties: Some(properties),
            required,
        },
    )
}
