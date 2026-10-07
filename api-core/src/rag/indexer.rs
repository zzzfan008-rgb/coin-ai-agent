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
        // knowledge_documents.id is uuid; bind a Uuid, never a &str
        // (Postgres rejects `uuid = text` with no implicit cast).
        let doc_uuid = Uuid::parse_str(doc_id)?;
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
        // P2-1: every failed Qdrant batch upsert is recorded — the final
        // status decision must never mark the document ready when points
        // were lost.
        let mut upsert_errors: Vec<String> = Vec::new();

        for (i, chunk_text) in chunks.iter().enumerate() {
            match retriever.embed_text(chunk_text).await {
                Ok(vector) => {
                    // Deterministic point ID so re-indexing replaces rather
                    // than duplicates. Qdrant requires UUID or unsigned int.
                    let point_id =
                        Uuid::new_v5(&Uuid::NAMESPACE_OID, format!("{doc_id}:{i}").as_bytes());

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
                let batch_len = batch.len();
                let upsert_result = retriever
                    .upsert_chunks(std::mem::take(&mut batch))
                    .await
                    .map_err(|e| {
                        tracing::error!(doc_id, "Qdrant batch upsert failed: {e}");
                        format!("Qdrant batch upsert failed: {e:#}")
                    });
                indexed = Self::apply_upsert_result(
                    indexed,
                    batch_len,
                    upsert_result,
                    &mut upsert_errors,
                );
            }
        }

        // ── 4. Final status ─────────────────────────────────────────────────
        // P2-1: a failed upsert batch must never leave the document ready.
        let (status, error_message) = Self::final_doc_state(indexed, total_chunks, &upsert_errors);

        sqlx::query(
            r#"UPDATE knowledge_documents
               SET status = $1,
                   chunk_count = $2,
                   processed_at = NOW(),
                   error_message = $3
               WHERE id = $4"#,
        )
        .bind(status)
        .bind(indexed as i32)
        .bind(error_message)
        .bind(doc_uuid)
        .execute(pool)
        .await?;

        tracing::info!(
            doc_id,
            status,
            indexed,
            total_chunks,
            "Document indexing complete"
        );
        Ok(indexed)
    }

    // ── Status helpers ───────────────────────────────────────────────────────

    async fn set_status(pool: &PgPool, doc_id: &str, status: &str) -> Result<()> {
        let doc_uuid = Uuid::parse_str(doc_id)?;
        sqlx::query("UPDATE knowledge_documents SET status = $1 WHERE id = $2")
            .bind(status)
            .bind(doc_uuid)
            .execute(pool)
            .await?;
        Ok(())
    }

    async fn set_failed(pool: &PgPool, doc_id: &str, message: &str) -> Result<()> {
        let doc_uuid = Uuid::parse_str(doc_id)?;
        sqlx::query(
            "UPDATE knowledge_documents
             SET status = 'failed', error_message = $1, processed_at = NOW()
             WHERE id = $2",
        )
        .bind(message)
        .bind(doc_uuid)
        .execute(pool)
        .await?;
        Ok(())
    }

    // ── Status decision ─────────────────────────────────────────────────────

    /// P2-1 wiring invariant (hermetically tested): apply one batch-upsert
    /// outcome to the run totals. On failure the error message is pushed to
    /// `upsert_errors` AND the batch length is rolled back from `indexed`
    /// (those points were never persisted), so `final_doc_state` can never
    /// report `ready` for a run that lost points. On success `indexed` is
    /// returned untouched. Pure function — no I/O — so the wiring itself is
    /// covered without touching Qdrant.
    fn apply_upsert_result(
        indexed: usize,
        batch_len: usize,
        upsert_result: Result<(), String>,
        upsert_errors: &mut Vec<String>,
    ) -> usize {
        match upsert_result {
            Ok(()) => indexed,
            Err(e) => {
                upsert_errors.push(e);
                // The points in this batch were NOT persisted — do not
                // count them as indexed (P2-1).
                indexed.saturating_sub(batch_len)
            }
        }
    }

    /// Final `(status, error_message)` decision for an indexing run.
    ///
    /// P2-1 invariant: a failed Qdrant batch upsert must NEVER produce
    /// `ready` — the points of that batch were not persisted, so the
    /// document must be `failed` with the upsert error recorded.
    /// `ready` with a message is reserved for embed-only partial
    /// failures (some chunks failed to embed but everything that could
    /// be embedded was persisted).
    fn final_doc_state(
        indexed: usize,
        total_chunks: usize,
        upsert_errors: &[String],
    ) -> (&'static str, Option<String>) {
        if !upsert_errors.is_empty() {
            return ("failed", Some(upsert_errors.join("; ")));
        }
        if indexed == 0 {
            return ("failed", Some("Some chunks failed to embed".into()));
        }
        if indexed < total_chunks {
            // partial — some chunks failed to embed but doc is usable
            return ("ready", Some("Some chunks failed to embed".into()));
        }
        ("ready", None)
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
            return if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![trimmed]
            };
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

    // ── P2-1: upsert failure must never mark the document ready ────────────

    #[test]
    fn upsert_failure_wiring_rolls_back_indexed_and_records_error() {
        // P3-1 wiring invariant: the batch loop feeds apply_upsert_result;
        // a failed batch MUST roll `indexed` back by the batch length AND
        // record the error, so final_doc_state sees the loss.
        let mut upsert_errors = Vec::new();
        let indexed = DocumentIndexer::apply_upsert_result(
            5,
            3,
            Err("Qdrant batch upsert failed: connection refused".into()),
            &mut upsert_errors,
        );
        assert_eq!(indexed, 2, "failed batch points must not stay indexed");
        assert_eq!(upsert_errors.len(), 1);
        // Composition with final_doc_state: the run is failed, never ready.
        let (status, msg) = DocumentIndexer::final_doc_state(indexed, 5, &upsert_errors);
        assert_eq!(status, "failed");
        assert!(msg.unwrap().contains("upsert failed"));
    }

    #[test]
    fn upsert_success_wiring_keeps_indexed_untouched() {
        let mut upsert_errors = Vec::new();
        let indexed = DocumentIndexer::apply_upsert_result(5, 3, Ok(()), &mut upsert_errors);
        assert_eq!(indexed, 5);
        assert!(upsert_errors.is_empty());
    }

    #[test]
    fn upsert_failure_marks_failed_never_ready() {
        let errs = vec!["Qdrant batch upsert failed: connection refused".to_string()];
        // Even when every chunk embedded fine, a lost upsert batch forces
        // `failed` and records the error.
        let (status, msg) = DocumentIndexer::final_doc_state(3, 3, &errs);
        assert_eq!(status, "failed");
        assert!(msg.unwrap().contains("upsert failed"));
        // Partial embed success + one lost batch still fails.
        let (status, _) = DocumentIndexer::final_doc_state(4, 5, &errs);
        assert_eq!(status, "failed");
    }

    #[test]
    fn all_embedded_and_persisted_is_ready_without_message() {
        let (status, msg) = DocumentIndexer::final_doc_state(2, 2, &[]);
        assert_eq!(status, "ready");
        assert!(msg.is_none());
    }

    #[test]
    fn embed_only_partial_failure_is_ready_with_message() {
        // Legacy semantics kept: embed failures alone leave the doc usable.
        let (status, msg) = DocumentIndexer::final_doc_state(2, 3, &[]);
        assert_eq!(status, "ready");
        assert_eq!(msg.as_deref(), Some("Some chunks failed to embed"));
    }

    #[test]
    fn zero_chunks_indexed_is_failed() {
        let (status, msg) = DocumentIndexer::final_doc_state(0, 3, &[]);
        assert_eq!(status, "failed");
        assert!(msg.is_some());
    }
}
