//! Skill executor — runs skill tool logic (Phase 1B: mock data).
//!
//! Execution path:
//!   route_tool_call() ← called from Agent Loop (tool/mod.rs dispatches skill_* here)
//!     → check_skill_permission()  ← inline permission gate
//!     → execute_skill()            ← mock / run.py dispatch
//!
//! The `SKILL_ENGINE` global must be initialised before this module is used.

use serde_json::Value;

use crate::api::handlers::UserContext;
use crate::error::{AppError, Result};
use crate::skill_engine::{SKILL_ENGINE, SkillMetadata, check_skill_permission};

/// Route an LLM tool_call to the correct skill executor.
///
/// Tool naming convention used by the LLM:
///   `<skill-id-kebab>_<tool-name-snake>`  e.g. `fabric-query_search_fabric`
/// We convert kebab → snake for parsing, then look up the skill.
pub async fn route_tool_call(
    tool_name: &str,
    arguments: &Value,
    user_ctx: &UserContext,
) -> Result<String> {
    // Resolve and clone everything we need while holding the read lock,
    // then drop it BEFORE any await (std RwLockReadGuard is !Send).
    let (skill_id, resolved_tool, skill_meta) = {
        let engine_guard = SKILL_ENGINE
            .read()
            .map_err(|_| AppError::Internal("SKILL_ENGINE not initialised".into()))?;
        let engine = engine_guard.as_ref().ok_or_else(|| {
            AppError::Internal(
                "SKILL_ENGINE not initialised — call SkillEngine::load_from_dir()".into(),
            )
        })?;

        // Build a kebab-case → skill_id lookup map for tool name resolution.
        let skill_id_lookup: std::collections::HashMap<String, String> = engine
            .loader
            .skill_ids()
            .iter()
            .map(|id| {
                // kebab to snake for matching LLM output
                let snake = id.replace('-', "_");
                (snake, id.clone())
            })
            .collect();

        // Build a (skill_id, tool_name) → true lookup for fully-qualified names.
        let full_name_lookup: std::collections::HashMap<String, (String, String)> = engine
            .loader
            .list()
            .into_iter()
            .flat_map(|m| {
                m.tools
                    .iter()
                    .map(|t| {
                        let skill_snake = m.id.replace('-', "_");
                        let key = format!("{}_{}", skill_snake, t.name);
                        (key, (m.id.clone(), t.name.clone()))
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        let (skill_id, resolved_tool) =
            resolve_tool_name(tool_name, &skill_id_lookup, &full_name_lookup).map_err(|_| {
                AppError::NotFound(format!("Could not resolve tool name: {tool_name}"))
            })?;

        let meta = engine
            .get_skill(&skill_id)
            .ok_or_else(|| AppError::NotFound(format!("Skill '{skill_id}' not found")))?
            .clone();

        (skill_id, resolved_tool, meta)
    }; // read guard dropped here

    // Permission gate.
    check_skill_permission(user_ctx, &skill_meta)?;

    let result = execute_skill(&skill_meta, &resolved_tool, arguments).await?;
    Ok(serde_json::json!({
        "status": "ok",
        "skill": skill_id,
        "tool": resolved_tool,
        "result": result,
    })
    .to_string())
}

/// Resolve a tool_name string to (skill_id, tool_name).
///
/// Supports:
///   `fabric_query_search_fabric`  → ("fabric-query", "search_fabric")
///   `style_inspiration_trend_analysis` → ("style-inspiration", "trend_analysis")
///   `search_fabric`             → tries to find by tool name; falls back to first skill with that tool
///   `skill_fabric_query_search_fabric` → strips prefix then same as first case
fn resolve_tool_name(
    tool_name: &str,
    skill_id_lookup: &std::collections::HashMap<String, String>,
    full_name_lookup: &std::collections::HashMap<String, (String, String)>,
) -> std::result::Result<(String, String), String> {
    let name = tool_name
        .trim_start_matches("skill_")
        .trim_start_matches("mcp_");

    // Try exact full-name match first.
    if let Some((sid, tname)) = full_name_lookup.get(name) {
        return Ok((sid.clone(), tname.clone()));
    }

    // Try longest-prefix skill match.
    let parts: Vec<&str> = name.split('_').collect();
    if parts.len() >= 2 {
        for end in (1..parts.len()).rev() {
            let skill_snake = parts[..end].join("_");
            if let Some(skill_id) = skill_id_lookup.get(&skill_snake) {
                let tool_name_out = parts[end..].join("_");
                return Ok((skill_id.clone(), tool_name_out));
            }
        }
    }

    Err(name.to_string())
}

/// Execute a skill tool and return structured JSON (Phase 1B mock).
pub async fn execute_skill(
    skill: &SkillMetadata,
    tool_name: &str,
    arguments: &Value,
) -> Result<Value> {
    let tool = skill
        .tools
        .iter()
        .find(|t| t.name == tool_name)
        .ok_or_else(|| AppError::NotFound(format!("Tool '{tool_name}' not found in skill '{}'", skill.id)))?;

    tracing::info!(
        skill = %skill.id,
        tool = %tool.name,
        "Executing skill tool (Phase 1B mock)"
    );

    // ── Mock responses per skill+tool ──────────────────────────────────────────
    let result = match (skill.id.as_str(), tool.name.as_str()) {
        // ── fabric-query ────────────────────────────────────────────────────────
        ("fabric-query", "search_fabric") => {
            let query = arguments.get("query").and_then(|v| v.as_str()).unwrap_or("");
            serde_json::json!({
                "fabrics": [
                    {"id": "fab_001", "name": "100% 纯棉帆布", "composition": {"cotton": 100}, "weight": "250-300g/m²",
                     "seasons": ["spring", "summer", "autumn"], "applicable_styles": ["casual", "streetwear"],
                     "care": {"washing": "冷水机洗", "drying": "阴凉晾干"}},
                    {"id": "fab_002", "name": "棉涤混纺府绸", "composition": {"cotton": 65, "polyester": 35}, "weight": "120-150g/m²",
                     "seasons": ["spring", "summer"], "applicable_styles": ["shirt", "dress", "formal"],
                     "care": {"washing": "温水机洗", "drying": "悬挂晾干"}},
                    {"id": "fab_003", "name": "高支数桑蚕丝", "composition": {"silk": 100}, "weight": "20-30g/m²",
                     "seasons": ["spring", "summer", "autumn"], "applicable_styles": ["luxury", "dress", "evening"],
                     "care": {"washing": "干洗", "drying": "平铺阴干"}}
                ],
                "query": query, "total": 3, "note": "Mock data — Phase 1B"
            })
        }
        ("fabric-query", "filter_by_season") => {
            let season = arguments.get("season").and_then(|v| v.as_str()).unwrap_or("all-season");
            let all = [
                ("spring", "fab_001", "100% 纯棉帆布"), ("spring", "fab_002", "棉涤混纺府绸"),
                ("summer", "fab_001", "100% 纯棉帆布"), ("summer", "fab_002", "棉涤混纺府绸"),
                ("summer", "fab_003", "高支数桑蚕丝"),
                ("autumn", "fab_001", "100% 纯棉帆布"), ("autumn", "fab_003", "高支数桑蚕丝"),
                ("winter", "fab_004", "羊毛呢料"), ("winter", "fab_005", "羊绒双面呢"),
                ("all-season", "fab_001", "100% 纯棉帆布"), ("all-season", "fab_006", "亚麻棉混纺"),
            ];
            let filtered: Vec<_> = all
                .iter()
                .filter(|(s, _, _)| *s == season || season == "all-season")
                .map(|(s, id, name)| serde_json::json!({ "id": id, "name": name, "season": s }))
                .collect();
            serde_json::json!({ "season": season, "fabrics": filtered, "total": filtered.len(), "note": "Mock data — Phase 1B" })
        }
        ("fabric-query", "get_applicable_styles") => {
            let fabric_id = arguments.get("fabric_id").and_then(|v| v.as_str()).unwrap_or("");
            let styles = match fabric_id {
                "fab_001" => vec!["休闲外套", "工装裤", "背包", "帽子"],
                "fab_002" => vec!["衬衫", "连衣裙", "正装裤", "校服"],
                "fab_003" => vec!["礼服", "晚装", "丝巾", "睡衣"],
                "fab_004" => vec!["大衣", "西装外套", "风衣"],
                _ => vec!["通用款式"],
            };
            serde_json::json!({ "fabric_id": fabric_id, "applicable_styles": styles, "recommendation": "推荐基于面料特性的款式设计", "note": "Mock data — Phase 1B" })
        }

        // ── color-matching ──────────────────────────────────────────────────────
        ("color-matching", "suggest_palette") => {
            let primary = arguments.get("primary_color").and_then(|v| v.as_str()).unwrap_or("#6366F1");
            let palette_type = arguments.get("palette_type").and_then(|v| v.as_str()).unwrap_or("complementary");
            let palettes = serde_json::json!({
                "complementary": {
                    "colors": [
                        {"hex": primary, "name": "主色", "role": "primary"},
                        {"hex": "#EF4444", "name": "互补色", "role": "accent"},
                        {"hex": "#F5F5F5", "name": "中性浅", "role": "background"},
                        {"hex": "#1F2937", "name": "中性深", "role": "text"}
                    ]
                },
                "analogous": {
                    "colors": [
                        {"hex": "#4F46E5", "name": "邻近色1", "role": "secondary"},
                        {"hex": primary, "name": "主色", "role": "primary"},
                        {"hex": "#818CF8", "name": "邻近色2", "role": "tertiary"}
                    ]
                },
                "triadic": {
                    "colors": [
                        {"hex": primary, "name": "主色", "role": "primary"},
                        {"hex": "#10B981", "name": "三色1", "role": "secondary"},
                        {"hex": "#F59E0B", "name": "三色2", "role": "accent"}
                    ]
                }
            });
            serde_json::json!({
                "palette_type": palette_type,
                "palette": palettes.get(palette_type).cloned().unwrap_or_else(|| palettes.get("complementary").unwrap().clone()),
                "harmony_score": 82, "usage_tips": "适合时尚前卫风格，建议搭配黑白色系作为过渡",
                "note": "Mock data — Phase 1B"
            })
        }
        ("color-matching", "color_harmony") => {
            let colors: Vec<String> = arguments
                .get("colors")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(String::from).collect())
                .unwrap_or_else(|| vec!["#6366F1".to_string(), "#EF4444".to_string()]);
            serde_json::json!({
                "colors": colors,
                "harmony_score": 78,
                "analysis": {
                    "hue_balance": "良好 — 色相环分布均匀",
                    "saturation_contrast": "适中",
                    "lightness_range": "对比度 45% — 层次丰富"
                },
                "suggestions": ["可适当提高明度以增加透气感", "建议加入中性色作为过渡"],
                "overall": "该配色方案和谐度较好，适合时尚风格"
            })
        }
        ("color-matching", "trend_colors") => {
            let season = arguments.get("season").and_then(|v| v.as_str()).unwrap_or("spring");
            let category = arguments.get("category").and_then(|v| v.as_str()).unwrap_or("apparel");
            serde_json::json!({
                "season": season, "category": category,
                "trends": [
                    {"hex": "#8B5CF6", "name": "极光紫", "hot": "high", "style": "y2k复兴"},
                    {"hex": "#10B981", "name": "鼠尾草绿", "hot": "high", "style": "自然极简"},
                    {"hex": "#F59E0B", "name": "琥珀金", "hot": "medium", "style": "复古华丽"},
                    {"hex": "#EC4899", "name": "玫瑰粉", "hot": "medium", "style": "柔美浪漫"},
                    {"hex": "#1E293B", "name": "石墨灰", "hot": "high", "style": "都市商务"}
                ],
                "note": "Mock data — Phase 1B"
            })
        }

        // ── style-inspiration ─────────────────────────────────────────────────
        ("style-inspiration", "generate_style_ideas") => {
            let keywords: Vec<String> = arguments
                .get("keywords")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(String::from).collect())
                .unwrap_or_else(|| vec!["休闲".to_string(), "春夏".to_string()]);
            let garment_type = arguments.get("garment_type").and_then(|v| v.as_str()).unwrap_or("general");
            serde_json::json!({
                "keywords": keywords, "garment_type": garment_type,
                "ideas": [
                    {"id": "insp_001", "title": "都市轻通勤穿搭", "description": "结合功能性与都市感的日常穿搭，简约利落又不失个性",
                     "silhouette": "H型", "key_features": ["无领衬衫", "高腰直筒裤", "乐福鞋"],
                     "colors": ["#1E293B", "#F5F5F5", "#8B5CF6"], "suitable_seasons": ["spring", "autumn"]},
                    {"id": "insp_002", "title": "户外运动风格", "description": "强调舒适与功能性，适合户外活动的穿搭方案",
                     "silhouette": "O型", "key_features": ["宽松连帽衫", "工装短裤", "运动鞋"],
                     "colors": ["#10B981", "#1E293B", "#F5F5F5"], "suitable_seasons": ["spring", "summer"]},
                    {"id": "insp_003", "title": "复古文艺风格", "description": "融合复古元素与现代剪裁，展现文艺气质",
                     "silhouette": "A型", "key_features": ["泡泡袖衬衫", "百褶裙", "玛丽珍鞋"],
                     "colors": ["#F59E0B", "#8B5CF6", "#EC4899"], "suitable_seasons": ["spring", "summer"]}
                ],
                "note": "Mock data — Phase 1B"
            })
        }
        ("style-inspiration", "style_variations") => {
            let base_style = arguments.get("base_style").and_then(|v| v.as_str()).unwrap_or("西装外套");
            let variation_type = arguments.get("variation_type").and_then(|v| v.as_str()).unwrap_or("all");
            serde_json::json!({
                "base_style": base_style, "variation_type": variation_type,
                "variations": [
                    {"id": "var_001", "title": "休闲版西装外套", "description": "保留西装轮廓，降低正式感，增加日常可穿搭性",
                     "changes": ["面料换成亚麻", "取消内衬", "口袋改为贴袋"]},
                    {"id": "var_002", "title": "oversized西装外套", "description": "放大廓形，强调慵懒随性的街头风格",
                     "changes": ["肩线外扩2-3cm", "袖口加宽", "长度加长至臀部"]},
                    {"id": "var_003", "title": "无领西装外套", "description": "去掉传统翻领，简化设计，更适合内搭",
                     "changes": ["取消翻领", "门襟改为暗扣", "领口加深"]}
                ],
                "note": "Mock data — Phase 1B"
            })
        }
        ("style-inspiration", "trend_analysis") => {
            let category = arguments.get("category").and_then(|v| v.as_str()).unwrap_or("all");
            let region = arguments.get("region").and_then(|v| v.as_str()).unwrap_or("global");
            serde_json::json!({
                "category": category, "region": region,
                "trends": [
                    {"id": "trend_001", "name": "柔软结构主义", "description": "在保持设计感的同时追求穿着舒适度，轮廓柔和但不松垮",
                     "heat_index": 92, "key_elements": ["圆润肩线", "垂褶细节", "弹性面料"]},
                    {"id": "trend_002", "name": "可持续时尚", "description": "环保面料与循环设计理念成为主流趋势",
                     "heat_index": 88, "key_elements": ["再生面料", "模块化设计", "天然染色"]},
                    {"id": "trend_003", "name": "数字美学", "description": "数字化设计语言与虚拟时装的灵感融合",
                     "heat_index": 75, "key_elements": ["几何图案", "霓虹色调", "科技面料"]}
                ],
                "note": "Mock data — Phase 1B"
            })
        }

        _ => {
            return Err(AppError::NotFound(format!(
                "Unknown skill/tool: '{}/{}'",
                skill.id, tool.name
            )));
        }
    };

    Ok(result)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_skill_id_lookup() {
        // Simulate the lookup map
        let skill_ids = vec!["fabric-query".to_string(), "color-matching".to_string(), "style-inspiration".to_string()];
        let lookup: std::collections::HashMap<String, String> = skill_ids
            .iter()
            .map(|id| (id.replace('-', "_"), id.clone()))
            .collect();
        assert_eq!(lookup.get("fabric_query"), Some(&"fabric-query".to_string()));
        assert_eq!(lookup.get("color_matching"), Some(&"color-matching".to_string()));
        assert_eq!(lookup.get("style_inspiration"), Some(&"style-inspiration".to_string()));
    }
}
