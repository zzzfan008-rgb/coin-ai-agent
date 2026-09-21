//! Intent classification layer (T-014).
//!
//! 规则关键词匹配：快、可解释。规则从 `rules.yaml` 加载并支持热加载
//! （每次 `classify` 前检查文件 mtime，变更后自动重读，无需重启）。
//!
//! 预留 LLM 降级路径：当规则无法命中（落到 general）时，可配置
//! [`LlmIntentFallback`] 走 LLM 分类。Phase 2 实现，当前仅提供
//! [`StubLlmIntentClassifier`] stub。

use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// 内置默认规则，rules.yaml 缺失时作为兜底（也保证单测不依赖工作目录）。
const EMBEDDED_RULES: &str = include_str!("rules.yaml");

/// 单条关键词 → 意图规则，对应 rules.yaml 中的一项。
#[derive(Debug, Clone, Deserialize)]
pub struct IntentRule {
    pub intent: String,
    #[serde(default)]
    pub skill: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

/// 一个可能的路由候选（多候选：意图模糊时返回多个可能 skill）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IntentCandidate {
    pub intent: String,
    pub skill_id: Option<String>,
    pub confidence: f32,
    pub matched_keywords: Vec<String>,
}

/// 分类结果。`intent`/`skill_id`/`confidence` 描述主候选，
/// `candidates` 按得分降序包含全部非 general 候选。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IntentResult {
    /// fabric / color / style / general
    pub intent: String,
    pub skill_id: Option<String>,
    pub matched_keywords: Vec<String>,
    pub confidence: f32,
    pub candidates: Vec<IntentCandidate>,
    /// 分类来源："rule" | "fallback"
    pub source: String,
}

struct Inner {
    rules: Vec<IntentRule>,
    mtime: Option<SystemTime>,
}

/// 意图路由器。线程安全（内部 RwLock），可跨请求共享。
pub struct IntentRouter {
    path: Option<PathBuf>,
    inner: RwLock<Inner>,
}

impl IntentRouter {
    /// 从 YAML 文件加载规则，并记录其路径/mtime 用于热加载。
    pub fn load_rules(path: &str) -> anyhow::Result<Self> {
        let p = PathBuf::from(path);
        let text = std::fs::read_to_string(&p)?;
        let rules = parse_rules(&text)?;
        let mtime = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
        tracing::info!(path, n = rules.len(), "Intent rules loaded");
        Ok(Self {
            path: Some(p),
            inner: RwLock::new(Inner { rules, mtime }),
        })
    }

    /// 使用编译期内置规则（文件缺失时的兜底）。
    pub fn embedded() -> anyhow::Result<Self> {
        let rules = parse_rules(EMBEDDED_RULES)?;
        tracing::info!(n = rules.len(), "Using embedded intent rules");
        Ok(Self {
            path: None,
            inner: RwLock::new(Inner { rules, mtime: None }),
        })
    }

    /// 文件存在则从文件加载（支持热加载），否则用内置规则。
    pub fn load_or_embedded(path: &Path) -> anyhow::Result<Self> {
        if path.exists() {
            Self::load_rules(&path.to_string_lossy())
        } else {
            tracing::warn!("Intent rules file not found at {:?}, falling back to embedded rules", path);
            Self::embedded()
        }
    }

    /// 规则关键词分类。纯字符串匹配（微秒级，P99 远低于 200ms）。
    pub fn classify(&self, message: &str) -> IntentResult {
        if let Some(result) = self.maybe_reload_and_classify(message) {
            return result;
        }
        // 热加载检查失败时退回已有规则。
        self.classify_locked(message)
    }

    fn maybe_reload_and_classify(&self, message: &str) -> Option<IntentResult> {
        // 热加载：mtime 变化则重读文件。stat 一次调用开销在微秒级。
        if let Some(path) = &self.path {
            let new_mtime = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
            let needs_reload = {
                let inner = self.inner.read().ok()?;
                new_mtime != inner.mtime
            };
            if needs_reload {
                if let Ok(text) = std::fs::read_to_string(path) {
                    if let Ok(rules) = parse_rules(&text) {
                        if let Ok(mut inner) = self.inner.write() {
                            inner.rules = rules;
                            inner.mtime = new_mtime;
                            tracing::info!(?path, "Intent rules hot-reloaded");
                        }
                    }
                }
            }
        }
        Some(self.classify_locked(message))
    }

    fn classify_locked(&self, message: &str) -> IntentResult {
        let inner = self.inner.read().expect("intent inner poisoned");
        let haystack = message.to_lowercase();

        // 命中打分（跳过 general / 空关键词规则）。
        let mut scored: Vec<(usize, &IntentRule, Vec<String>)> = inner
            .rules
            .iter()
            .filter(|r| !r.keywords.is_empty())
            .filter_map(|r| {
                let hits: Vec<String> = r
                    .keywords
                    .iter()
                    .filter(|kw| !kw.is_empty() && haystack.contains(&kw.to_lowercase()))
                    .cloned()
                    .collect();
                if hits.is_empty() { None } else { Some((hits.len(), r, hits)) }
            })
            .collect();

        // 命中数降序；平分时按 rules.yaml 中的原始顺序（数组已按顺序解析）。
        scored.sort_by(|a, b| b.0.cmp(&a.0));

        if scored.is_empty() {
            return IntentResult {
                intent: "general".into(),
                skill_id: None,
                matched_keywords: vec![],
                confidence: 0.3,
                candidates: vec![],
                source: "rule".into(),
            };
        }

        let top_score = scored[0].0 as f32;
        let mut candidates: Vec<IntentCandidate> = Vec::new();
        for (score, rule, hits) in &scored {
            // 次优候选得分达到 top 的 50% 即视为模糊，保留为多候选。
            if (*score as f32) < top_score * 0.5 {
                break;
            }
            candidates.push(IntentCandidate {
                intent: rule.intent.clone(),
                skill_id: rule.skill.clone(),
                confidence: confidence_for(*score),
                matched_keywords: hits.clone(),
            });
        }

        let primary = candidates.remove(0);
        IntentResult {
            intent: primary.intent,
            skill_id: primary.skill_id,
            matched_keywords: primary.matched_keywords,
            confidence: primary.confidence,
            candidates,
            source: "rule".into(),
        }
    }

    /// 规则落到 general 时尝试 LLM 降级分类（Phase 2 启用）。
    pub async fn classify_with_fallback(
        &self,
        message: &str,
        fallback: Option<&dyn LlmIntentFallback>,
    ) -> IntentResult {
        let result = self.classify(message);
        if result.intent != "general" {
            return result;
        }
        if let Some(fb) = fallback {
            match fb.classify(message).await {
                Ok(mut r) => {
                    r.source = "fallback".into();
                    return r;
                }
                Err(e) => tracing::warn!("LLM intent fallback failed: {e}"),
            }
        }
        result
    }
}

/// 置信度：单关键词命中 0.6，每多一个命中 +0.15，封顶 0.95。
fn confidence_for(matched: usize) -> f32 {
    (0.6 + 0.15 * (matched.saturating_sub(1)) as f32).min(0.95)
}

/// 解析 rules.yaml，并保证存在 general 兜底规则。
pub fn parse_rules(text: &str) -> anyhow::Result<Vec<IntentRule>> {
    let mut rules: Vec<IntentRule> = serde_yaml::from_str(text)?;
    for r in &mut rules {
        r.keywords.sort();
        r.keywords.dedup();
    }
    if !rules.iter().any(|r| r.intent == "general") {
        rules.push(IntentRule {
            intent: "general".into(),
            skill: None,
            keywords: vec![],
        });
    }
    Ok(rules)
}

// ── LLM 降级预留（Phase 2 实现） ─────────────────────────────────────────────

/// LLM 意图分类降级接口。规则无法命中时可配置走 LLM。
/// 使用显式 boxed future 以便 `dyn LlmIntentFallback` 可在运行时分发。
pub trait LlmIntentFallback: Send + Sync {
    fn classify<'a>(
        &'a self,
        message: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<IntentResult>> + Send + 'a>,
    >;
}

/// Stub：Phase 2 接入真实 LLM 前始终报错，保证主流程不受影响。
pub struct StubLlmIntentClassifier;

impl LlmIntentFallback for StubLlmIntentClassifier {
    fn classify<'a>(
        &'a self,
        _message: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<IntentResult>> + Send + 'a>,
    > {
        Box::pin(async move {
            Err(anyhow::anyhow!(
                "LLM intent fallback is not implemented yet (Phase 2)"
            ))
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn router() -> IntentRouter {
        IntentRouter::embedded().expect("embedded rules parse")
    }

    #[test]
    fn routes_fabric_question() {
        let r = router().classify("这件衬衫用什么面料好");
        assert_eq!(r.intent, "fabric");
        assert_eq!(r.skill_id.as_deref(), Some("fabric-query"));
        assert!(r.matched_keywords.contains(&"面料".to_string()));
        // 衬衫同时命中 style → 模糊场景下应作为次优候选
        assert!(r.candidates.iter().any(|c| c.intent == "style"));
    }

    #[test]
    fn routes_color_question() {
        let r = router().classify("蓝色配什么颜色好看");
        assert_eq!(r.intent, "color");
        assert_eq!(r.skill_id.as_deref(), Some("color-matching"));
        assert!(r.matched_keywords.iter().any(|k| k.contains("颜色") || k == "色"));
    }

    #[test]
    fn routes_style_question() {
        let r = router().classify("给我一些休闲款式的灵感");
        assert_eq!(r.intent, "style");
        assert_eq!(r.skill_id.as_deref(), Some("style-inspiration"));
        assert!(r.matched_keywords.contains(&"款式".to_string()));
        assert!(r.matched_keywords.contains(&"灵感".to_string()));
    }

    #[test]
    fn routes_greeting_to_general() {
        let r = router().classify("你好");
        assert_eq!(r.intent, "general");
        assert!(r.skill_id.is_none());
        assert!(r.matched_keywords.is_empty());
    }

    #[test]
    fn english_keywords_match_case_insensitively() {
        let r = router().classify("Which FABRIC is more breathable?");
        assert_eq!(r.intent, "fabric");
    }

    #[test]
    fn confidence_grows_with_matches() {
        assert_eq!(confidence_for(1), 0.6);
        assert!(confidence_for(2) > confidence_for(1));
        assert!(confidence_for(100) <= 0.95);
    }

    #[test]
    fn parse_rules_adds_general_fallback() {
        let rules = parse_rules(
            "- intent: x\n  skill: x-skill\n  keywords: [foo]\n",
        )
        .unwrap();
        assert!(rules.iter().any(|r| r.intent == "general"));
    }

    #[test]
    fn classify_is_well_under_200ms() {
        let r = router();
        let msg = "这件衬衫用什么面料好，蓝色配什么颜色好看，给我一些休闲款式的灵感";
        // P99 < 200ms: assert a loose 10ms upper bound (real runs are ~1-10µs).
        for _ in 0..1000 {
            let started = std::time::Instant::now();
            let _ = r.classify(msg);
            assert!(started.elapsed().as_millis() < 10);
        }
    }

    #[tokio::test]
    async fn fallback_stub_keeps_general() {
        let r = router();
        let out = r
            .classify_with_fallback("你好", Some(&StubLlmIntentClassifier))
            .await;
        assert_eq!(out.intent, "general");
        assert_eq!(out.source, "rule"); // stub 报错，保持规则结果
    }
}
