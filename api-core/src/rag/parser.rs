//! Document parser — extracts plain text from PDF / DOCX / TXT / MD files.
//!
//! Supported formats:
//!   - PDF:  via `lopdf` (text layer only; scanned PDFs return empty)
//!   - DOCX: via zip + XML tag stripping (no external deps)
//!   - TXT:  direct read
//!   - MD:   direct read (Markdown is plain text for embedding purposes)

use anyhow::{Context, Result};

/// Extracts plain text from a document file.
pub struct DocumentParser;

impl DocumentParser {
    /// Parse a document by file extension.
    pub async fn parse(file_bytes: &[u8], file_type: &str) -> Result<String> {
        match file_type.to_lowercase().as_str() {
            "pdf" => Self::parse_pdf(file_bytes),
            "docx" => Self::parse_docx(file_bytes),
            "txt" | "md" => Self::parse_txt(file_bytes),
            _ => anyhow::bail!("Unsupported file type: {file_type}"),
        }
    }

    /// Parse a PDF from raw bytes using lopdf.
    pub fn parse_pdf(bytes: &[u8]) -> Result<String> {
        let doc = lopdf::Document::load_mem(bytes)
            .context("Failed to load PDF — is it a valid PDF?")?;

        let page_numbers: Vec<u32> = doc.get_pages().keys().copied().collect();
        let mut pages: Vec<String> = Vec::new();

        for page_num in page_numbers {
            if let Ok(content) = doc.extract_text(&[page_num]) {
                let trimmed = content.trim().to_string();
                if !trimmed.is_empty() {
                    pages.push(trimmed);
                }
            }
        }

        let text = pages.join("\n\n");
        if text.is_empty() {
            tracing::warn!("PDF extracted empty text — may be scanned/image-based");
        }
        Ok(text)
    }

    /// Parse a DOCX from raw bytes.
    /// DOCX is a ZIP file; the body lives in `word/document.xml`.
    pub fn parse_docx(bytes: &[u8]) -> Result<String> {
        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor)?;

        let mut xml_buf = String::new();
        {
            let mut doc_file = archive.by_name("word/document.xml")?;
            use std::io::Read;
            doc_file.read_to_string(&mut xml_buf)?;
        }

        Ok(Self::xml_to_text(&xml_buf))
    }

    /// Strip XML tags from a DOCX body, preserving paragraph breaks.
    fn xml_to_text(xml: &str) -> String {
        let mut result = String::new();
        let mut in_tag = false;

        for c in xml.chars() {
            match c {
                '<' => {
                    // Insert a paragraph break before opening certain tags
                    // is handled implicitly: every tag boundary becomes a
                    // potential newline candidate.
                    if !result.is_empty() && !result.ends_with('\n') {
                        result.push('\n');
                    }
                    in_tag = true;
                }
                '>' => {
                    in_tag = false;
                }
                '\n' | '\r' | '\t' | ' ' => {
                    // Collapse whitespace runs, but don't add after a newline.
                    if !result.is_empty() && !result.ends_with('\n') && !result.ends_with(' ') {
                        result.push(' ');
                    }
                }
                _ if !in_tag => {
                    result.push(c);
                }
                _ => {}
            }
        }

        // Collapse consecutive newlines.
        let mut collapsed = String::new();
        let mut last_was_newline = false;

        for c in result.chars() {
            if c == '\n' {
                if !last_was_newline {
                    collapsed.push('\n');
                }
                last_was_newline = true;
            } else {
                collapsed.push(c);
                last_was_newline = false;
            }
        }

        collapsed.trim().to_string()
    }

    /// Parse plain text / Markdown (both are UTF-8 text for our purposes).
    pub fn parse_txt(bytes: &[u8]) -> Result<String> {
        Ok(String::from_utf8_lossy(bytes).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txt_roundtrip() {
        let text = DocumentParser::parse_txt(b"Hello, world!").unwrap();
        assert_eq!(text, "Hello, world!");
    }

    #[test]
    fn xml_strips_tags() {
        let xml = "<w:p><w:t>Hello</w:t></w:p><w:p><w:t>World</w:t></w:p>";
        let text = DocumentParser::xml_to_text(xml);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
    }
}
