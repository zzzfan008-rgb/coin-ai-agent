#![allow(dead_code)]

//! Fashion DB store — real DB queries for fabrics, colors, and styles.
//!
//! Uses the same `PgPool` as `SessionStore` (shared via `AppState`).
//! All queries are scoped by `org_id` and `dept_id` for multi-tenancy.

use serde::Serialize;
use sqlx::{postgres::PgPool, QueryBuilder, Row};
use uuid::Uuid;

use crate::error::{AppError, Result};

/// Shared pool handle for fashion tables.
#[derive(Clone)]
pub struct FashionStore {
    pool: PgPool,
}

impl FashionStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // ── Fabrics ─────────────────────────────────────────────────────────────────

    /// Search fabrics by name or composition keyword.
    pub async fn search_fabrics(
        &self,
        org_id: Uuid,
        dept_id: Uuid,
        query: &str,
        limit: i64,
    ) -> Result<Vec<Fabric>> {
        let pattern = format!("%{query}%");
        let rows = sqlx::query(
            r#"SELECT id, org_id, dept_id, name, composition, weight_gm2,
                      season, applicable_styles, care_instructions, features,
                      created_at
               FROM fabrics
               WHERE org_id = $1 AND dept_id = $2
                 AND (name ILIKE $3 OR composition ILIKE $3)
               ORDER BY created_at DESC
               LIMIT $4"#,
        )
        .bind(org_id)
        .bind(dept_id)
        .bind(&pattern)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(Fabric::from_row).collect())
    }

    /// List all fabrics for an org/dept.
    pub async fn list_fabrics(
        &self,
        org_id: Uuid,
        dept_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Fabric>> {
        let rows = sqlx::query(
            r#"SELECT id, org_id, dept_id, name, composition, weight_gm2,
                      season, applicable_styles, care_instructions, features,
                      created_at
               FROM fabrics
               WHERE org_id = $1 AND dept_id = $2
               ORDER BY created_at DESC
               LIMIT $3"#,
        )
        .bind(org_id)
        .bind(dept_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(Fabric::from_row).collect())
    }

    /// Filter fabrics by season.
    pub async fn filter_fabrics_by_season(
        &self,
        org_id: Uuid,
        dept_id: Uuid,
        season: &str,
    ) -> Result<Vec<Fabric>> {
        let rows = sqlx::query(
            r#"SELECT id, org_id, dept_id, name, composition, weight_gm2,
                      season, applicable_styles, care_instructions, features,
                      created_at
               FROM fabrics
               WHERE org_id = $1 AND dept_id = $2
                 AND (season = $3 OR season = 'all-season')
               ORDER BY created_at DESC"#,
        )
        .bind(org_id)
        .bind(dept_id)
        .bind(season)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(Fabric::from_row).collect())
    }

    /// Get a single fabric by ID.
    pub async fn get_fabric(&self, org_id: Uuid, dept_id: Uuid, fabric_id: Uuid) -> Result<Fabric> {
        let row = sqlx::query(
            r#"SELECT id, org_id, dept_id, name, composition, weight_gm2,
                      season, applicable_styles, care_instructions, features,
                      created_at
               FROM fabrics
               WHERE id = $1 AND org_id = $2 AND dept_id = $3"#,
        )
        .bind(fabric_id)
        .bind(org_id)
        .bind(dept_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Fabric {fabric_id} not found")))?;

        Ok(Fabric::from_row(&row))
    }

    // ── Colors ──────────────────────────────────────────────────────────────────

    /// Search colors by name or hex.
    pub async fn search_colors(
        &self,
        org_id: Uuid,
        dept_id: Uuid,
        query: &str,
        limit: i64,
    ) -> Result<Vec<Color>> {
        let pattern = format!("%{query}%");
        let rows = sqlx::query(
            r#"SELECT id, org_id, dept_id, hex, name, rgb_r, rgb_g, rgb_b,
                      hsl_h, hsl_s, hsl_l, category, season, created_at
               FROM colors
               WHERE org_id = $1 AND dept_id = $2
                 AND (name ILIKE $3 OR hex ILIKE $3)
               ORDER BY created_at DESC
               LIMIT $4"#,
        )
        .bind(org_id)
        .bind(dept_id)
        .bind(&pattern)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(Color::from_row).collect())
    }

    /// Get all colors for a season.
    pub async fn get_colors_by_season(
        &self,
        org_id: Uuid,
        dept_id: Uuid,
        season: &str,
    ) -> Result<Vec<Color>> {
        let rows = sqlx::query(
            r#"SELECT id, org_id, dept_id, hex, name, rgb_r, rgb_g, rgb_b,
                      hsl_h, hsl_s, hsl_l, category, season, created_at
               FROM colors
               WHERE org_id = $1 AND dept_id = $2 AND season = $3
               ORDER BY created_at DESC"#,
        )
        .bind(org_id)
        .bind(dept_id)
        .bind(season)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(Color::from_row).collect())
    }

    /// Get complementary/analogous/triadic suggestions for a given hex.
    /// Phase 1B note: the executor computes palettes in `color_theory`;
    /// this DB-backed helper is retained for random curated suggestions.
    pub async fn suggest_color_palette(
        &self,
        org_id: Uuid,
        dept_id: Uuid,
        _primary_hex: &str,
    ) -> Result<Vec<Color>> {
        let rows = sqlx::query(
            r#"SELECT id, org_id, dept_id, hex, name, rgb_r, rgb_g, rgb_b,
                      hsl_h, hsl_s, hsl_l, category, season, created_at
               FROM colors
               WHERE org_id = $1 AND dept_id = $2
                 AND category IN ('accent', 'background', 'text', 'secondary')
               ORDER BY RANDOM()
               LIMIT 5"#,
        )
        .bind(org_id)
        .bind(dept_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(Color::from_row).collect())
    }

    // ── Styles ──────────────────────────────────────────────────────────────────

    /// Search styles by name or garment_type.
    pub async fn search_styles(
        &self,
        org_id: Uuid,
        dept_id: Uuid,
        query: &str,
        limit: i64,
    ) -> Result<Vec<Style>> {
        let pattern = format!("%{query}%");
        let rows = sqlx::query(
            r#"SELECT id, org_id, dept_id, name, description, silhouette,
                      garment_type, key_features, suitable_seasons, target_audience,
                      created_at
               FROM styles
               WHERE org_id = $1 AND dept_id = $2
                 AND (name ILIKE $3 OR garment_type ILIKE $3)
               ORDER BY created_at DESC
               LIMIT $4"#,
        )
        .bind(org_id)
        .bind(dept_id)
        .bind(&pattern)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(Style::from_row).collect())
    }

    /// Get styles by season.
    pub async fn get_styles_by_season(
        &self,
        org_id: Uuid,
        dept_id: Uuid,
        season: &str,
    ) -> Result<Vec<Style>> {
        let rows = sqlx::query(
            r#"SELECT id, org_id, dept_id, name, description, silhouette,
                      garment_type, key_features, suitable_seasons, target_audience,
                      created_at
               FROM styles
               WHERE org_id = $1 AND dept_id = $2
                 AND $3 = ANY(suitable_seasons)
               ORDER BY created_at DESC"#,
        )
        .bind(org_id)
        .bind(dept_id)
        .bind(season)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(Style::from_row).collect())
    }

    /// Get style variations (all styles sharing the same garment_type).
    pub async fn get_style_variations(
        &self,
        org_id: Uuid,
        dept_id: Uuid,
        garment_type: &str,
    ) -> Result<Vec<Style>> {
        let rows = sqlx::query(
            r#"SELECT id, org_id, dept_id, name, description, silhouette,
                      garment_type, key_features, suitable_seasons, target_audience,
                      created_at
               FROM styles
               WHERE org_id = $1 AND dept_id = $2
                 AND garment_type = $3
               ORDER BY name ASC"#,
        )
        .bind(org_id)
        .bind(dept_id)
        .bind(garment_type)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(Style::from_row).collect())
    }

    // ── Fabric → styles ────────────────────────────────────────────────────────

    /// Get a single style by ID.
    pub async fn get_style(
        &self,
        org_id: Uuid,
        dept_id: Uuid,
        style_id: Uuid,
    ) -> Result<Style> {
        let row = sqlx::query(
            r#"SELECT id, org_id, dept_id, name, description, silhouette,
                      garment_type, key_features, suitable_seasons, target_audience,
                      created_at
               FROM styles
               WHERE id = $1 AND org_id = $2 AND dept_id = $3"#,
        )
        .bind(style_id)
        .bind(org_id)
        .bind(dept_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Style {style_id} not found")))?;

        Ok(Style::from_row(&row))
    }

    /// Find the first style whose name matches (exact, then fuzzy).
    pub async fn find_style_by_name(
        &self,
        org_id: Uuid,
        dept_id: Uuid,
        name: &str,
    ) -> Result<Style> {
        let row = sqlx::query(
            r#"SELECT id, org_id, dept_id, name, description, silhouette,
                      garment_type, key_features, suitable_seasons, target_audience,
                      created_at
               FROM styles
               WHERE org_id = $1 AND dept_id = $2 AND name = $3"#,
        )
        .bind(org_id)
        .bind(dept_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            return Ok(Style::from_row(&row));
        }

        // Fuzzy fallback.
        let pattern = format!("%{name}%");
        sqlx::query(
            r#"SELECT id, org_id, dept_id, name, description, silhouette,
                      garment_type, key_features, suitable_seasons, target_audience,
                      created_at
               FROM styles
               WHERE org_id = $1 AND dept_id = $2 AND name ILIKE $3
               ORDER BY created_at DESC LIMIT 1"#,
        )
        .bind(org_id)
        .bind(dept_id)
        .bind(pattern)
        .fetch_optional(&self.pool)
        .await?
        .map(|row| Style::from_row(&row))
        .ok_or_else(|| AppError::NotFound(format!("Style named '{name}' not found")))
    }

    /// Resolve which style tags a fabric carries and find matching `styles`
    /// rows (by garment_type / name pattern or exact key_features overlap).
    pub async fn get_applicable_styles(
        &self,
        org_id: Uuid,
        dept_id: Uuid,
        fabric_id: Uuid,
    ) -> Result<ApplicableStyles> {
        let fabric = self.get_fabric(org_id, dept_id, fabric_id).await?;
        let tags = fabric.applicable_styles.clone();

        let styles = if tags.is_empty() {
            Vec::new()
        } else {
            let patterns: Vec<String> = tags.iter().map(|t| format!("%{t}%")).collect();
            let rows = sqlx::query(
                r#"SELECT id, org_id, dept_id, name, description, silhouette,
                          garment_type, key_features, suitable_seasons, target_audience,
                          created_at
                   FROM styles
                   WHERE org_id = $1 AND dept_id = $2
                     AND (garment_type ILIKE ANY($3)
                          OR name ILIKE ANY($3)
                          OR key_features && $4)
                   ORDER BY created_at DESC
                   LIMIT 20"#,
            )
            .bind(org_id)
            .bind(dept_id)
            .bind(&patterns)
            .bind(&tags)
            .fetch_all(&self.pool)
            .await?;
            rows.iter().map(Style::from_row).collect()
        };

        Ok(ApplicableStyles {
            recommendation: "推荐基于面料成分、克重与特性的款式设计方向".into(),
            fabric,
            tags,
            styles,
        })
    }

    // ── Trend colors ───────────────────────────────────────────────────────────

    /// Trend colors are seeded with category `流行色` (also accepts `trend`),
    /// optionally narrowed by season.
    pub async fn get_trend_colors(
        &self,
        org_id: Uuid,
        dept_id: Uuid,
        season: Option<&str>,
    ) -> Result<Vec<Color>> {
        let rows = if let Some(season) = season.filter(|s| !s.is_empty()) {
            sqlx::query(
                r#"SELECT id, org_id, dept_id, hex, name, rgb_r, rgb_g, rgb_b,
                          hsl_h, hsl_s, hsl_l, category, season, created_at
                   FROM colors
                   WHERE org_id = $1 AND dept_id = $2
                     AND category IN ('流行色', 'trend')
                     AND (season = $3 OR season = 'all-season')
                   ORDER BY created_at DESC LIMIT 20"#,
            )
            .bind(org_id)
            .bind(dept_id)
            .bind(season)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"SELECT id, org_id, dept_id, hex, name, rgb_r, rgb_g, rgb_b,
                          hsl_h, hsl_s, hsl_l, category, season, created_at
                   FROM colors
                   WHERE org_id = $1 AND dept_id = $2
                     AND category IN ('流行色', 'trend')
                   ORDER BY created_at DESC LIMIT 20"#,
            )
            .bind(org_id)
            .bind(dept_id)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(rows.iter().map(Color::from_row).collect())
    }

    // ── Style ideas ────────────────────────────────────────────────────────────

    /// Search the styles library by keywords (name / description / garment_type
    /// / key_features) with an optional garment_type filter.
    pub async fn generate_style_ideas(
        &self,
        org_id: Uuid,
        dept_id: Uuid,
        keywords: &[String],
        garment_type: Option<&str>,
        limit: i64,
    ) -> Result<Vec<StyleIdea>> {
        let mut qb = QueryBuilder::new(
            r#"SELECT id, org_id, dept_id, name, description, silhouette,
                      garment_type, key_features, suitable_seasons, target_audience,
                      created_at
               FROM styles
               WHERE org_id = "#,
        );
        qb.push_bind(org_id).push(" AND dept_id = ").push_bind(dept_id);

        if !keywords.is_empty() {
            qb.push(" AND (");
            for (i, kw) in keywords.iter().enumerate() {
                if i > 0 {
                    qb.push(" OR ");
                }
                let pat = format!("%{kw}%");
                qb.push("name ILIKE ")
                    .push_bind(pat.clone())
                    .push(" OR description ILIKE ")
                    .push_bind(pat.clone())
                    .push(" OR garment_type ILIKE ")
                    .push_bind(pat.clone())
                    .push(" OR array_to_string(key_features, ' ') ILIKE ")
                    .push_bind(pat);
            }
            qb.push(")");
        }

        if let Some(gt) = garment_type.filter(|g| !g.is_empty() && *g != "general") {
            qb.push(" AND garment_type ILIKE ").push_bind(format!("%{gt}%"));
        }

        qb.push(" ORDER BY created_at DESC LIMIT ").push_bind(limit);

        let rows = qb.build().fetch_all(&self.pool).await?;
        let styles: Vec<Style> = rows.iter().map(Style::from_row).collect();

        // Report which input keywords actually hit each row.
        Ok(styles
            .into_iter()
            .map(|s| {
                let haystack = format!(
                    "{} {} {} {}",
                    s.name,
                    s.description.as_deref().unwrap_or(""),
                    s.garment_type.as_deref().unwrap_or(""),
                    s.key_features.join(" ")
                )
                .to_lowercase();
                let matched = keywords
                    .iter()
                    .filter(|k| haystack.contains(&k.to_lowercase()))
                    .cloned()
                    .collect();
                StyleIdea::from_style(s, matched)
            })
            .collect())
    }

    // ── Style variations ───────────────────────────────────────────────────────

    /// Generate variations for a base style. `base_style` may be a style UUID
    /// or a style name (exact → fuzzy match).
    pub async fn style_variations(
        &self,
        org_id: Uuid,
        dept_id: Uuid,
        base_style: &str,
        variation_type: &str,
        count: i64,
    ) -> Result<Vec<StyleVariation>> {
        let base = match Uuid::parse_str(base_style) {
            Ok(id) => self.get_style(org_id, dept_id, id).await?,
            Err(_) => self.find_style_by_name(org_id, dept_id, base_style).await?,
        };
        Ok(build_style_variations(&base, variation_type, count))
    }

    /// Health check.
    pub async fn health_check(&self) -> Result<bool> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(true)
    }
}

// ── Domain types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Fabric {
    pub id: Uuid,
    pub org_id: Uuid,
    pub dept_id: Uuid,
    pub name: String,
    pub composition: Option<String>,
    pub weight_gm2: Option<i32>,
    pub season: Option<String>,
    pub applicable_styles: Vec<String>,
    pub care_instructions: Option<String>,
    pub features: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Fabric {
    fn from_row(row: &sqlx::postgres::PgRow) -> Self {
        Self {
            id: row.get("id"),
            org_id: row.get("org_id"),
            dept_id: row.get("dept_id"),
            name: row.get("name"),
            composition: row.get("composition"),
            weight_gm2: row.get("weight_gm2"),
            season: row.get("season"),
            applicable_styles: row
                .get::<Option<Vec<String>>, _>("applicable_styles")
                .unwrap_or_default(),
            care_instructions: row.get("care_instructions"),
            features: row.get("features"),
            created_at: row.get("created_at"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Color {
    pub id: Uuid,
    pub org_id: Uuid,
    pub dept_id: Uuid,
    pub hex: String,
    pub name: Option<String>,
    pub rgb_r: Option<i32>,
    pub rgb_g: Option<i32>,
    pub rgb_b: Option<i32>,
    pub hsl_h: Option<i32>,
    pub hsl_s: Option<i32>,
    pub hsl_l: Option<i32>,
    pub category: Option<String>,
    pub season: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Color {
    fn from_row(row: &sqlx::postgres::PgRow) -> Self {
        Self {
            id: row.get("id"),
            org_id: row.get("org_id"),
            dept_id: row.get("dept_id"),
            hex: row.get("hex"),
            name: row.get("name"),
            rgb_r: row.get("rgb_r"),
            rgb_g: row.get("rgb_g"),
            rgb_b: row.get("rgb_b"),
            hsl_h: row.get("hsl_h"),
            hsl_s: row.get("hsl_s"),
            hsl_l: row.get("hsl_l"),
            category: row.get("category"),
            season: row.get("season"),
            created_at: row.get("created_at"),
        }
    }
}

// ── Stub response types used by partially-implemented methods ─────────────────

/// Output of `get_applicable_styles`: recommended styles for a fabric.
#[derive(Debug, Clone, Serialize)]
pub struct ApplicableStyles {
    pub recommendation: String,
    pub fabric: Fabric,
    pub tags: Vec<String>,
    pub styles: Vec<Style>,
}

/// A style record annotated with which search keywords matched it.
#[derive(Debug, Clone, Serialize)]
pub struct StyleIdea {
    pub style: Style,
    pub matched_keywords: Vec<String>,
}

impl StyleIdea {
    fn from_style(style: Style, matched_keywords: Vec<String>) -> Self {
        Self { style, matched_keywords }
    }
}

/// A generated variation of a base style.
#[derive(Debug, Clone, Serialize)]
pub struct StyleVariation {
    pub base_name: String,
    pub variation_name: String,
    pub variation_type: String,
    pub description: String,
}

/// Build `count` style variations of the given base style.
fn build_style_variations(
    base: &Style,
    variation_type: &str,
    count: i64,
) -> Vec<StyleVariation> {
    let prefixes = match variation_type {
        "silhouette" => &["A-line 版", "H-line 版", "Oversize 版", "修身版"][..],
        "season" => &["春夏款", "秋冬款", "四季款", "季中款"][..],
        "audience" => &["年轻线", "高端线", "休闲线", "商务线"][..],
        _ => &["经典款", "前卫款", "简约款", "复古款"][..],
    };
    prefixes
        .iter()
        .take(count as usize)
        .map(|prefix| StyleVariation {
            base_name: base.name.clone(),
            variation_name: format!("{} · {}", prefix, base.name),
            variation_type: variation_type.to_string(),
            description: format!(
                "{}风格的 {} 款式变体",
                prefix,
                base.garment_type.as_deref().unwrap_or("常规")
            ),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct Style {
    pub id: Uuid,
    pub org_id: Uuid,
    pub dept_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub silhouette: Option<String>,
    pub garment_type: Option<String>,
    pub key_features: Vec<String>,
    pub suitable_seasons: Vec<String>,
    pub target_audience: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Style {
    fn from_row(row: &sqlx::postgres::PgRow) -> Self {
        Self {
            id: row.get("id"),
            org_id: row.get("org_id"),
            dept_id: row.get("dept_id"),
            name: row.get("name"),
            description: row.get("description"),
            silhouette: row.get("silhouette"),
            garment_type: row.get("garment_type"),
            key_features: row
                .get::<Option<Vec<String>>, _>("key_features")
                .unwrap_or_default(),
            suitable_seasons: row
                .get::<Option<Vec<String>>, _>("suitable_seasons")
                .unwrap_or_default(),
            target_audience: row
                .get::<Option<Vec<String>>, _>("target_audience")
                .unwrap_or_default(),
            created_at: row.get("created_at"),
        }
    }
}
