//! Pure color math for the `color-matching` skill — no DB / I/O.
//!
//! Provides:
//! - `build_palette(primary_hex, scheme)` — color-wheel palettes
//! - `analyze_harmony(colors)` — harmony scoring for a color set
//!
//! All functions degrade to `None`/defaults on malformed input rather than
//! panicking, so LLM-supplied values can't crash the executor.

use serde::Serialize;

// ── Public result types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct PaletteColor {
    pub hex: String,
    pub name: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ColorPalette {
    pub palette_type: String,
    pub primary_hex: String,
    pub colors: Vec<PaletteColor>,
    pub harmony_score: i32,
    pub usage_tips: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarmonyAnalysis {
    pub hue_balance: String,
    pub saturation_contrast: String,
    pub lightness_range: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarmonyResult {
    pub colors: Vec<String>,
    pub harmony_score: i32,
    pub analysis: HarmonyAnalysis,
    pub suggestions: Vec<String>,
    pub overall: String,
}

// ── Hex / RGB / HSL conversions ───────────────────────────────────────────────

/// Parse "#RRGGBB", "RRGGBB" (also 3-digit "#RGB") into (r, g, b).
pub fn hex_to_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.trim().trim_start_matches('#');
    let h = match h.len() {
        6 => h.to_string(),
        3 => h.chars().flat_map(|c| [c, c]).collect(),
        _ => return None,
    };
    let n = u32::from_str_radix(&h, 16).ok()?;
    Some((((n >> 16) & 255) as u8, ((n >> 8) & 255) as u8, (n & 255) as u8))
}

pub fn rgb_to_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

/// Returns (h: 0..360, s: 0..100, l: 0..100).
pub fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;

    let h = if d.abs() < f64::EPSILON {
        0.0
    } else if (max - r).abs() < f64::EPSILON {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if (max - g).abs() < f64::EPSILON {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };

    let s = if d.abs() < f64::EPSILON {
        0.0
    } else {
        d / (1.0 - (2.0 * l - 1.0).abs())
    };

    (h, s * 100.0, l * 100.0)
}

pub fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let h = h.rem_euclid(360.0);
    let s = (s / 100.0).clamp(0.0, 1.0);
    let l = (l / 100.0).clamp(0.0, 1.0);

    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = match h as i64 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    (
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    )
}

// ── Palette builder ───────────────────────────────────────────────────────────

/// Smallest signed difference between two hues on the 0..360 wheel.
fn hue_delta(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    if d > 180.0 { d - 360.0 } else { d }
}

/// Build a color-wheel palette around `primary_hex`.
/// Falls back to indigo (#6366F1) / complementary when inputs are unusable.
pub fn build_palette(primary_hex: &str, scheme: &str) -> ColorPalette {
    let (r, g, b) = hex_to_rgb(primary_hex).unwrap_or((99, 102, 241));
    let (h, s, l) = rgb_to_hsl(r, g, b);
    let primary = rgb_to_hex(r, g, b);

    let pc = |hue: f64, sat: f64, lit: f64, name: &str, role: &str| PaletteColor {
        hex: {
            let (rr, gg, bb) = hsl_to_rgb(hue, sat, lit);
            rgb_to_hex(rr, gg, bb)
        },
        name: name.to_string(),
        role: role.to_string(),
    };

    let primary_entry = PaletteColor { hex: primary.clone(), name: "主色".into(), role: "primary".into() };

    // (extra colors, score, tips) per scheme
    let (extra, score, tips): (Vec<PaletteColor>, i32, &str) = match scheme {
        "complementary" => (
            vec![pc(h + 180.0, s, l, "互补色", "accent"),
                 pc(h, s * 0.3, 92.0, "中性浅", "background"),
                 pc(h, s * 0.3, 18.0, "中性深", "text")],
            85,
            "强对比搭配，建议用黑、白、灰等中性色作为过渡，主色占比约60%，互补色控制在10%以内作为点缀",
        ),
        "analogous" => (
            vec![pc(h - 30.0, s, l, "邻近色1", "secondary"),
                 pc(h + 30.0, s * 0.9, l + 5.0, "邻近色2", "tertiary"),
                 pc(h, s * 0.3, 90.0, "中性浅", "background")],
            88,
            "色相邻近、观感柔和，适合通勤与自然风格；可用明度差异拉开层次，避免颜色糊在一起",
        ),
        "triadic" => (
            vec![pc(h + 120.0, s, l, "三色1", "secondary"),
                 pc(h + 240.0, s * 0.9, l + 5.0, "三色2", "accent"),
                 pc(h, s * 0.25, 90.0, "中性浅", "background")],
            82,
            "三色配色活泼平衡，建议一种主色占大面积，另两色等比小面积使用，保持饱和度统一",
        ),
        "split-complementary" => (
            vec![pc(h + 150.0, s, l, "分裂互补1", "secondary"),
                 pc(h + 210.0, s * 0.9, l + 5.0, "分裂互补2", "accent"),
                 pc(h, s * 0.3, 90.0, "中性浅", "background")],
            84,
            "保留互补配色的张力但更温和，适合现代休闲风格，注意控制点缀色的面积",
        ),
        "monochromatic" => (
            vec![pc(h, s, (l + 15.0).min(95.0), "同色系浅", "secondary"),
                 pc(h, s, (l - 15.0).max(5.0), "同色系深", "accent"),
                 pc(h, s * 0.5, (l - 30.0).max(5.0), "同色系暗", "text")],
            80,
            "单色系高级简约，靠明度层次塑造结构；可加入小面积金属色或无彩色提神",
        ),
        // custom / unknown → complementary with a neutral fallback set
        _ => (
            vec![pc(h + 180.0, s, l, "互补色", "accent"),
                 pc(h, s * 0.3, 92.0, "中性浅", "background"),
                 pc(h, s * 0.3, 18.0, "中性深", "text")],
            78,
            "自定义方案，默认按互补色生成；可再指定 complementary/analogous/triadic 等方案",
        ),
    };

    let mut colors = vec![primary_entry];
    colors.extend(extra);

    ColorPalette {
        palette_type: scheme.to_string(),
        primary_hex: primary,
        colors,
        harmony_score: score,
        usage_tips: tips.to_string(),
    }
}

// ── Harmony analysis ──────────────────────────────────────────────────────────

/// Analyze the harmony of a set of HEX colors.
/// Unparseable entries are ignored but echoed back in the result.
pub fn analyze_harmony(colors: &[String]) -> HarmonyResult {
    let parsed: Vec<((u8, u8, u8), (f64, f64, f64))> = colors
        .iter()
        .filter_map(|c| hex_to_rgb(c).map(|rgb| (rgb, rgb_to_hsl(rgb.0, rgb.1, rgb.2))))
        .collect();

    if parsed.is_empty() {
        return HarmonyResult {
            colors: colors.to_vec(),
            harmony_score: 0,
            analysis: HarmonyAnalysis {
                hue_balance: "无法识别任何有效 HEX 色值".into(),
                saturation_contrast: "—".into(),
                lightness_range: "—".into(),
            },
            suggestions: vec!["请提供 #RRGGBB 格式的色值".into()],
            overall: "输入无效，无法分析".into(),
        };
    }

    let n = parsed.len() as f64;
    let hs: Vec<f64> = parsed.iter().map(|(_, hsl)| hsl.0).collect();
    let ss: Vec<f64> = parsed.iter().map(|(_, hsl)| hsl.1).collect();
    let ls: Vec<f64> = parsed.iter().map(|(_, hsl)| hsl.2).collect();

    let s_mean = ss.iter().sum::<f64>() / n;
    let l_min = ls.iter().cloned().fold(100.0, f64::min);
    let l_max = ls.iter().cloned().fold(0.0, f64::max);
    let l_range = l_max - l_min;

    // ── Hue relationships ──────────────────────────────────────────────────────
    // Detect classic schemes from pairwise hue deltas.
    let mut deltas: Vec<f64> = Vec::new();
    for i in 0..hs.len() {
        for j in (i + 1)..hs.len() {
            deltas.push(hue_delta(hs[i], hs[j]).abs());
        }
    }
    let any_near = |target: f64, tol: f64| deltas.iter().any(|d| (d - target).abs() <= tol);
    let achromatic = s_mean < 12.0;

    let (hue_desc, hue_bonus) = if achromatic {
        ("无彩色组合 — 依靠明度与质感变化".to_string(), 6)
    } else if n >= 2.0 && any_near(180.0, 25.0) {
        ("互补/近互补关系 — 对比张力强".to_string(), 14)
    } else if n >= 2.0 && deltas.iter().all(|d| *d <= 40.0) {
        ("邻近色相 — 分布集中、和谐统一".to_string(), 11)
    } else if n >= 3.0 && any_near(120.0, 25.0) {
        ("三色关系 — 色相环分布均衡".to_string(), 12)
    } else if deltas.iter().all(|d| *d <= 70.0) {
        ("色相差异适中 — 温和有序".to_string(), 8)
    } else {
        ("色相分布较散 — 需注意主次关系".to_string(), 2)
    };

    // ── Saturation balance ─────────────────────────────────────────────────────
    let s_min = ss.iter().cloned().fold(100.0, f64::min);
    let s_max = ss.iter().cloned().fold(0.0, f64::max);
    let s_gap = s_max - s_min;
    let (sat_desc, sat_bonus) = if s_gap <= 20.0 {
        ("饱和度一致 — 整体协调".to_string(), 8)
    } else if s_gap <= 45.0 {
        ("饱和度对比适中 — 层次自然".to_string(), 6)
    } else {
        ("饱和度落差较大 — 鲜浊并存，易显突兀".to_string(), -6)
    };

    // ── Lightness contrast ─────────────────────────────────────────────────────
    let (lit_desc, lit_bonus) = if (20.0..=60.0).contains(&l_range) {
        (format!("明度对比 {:.0}% — 层次清晰", l_range), 10)
    } else if l_range < 20.0 {
        (format!("明度对比仅 {:.0}% — 容易发闷", l_range), -4)
    } else {
        (format!("明度对比 {:.0}% — 反差强烈", l_range), 3)
    };

    let mut score = 62 + hue_bonus + sat_bonus + lit_bonus;
    if n >= 5.0 {
        score -= 6; // too many colors tends to dilute focus
    }
    let score = score.clamp(0, 100);

    // ── Suggestions ────────────────────────────────────────────────────────────
    let mut suggestions: Vec<String> = Vec::new();
    if s_gap > 45.0 {
        suggestions.push("尝试把高饱和颜色的纯度降低，使鲜浊程度更统一".into());
    }
    if l_range < 20.0 {
        suggestions.push("拉开明暗差距：提亮一色或加深一色，增加透气感".into());
    }
    if l_range > 65.0 {
        suggestions.push("明暗反差过大，可加入中间明度的中性色缓冲".into());
    }
    if n >= 5.0 {
        suggestions.push("颜色数量偏多，建议收敛到 3–4 个主色，其余作为点缀".into());
    }
    if suggestions.is_empty() {
        suggestions.push("整体关系均衡，可按 60/30/10 的面积比例落地配色".into());
    }

    let overall = if score >= 80 {
        "配色和谐度优秀，可直接用于设计方案"
    } else if score >= 65 {
        "配色和谐度良好，按建议微调后更佳"
    } else {
        "配色存在明显冲突，建议参照建议调整"
    };

    HarmonyResult {
        colors: colors.to_vec(),
        harmony_score: score,
        analysis: HarmonyAnalysis {
            hue_balance: hue_desc,
            saturation_contrast: sat_desc,
            lightness_range: lit_desc,
        },
        suggestions,
        overall: overall.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip_and_hsl() {
        let (r, g, b) = hex_to_rgb("#6366F1").unwrap();
        assert_eq!((r, g, b), (99, 102, 241));
        let (h, s, l) = rgb_to_hsl(r, g, b);
        let (r2, g2, b2) = hsl_to_rgb(h, s, l);
        assert!((r2 as i32 - r as i32).abs() <= 1);
        assert!((g2 as i32 - g as i32).abs() <= 1);
        assert!((b2 as i32 - b as i32).abs() <= 1);
    }

    #[test]
    fn complementary_is_180_apart() {
        let p = build_palette("#FF0000", "complementary");
        let accent = p.colors.iter().find(|c| c.role == "accent").unwrap();
        assert_eq!(accent.hex, "#00FFFF"); // hue 180 from red in HSL is cyan
    }

    #[test]
    fn bad_hex_does_not_panic() {
        let r = analyze_harmony(&["not-a-color".into(), "#fff".into()]);
        assert!(r.harmony_score >= 0);
    }
}
