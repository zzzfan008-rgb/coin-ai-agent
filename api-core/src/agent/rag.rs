//! RAG retriever — queries Qdrant for relevant context.

use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;

/// Qdrant search result payload.
#[derive(Debug, Deserialize)]
struct QdrantSearchResult {
    pub id: String,
    pub score: f32,
    pub payload: serde_json::Value,
}

/// RAG retriever backed by Qdrant.
pub struct RagRetriever {
    http: Client,
    qdrant_url: String,
}

impl RagRetriever {
    pub fn new(qdrant_url: &str) -> Self {
        Self {
            http: Client::new(),
            qdrant_url: qdrant_url.to_string(),
        }
    }

    /// Retrieve relevant text chunks from Qdrant collections.
    ///
    /// `query` — the user query (will be embedded and searched)
    /// `collections` — list of collection names to search
    /// Returns concatenated relevant text.
    pub async fn retrieve(&self, query: &str, collections: &[String]) -> Result<String> {
        if collections.is_empty() {
            return Ok(String::new());
        }

        // Phase 1B: implement actual embedding + Qdrant search
        // For Phase 1A stub, return empty context
        tracing::debug!(query = %query, collections = ?collections, "RAG retrieve stub");
        Ok(String::new())
    }

    /// Full Qdrant search — Phase 1B implementation.
    pub async fn qdrant_search(
        &self,
        collection: &str,
        vector: &[f32],
        limit: usize,
    ) -> Result<Vec<String>> {
        let url = format!("{}/collections/{}/points/search", self.qdrant_url, collection);

        let body = serde_json::json!({
            "vector": vector,
            "limit": limit,
            "with_payload": true,
        });

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let results: Vec<QdrantSearchResult> = resp.json().await?;

        Ok(results
            .into_iter()
            .filter_map(|r| {
                r.payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
            .collect())
    }
}
