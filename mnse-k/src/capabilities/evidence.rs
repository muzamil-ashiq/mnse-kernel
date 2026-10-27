//! Structured Evidence & Provenance Contract for MNSE Capabilities.
//!
//! Enforces the rule: Every capability execution returns structured evidence
//! (cryptographic SHA256 hashes of input and output files, engine name/version,
//! execution duration, warnings, and result data) and logs an audit fact to the ledger.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredEvidence {
    pub operation_id: Uuid,
    pub status: String, // "success" | "error" | "warning"
    pub engine: EngineInfo,
    pub input: Option<FileEvidence>,
    pub output: Option<FileEvidence>,
    pub data: serde_json::Value,
    pub warnings: Vec<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineInfo {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEvidence {
    pub path: String,
    pub sha256: String,
    pub media_type: String,
    pub size_bytes: u64,
}

/// Compute SHA256 hash of a file on disk
pub fn hash_file<P: AsRef<Path>>(path: P) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Create FileEvidence if file exists
pub fn inspect_file_evidence<P: AsRef<Path>>(path: P) -> Option<FileEvidence> {
    let p = path.as_ref();
    if !p.exists() {
        return None;
    }

    let meta = std::fs::metadata(p).ok()?;
    let sha256 = hash_file(p).unwrap_or_else(|_| "hash_error".to_string());
    let media_type = guess_media_type(p).to_string();

    Some(FileEvidence {
        path: p.to_string_lossy().to_string(),
        sha256,
        media_type,
        size_bytes: meta.len(),
    })
}

/// Guess media type from file extension
pub fn guess_media_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "pdf" => "application/pdf",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "odt" => "application/vnd.oasis.opendocument.text",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "html" => "text/html",
        "json" => "application/json",
        "csv" => "text/csv",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_sha256_hash_file_known_vector() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(b"hello world\n").unwrap();
        tmp.flush().unwrap();

        // sha256("hello world\n") = a948904f2f0f479b8f8197694b30184b0d2ed1c1cd2a1ec0fb85d299a192a447
        let hash = hash_file(tmp.path()).unwrap();
        assert_eq!(hash, "a948904f2f0f479b8f8197694b30184b0d2ed1c1cd2a1ec0fb85d299a192a447");
    }

    #[test]
    fn test_inspect_file_evidence() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(b"binary sensory stream").unwrap();
        tmp.flush().unwrap();

        let ev = inspect_file_evidence(tmp.path());
        assert!(ev.is_some());
        let ev = ev.unwrap();
        assert_eq!(ev.size_bytes, 21);
        assert!(!ev.sha256.is_empty());

        // Non-existent file returns None
        assert!(inspect_file_evidence("/tmp/definitely_not_a_real_mnse_file_12345.xyz").is_none());
    }

    #[test]
    fn test_guess_media_type() {
        assert_eq!(guess_media_type(Path::new("doc.pdf")), "application/pdf");
        assert_eq!(guess_media_type(Path::new("doc.docx")), "application/vnd.openxmlformats-officedocument.wordprocessingml.document");
        assert_eq!(guess_media_type(Path::new("img.png")), "image/png");
        assert_eq!(guess_media_type(Path::new("readme.md")), "text/markdown");
        assert_eq!(guess_media_type(Path::new("archive.unknown")), "application/octet-stream");
    }
}
