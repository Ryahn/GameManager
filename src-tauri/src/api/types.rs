//! Serde models for the f95api.xyz endpoints the launcher uses.
//!
//! Every field is `#[serde(default)]` so additive API changes never break
//! older launchers. Shapes mirror `/openapi.json` (F95ApiDB 0.1.0).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ThreadSummary {
    pub thread_id: i64,
    pub url: String,
    pub title: String,
    pub creator: String,
    pub version: String,
    pub developer: String,
    pub category: String,
    pub sam_ts: Option<i64>,
    pub last_promoted: Option<i64>,
    pub first_post_ts: Option<i64>,
    pub first_post_date: Option<String>,
    pub is_sticky: bool,
    pub is_new: bool,
    pub promotion_banned: bool,
    pub prefixes: Vec<String>,
    pub cover: Option<String>,
    pub banner: Option<String>,
    pub previews: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExternalLink {
    pub role: String,
    pub host: String,
    pub url: String,
    pub label: String,
    pub dlsite_product_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ThreadDetail {
    #[serde(flatten)]
    pub summary: ThreadSummary,
    pub overview: Option<String>,
    pub views: Option<i64>,
    pub likes: Option<i64>,
    pub genre: Option<String>,
    pub changelog: Option<String>,
    pub thread_updated: Option<String>,
    pub release_date: Option<String>,
    pub censored: Option<String>,
    pub os: Option<String>,
    pub language: Option<String>,
    pub sam_rating: Option<f64>,
    pub tag_names: Vec<String>,
    pub external_links: Vec<ExternalLink>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SearchHit {
    pub thread_id: i64,
    pub title: String,
    pub creator: String,
    pub url: String,
    pub category: String,
    pub score: Option<f64>,
    pub cover: Option<String>,
    pub banner: Option<String>,
    pub previews: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SearchResponse {
    pub query: String,
    pub hits: Vec<SearchHit>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Rating {
    pub average: Option<f64>,
    pub best: Option<f64>,
    pub count: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Author {
    pub name: String,
}

/// `/v1/f95/latest` item (typed "handiwork"). Only the fields the UI uses;
/// download links are partner-only and intentionally not modelled.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Handiwork {
    pub id: i64,
    pub name: String,
    pub overview: String,
    pub authors: Vec<Author>,
    pub category: String,
    pub cover: Option<String>,
    pub banner: Option<String>,
    pub previews: Vec<String>,
    pub last_thread_update: Option<String>,
    pub thread_publishing_date: Option<String>,
    pub last_promoted: Option<i64>,
    pub prefixes: Vec<String>,
    pub rating: Rating,
    pub tags: Vec<String>,
    pub url: String,
    pub engine: Option<String>,
    pub status: Option<String>,
    pub version: Option<String>,
    pub os: serde_json::Value,
    pub language: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Bookmark {
    pub thread_id: i64,
    pub has_update: bool,
    pub flagged_at: Option<String>,
    pub last_seen_version: String,
    pub current_version: String,
    pub needs_manual_review: bool,
    pub review_reason: Option<String>,
    pub thread: Option<ThreadSummary>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BookmarkListResponse {
    pub total: i64,
    pub items: Vec<Bookmark>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BulkBookmarkResponse {
    pub created: Vec<i64>,
    pub already_tracked: Vec<i64>,
    pub not_found: Vec<i64>,
}

/// Query for `/v1/f95/latest`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LatestQuery {
    pub category: Option<String>,
    pub order: Option<String>,
    pub date: Option<u32>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub included_tags: Vec<String>,
    pub excluded_tags: Vec<String>,
}

impl LatestQuery {
    pub fn to_pairs(&self) -> Vec<(&'static str, String)> {
        let mut q = vec![
            (
                "category",
                self.category.clone().unwrap_or_else(|| "games".into()),
            ),
            ("kind", "handiwork".into()),
        ];
        if let Some(o) = &self.order {
            q.push(("order", o.clone()));
        }
        if let Some(d) = self.date {
            q.push(("date", d.to_string()));
        }
        q.push(("page", self.page.unwrap_or(1).max(1).to_string()));
        q.push(("limit", self.limit.unwrap_or(30).clamp(1, 90).to_string()));
        for t in &self.included_tags {
            q.push(("included_tags", t.clone()));
        }
        for t in &self.excluded_tags {
            q.push(("excluded_tags", t.clone()));
        }
        q
    }
}
