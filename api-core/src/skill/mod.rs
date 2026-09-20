//! Skill Loader — discovers and executes skills from the skills/ directory.
//!
//! Each skill is a directory containing a SKILL.md (metadata) and optional
//! run.py (executable). Skills are loaded at startup and registered with
//! the tool engine.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Skill metadata loaded from SKILL.md frontmatter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub tools: Vec<ToolSpec>,
    pub prompt_template: Option<String>,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub parameters: serde_json::Value,
}

/// Skill registry — all available skills loaded at startup.
pub struct SkillRegistry {
    skills: HashMap<String, SkillMetadata>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
        }
    }

    /// Scan a directory and load all skill SKILL.md files.
    pub fn load_from_dir(&mut self, dir: &Path) -> Result<()> {
        if !dir.exists() {
            tracing::warn!("Skills directory does not exist: {:?}", dir);
            return Ok(());
        }

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_md = path.join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }
            if let Ok(meta) = Self::load_skill(&skill_md) {
                tracing::info!(skill_id = %meta.id, "Loaded skill");
                self.skills.insert(meta.id.clone(), meta);
            }
        }

        Ok(())
    }

    fn load_skill(path: &Path) -> Result<SkillMetadata> {
        let content = std::fs::read_to_string(path)?;
        let meta = Self::parse_frontmatter(&content)?;
        Ok(meta)
    }

    /// Parse YAML frontmatter from SKILL.md content.
    fn parse_frontmatter(content: &str) -> Result<SkillMetadata> {
        let content = content.trim();
        if !content.starts_with("---") {
            return Ok(SkillMetadata {
                id: "unknown".to_string(),
                name: "Unknown Skill".to_string(),
                version: "0.0.0".to_string(),
                description: content.chars().take(200).collect(),
                tools: vec![],
                prompt_template: None,
                permissions: vec![],
            });
        }

        let end = content[3..].find("---").map(|i| i + 3);
        let Some(end_idx) = end else {
            return Ok(SkillMetadata {
                id: "unknown".to_string(),
                name: "Unknown Skill".to_string(),
                version: "0.0.0".to_string(),
                description: content.to_string(),
                tools: vec![],
                prompt_template: None,
                permissions: vec![],
            });
        };

        let yaml_str = &content[3..end_idx];
        let meta: SkillMetadata = serde_yaml::from_str(yaml_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse skill YAML: {e}"))?;
        Ok(meta)
    }

    pub fn get(&self, id: &str) -> Option<&SkillMetadata> {
        self.skills.get(id)
    }

    pub fn list(&self) -> Vec<&SkillMetadata> {
        self.skills.values().collect()
    }

    /// Execute a skill's run.py with given parameters (Phase 1B).
    pub async fn execute(
        &self,
        skill_id: &str,
        tool_name: &str,
        _parameters: &serde_json::Value,
    ) -> Result<String> {
        let _skill = self
            .get(skill_id)
            .ok_or_else(|| anyhow::anyhow!("Skill {skill_id} not found"))?;

        let run_py = Path::new("skills").join(skill_id).join("run.py");
        if !run_py.exists() {
            return Ok(serde_json::json!({
                "status": "stub",
                "skill": skill_id,
                "tool": tool_name,
                "message": "No run.py found — stub response"
            }).to_string());
        }

        // Phase 1B: spawn Python subprocess with parameters
        tracing::info!(skill = %skill_id, tool = %tool_name, "Skill execution (Phase 1B)");
        Ok(serde_json::json!({
            "status": "ok",
            "skill": skill_id,
            "tool": tool_name,
            "message": "Skill execution stub"
        }).to_string())
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}
