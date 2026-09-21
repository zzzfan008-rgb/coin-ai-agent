//! SKILL.md loader — parses YAML frontmatter, resolves dependencies.
//!
//! Naming convention: each skill lives in `<skills_dir>/<skill_id>/SKILL.md`.
//! The loader ignores subdirectories without a SKILL.md file.

use std::path::Path;

use crate::skill_engine::SkillMetadata;

/// Scans a directory tree and builds a registry of loaded skill metadata.
#[derive(Debug, Default)]
pub struct SkillLoader {
    skills: Vec<SkillMetadata>,
}

impl SkillLoader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Scan `dir` for skill subdirectories and parse their SKILL.md files.
    pub fn scan(&mut self, dir: &Path) -> anyhow::Result<()> {
        self.skills.clear();

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
            match Self::load_file(&skill_md) {
                Ok(meta) => {
                    tracing::info!(skill_id = %meta.id, "Loaded skill from {:?}", skill_md);
                    self.skills.push(meta);
                }
                Err(e) => {
                    tracing::warn!("Failed to load {:?}: {e}", skill_md);
                }
            }
        }

        // Sort by skill id for deterministic ordering
        self.skills.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(())
    }

    fn load_file(path: &Path) -> anyhow::Result<SkillMetadata> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content)
    }

    /// Parse YAML frontmatter from SKILL.md content.
    /// Format: `---\n<yaml>\n---\n<markdown body>`
    pub fn parse(content: &str) -> anyhow::Result<SkillMetadata> {
        let content = content.trim();
        if !content.starts_with("---") {
            return Err(anyhow::anyhow!(
                "SKILL.md must start with YAML frontmatter (---)"
            ));
        }

        // Find the closing `---` of the frontmatter block.
        // The content after `---` starts the YAML block.
        let after_first = &content[3..];
        let end_idx = after_first
            .find("---")
            .ok_or_else(|| anyhow::anyhow!("Missing closing --- in SKILL.md frontmatter"))?;

        let yaml_str = &after_first[..end_idx];

        let mut meta: SkillMetadata = serde_yaml::from_str(yaml_str)
            .map_err(|e| anyhow::anyhow!("Invalid SKILL.md frontmatter: {e}"))?;

        // Normalise: SKILL.md frontmatter uses `name` as skill name, map to `id`.
        if meta.id.is_empty() || meta.id == "unknown" {
            meta.id = meta.name.clone();
        }

        Ok(meta)
    }

    pub fn list(&self) -> Vec<&SkillMetadata> {
        self.skills.iter().collect()
    }

    pub fn get(&self, skill_id: &str) -> Option<&SkillMetadata> {
        self.skills.iter().find(|s| s.id == skill_id)
    }

    pub fn skill_ids(&self) -> Vec<String> {
        self.skills.iter().map(|s| s.id.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fabric_query() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let content = std::fs::read_to_string(format!(
            "{}/../skills/fabric-query/SKILL.md",
            manifest_dir
        ))
        .unwrap();
        let meta = SkillLoader::parse(&content).unwrap();
        assert_eq!(meta.id, "fabric-query");
        assert_eq!(meta.tools.len(), 3);
        assert!(meta.tools.iter().any(|t| t.name == "search_fabric"));
    }

    #[test]
    fn test_parse_color_matching() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let content = std::fs::read_to_string(format!(
            "{}/../skills/color-matching/SKILL.md",
            manifest_dir
        ))
        .unwrap();
        let meta = SkillLoader::parse(&content).unwrap();
        assert_eq!(meta.id, "color-matching");
        assert_eq!(meta.tools.len(), 3);
    }

    #[test]
    fn test_parse_style_inspiration() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let content = std::fs::read_to_string(format!(
            "{}/../skills/style-inspiration/SKILL.md",
            manifest_dir
        ))
        .unwrap();
        let meta = SkillLoader::parse(&content).unwrap();
        assert_eq!(meta.id, "style-inspiration");
        assert_eq!(meta.tools.len(), 3);
    }
}
