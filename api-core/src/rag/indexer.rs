//! Document indexer — chunks text, embeds each chunk, and upserts to Qdrant.
//!
//! Chunk strategy: sliding window of ~`chunk_size` tokens (estimated at
//! 4 chars/token) with `chunk_overlap` token overlap.
//!
//! Pipeline:
//! ```text
//! parse() → chunk_text() → embed per chunk → upsert batch → UPDATE doc row
//! ```

use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;

use super::{QdrantChunk, QdrantPayload, RagRetriever};

/// Runs the full parse → chunk → embed → upsert pipeline for one document.
pub struct DocumentIndexer;

impl DocumentIndexer {
    /// Index a document from raw file bytes.
    ///
    /// # Arguments
    /// * `retriever`  — shared RAG service (embedding + Qdrant)
    /// * `doc_id`     — `knowledge_documents.id`
    /// * `org_id`     — organisation UUID
    /// * `dept_id`    — department UUID
    /// * `title`      — document title (stored in payload)
    /// * `file_bytes` — raw uploaded file
    /// * `file_type`  — `pdf` | `docx` | `txt` | `md`
    /// * `pool`       — PostgreSQL pool for status updates
    ///
    /// Returns the number of chunks successfully indexed.
    pub async fn index_document(
        retriever: &RagRetriever,
        doc_id: &str,
        org_id: &str,
        dept_id: &str,
        title: &str,
        file_bytes: &[u8],
        file_type: &str,
        pool: &PgPool,
    ) -> Result<usize> {
        // ── 1. Parse ─────────────────────────────────────────────────────────
        tracing::info!(doc_id, file_type, "Parsing document");
        let text = super::DocumentParser::parse(file_bytes, file_type).await?;

        if text.trim().is_empty() {
            Self::set_failed(pool, doc_id, "Document contains no extractable text").await?;
            return Ok(0);
        }

        Self::set_status(pool, doc_id, "processing").await?;

        // ── 2. Chunk ────────────────────────────────────────────────────────
        let cfg = retriever.config();
        let chunks = Self::chunk_text(&text, cfg.chunk_size, cfg.chunk_overlap);
        let total_chunks = chunks.len();

        if chunks.is_empty() {
            Self::set_failed(pool, doc_id, "No chunks produced").await?;
            return Ok(0);
        }

        tracing::info!(doc_id, total_chunks, "Text chunked");

        // ── 3. Embed + upsert in batches ────────────────────────────────────
        let mut indexed: usize = 0;
        let mut batch: Vec<QdrantChunk> = Vec::new();

        for (i, chunk_text) in chunks.iter().enumerate() {
            match retriever.embed_text(chunk_text).await {
                Ok(vector) => {
                    // Deterministic point ID so re-indexing replaces rather
                    // than duplicates. Qdrant requires UUID or unsigned int.
                    let point_id = Uuid::new_v5(
                        &Uuid::NAMESPACE_OID,
                        format!("{doc_id}:{i}").as_bytes(),
                    );

                    batch.push(QdrantChunk {
                        id: point_id.to_string(),
                        vector,
                        payload: QdrantPayload {
                            text: chunk_text.clone(),
                            doc_id: doc_id.to_string(),
                            dept_id: dept_id.to_string(),
                            org_id: org_id.to_string(),
                            title: title.to_string(),
                            chunk_index: i as i32,
                        },
                    });
                    indexed += 1;
                }
                Err(e) => {
                    tracing::warn!(doc_id, chunk_index = i, "Embed failed, skipping: {e}");
                }
            }

            let batch_full = batch.len() >= cfg.upsert_batch_size;
            let last_chunk = i == total_chunks - 1;

            if (batch_full || last_chunk) && !batch.is_empty() {
                if let Err(e) = retriever.upsert_chunks(std::mem::take(&mut batch)).await {
                    tracing::error!(doc_id, "Qdrant batch upsert failed: {e}");
                }
            }
        }

        // ── 4. Final status ─────────────────────────────────────────────────
        let status = if indexed == 0 {
            "failed"
        } else if indexed < total_chunks {
            "ready" // partial — some chunks failed but doc is usable
        } else {
            "ready"
        };

        sqlx::query(
            r#"UPDATE knowledge_documents
               SET status = $1,
                   chunk_count = $2,
                   processed_at = NOW(),
                   error_message = CASE WHEN $3 < $4
                       THEN 'Some chunks failed to embed'
                       ELSE NULL END
               WHERE id = $5"#,
        )
        .bind(status)
        .bind(indexed as i32)
        .bind(indexed as i32)
        .bind(total_chunks as i32)
        .bind(doc_id)
        .execute(pool)
        .await?;

        tracing::info!(doc_id, status, indexed, total_chunks, "Document indexing complete");
        Ok(indexed)
    }

    // ── Status helpers ───────────────────────────────────────────────────────

    async fn set_status(pool: &PgPool, doc_id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE knowledge_documents SET status = $1 WHERE id = $2")
            .bind(status)
            .bind(doc_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    async fn set_failed(pool: &PgPool, doc_id: &str, message: &str) -> Result<()> {
        sqlx::query(
            "UPDATE knowledge_documents
             SET status = 'failed', error_message = $1, processed_at = NOW()
             WHERE id = $2",
        )
        .bind(message)
        .bind(doc_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    // ── Chunking ─────────────────────────────────────────────────────────────

    /// Split text into overlapping chunks using a character-based sliding
    /// window. Token count is estimated at ~4 characters per token, which
    /// works well for mixed CJK/Latin content.
    pub fn chunk_text(text: &str, chunk_tokens: usize, overlap_tokens: usize) -> Vec<String> {
        let chunk_chars = chunk_tokens * 4;
        let overlap_chars = overlap_tokens * 4;
        let step = chunk_chars.saturating_sub(overlap_chars).max(1);

        let chars: Vec<char> = text.chars().collect();

        if chars.len() <= chunk_chars {
            let trimmed: String = chars.iter().collect::<String>().trim().to_string();
            return if trimmed.is_empty() { Vec::new() } else { vec![trimmed] };
        }

        let mut chunks = Vec::new();
        let mut start = 0usize;

        while start < chars.len() {
            let end = (start + chunk_chars).min(chars.len());
            let chunk: String = chars[start..end].iter().collect();
            let trimmed = chunk.trim().to_string();

            if !trimmed.is_empty() {
                chunks.push(trimmed);
            }

            if end == chars.len() {
                break;
            }
            start += step;
        }

        chunks
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_single_chunk() {
        let chunks = DocumentIndexer::chunk_text("hello world", 500, 50);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "hello world");
    }

    #[test]
    fn empty_text_no_chunks() {
        assert!(DocumentIndexer::chunk_text("   \n  ", 500, 50).is_empty());
    }

    #[test]
    fn long_text_produces_overlapping_chunks() {
        // 4000 chars, chunk = 400 chars (100 tokens * 4), step = 360
        let text: String = "abcdefghij".repeat(400); // 4000 chars
        let chunks = DocumentIndexer::chunk_text(&text, 100, 10);
        assert!(chunks.len() > 1);
        // First chunk should be 400 chars
        assert_eq!(chunks[0].len(), 400);
    }
}
