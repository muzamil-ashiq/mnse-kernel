//! Universal Document Capability Provider — Converter, Office Engine, and OCR.
//!
//! Powered by LibreOffice Headless and Tesseract OCR.

use super::evidence::{inspect_file_evidence, EngineInfo, StructuredEvidence};
use anyhow::{anyhow, Result};
use serde_json::json;
use std::path::Path;
use std::process::Command;
use std::time::Instant;
use uuid::Uuid;

pub struct DocCapability;

impl DocCapability {
    /// Writer/Converter: Convert documents (DOCX, PPTX, XLSX, HTML, etc.) to target format via LibreOffice Headless
    pub fn convert(input_path: &str, target_format: &str, out_param: Option<&str>) -> Result<StructuredEvidence> {
        let input_p = Path::new(input_path);
        if !input_p.exists() {
            return Err(anyhow!("Input document not found: {}", input_path));
        }

        let start_time = Instant::now();
        let input_ev = inspect_file_evidence(input_path);

        // Differentiate between explicit destination filename and output directory
        let (output_directory, explicit_dest) = if let Some(out) = out_param {
            let out_p = Path::new(out);
            if out_p.extension().is_some() {
                let parent = out_p.parent().unwrap_or_else(|| Path::new("."));
                (parent.to_path_buf(), Some(out_p.to_path_buf()))
            } else {
                (out_p.to_path_buf(), None)
            }
        } else {
            (input_p.parent().unwrap_or_else(|| Path::new(".")).to_path_buf(), None)
        };

        std::fs::create_dir_all(&output_directory)?;

        let output = Command::new("libreoffice")
            .arg("--headless")
            .arg("--convert-to")
            .arg(target_format)
            .arg(input_path)
            .arg("--outdir")
            .arg(&output_directory)
            .output()
            .map_err(|e| anyhow!("Failed to execute libreoffice: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("LibreOffice conversion failed: {}", stderr.trim()));
        }

        // Expected output filename: stem + target_format
        let stem = input_p.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
        let default_out = output_directory.join(format!("{}.{}", stem, target_format));

        let final_out = if let Some(dest) = explicit_dest {
            if default_out.exists() && default_out != dest {
                let _ = std::fs::rename(&default_out, &dest);
            }
            dest
        } else {
            default_out
        };

        let output_ev = inspect_file_evidence(&final_out);
        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(StructuredEvidence {
            operation_id: Uuid::new_v4(),
            status: "success".to_string(),
            engine: EngineInfo {
                name: "libreoffice-headless".to_string(),
                version: Some("24.8".to_string()),
            },
            input: input_ev,
            output: output_ev,
            data: json!({
                "source": input_path,
                "target_format": target_format,
                "output_file": final_out.to_string_lossy()
            }),
            warnings: Vec::new(),
            duration_ms,
        })
    }

    /// Reader/OCR: Perform optical character recognition on scanned images or photos via Tesseract
    pub fn ocr(image_path: &str, lang: Option<&str>) -> Result<StructuredEvidence> {
        let p = Path::new(image_path);
        if !p.exists() {
            return Err(anyhow!("Image file not found: {}", image_path));
        }

        let start_time = Instant::now();
        let input_ev = inspect_file_evidence(image_path);

        let mut cmd = Command::new("tesseract");
        cmd.arg(image_path);
        cmd.arg("stdout"); // print OCR output directly to stdout
        if let Some(l) = lang {
            cmd.arg("-l").arg(l);
        }

        let output = cmd.output().map_err(|e| anyhow!("Failed to execute tesseract: {}", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Tesseract OCR failed: {}", stderr.trim()));
        }

        let text = String::from_utf8_lossy(&output.stdout).to_string();
        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(StructuredEvidence {
            operation_id: Uuid::new_v4(),
            status: "success".to_string(),
            engine: EngineInfo {
                name: "tesseract-ocr".to_string(),
                version: Some("5.5".to_string()),
            },
            input: input_ev,
            output: None,
            data: json!({
                "image": image_path,
                "lang": lang.unwrap_or("eng"),
                "char_count": text.len(),
                "text": text
            }),
            warnings: Vec::new(),
            duration_ms,
        })
    }

    /// Inspector: Detect true MIME type from file content bytes via Linux `file` utility
    pub fn detect_type(path: &str) -> Result<StructuredEvidence> {
        let start_time = Instant::now();
        let input_ev = inspect_file_evidence(path);

        let output = Command::new("file")
            .arg("--mime-type")
            .arg("-b")
            .arg(path)
            .output()
            .map_err(|e| anyhow!("Failed to execute file command: {}", e))?;

        let mime = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(StructuredEvidence {
            operation_id: Uuid::new_v4(),
            status: "success".to_string(),
            engine: EngineInfo {
                name: "linux-file-mime".to_string(),
                version: None,
            },
            input: input_ev,
            output: None,
            data: json!({
                "path": path,
                "detected_mime": mime
            }),
            warnings: Vec::new(),
            duration_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_doc_detect_type() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(b"{\"hello\": \"mnse\"}\n").unwrap();
        tmp.flush().unwrap();

        let ev = DocCapability::detect_type(tmp.path().to_str().unwrap()).expect("detect type");
        assert_eq!(ev.status, "success");
        let mime = ev.data["detected_mime"].as_str().unwrap();
        assert!(mime.contains("json") || mime.contains("text"));
    }

    #[test]
    fn test_doc_missing_file_error() {
        let res = DocCapability::convert("/non_existent_file.xyz", "pdf", None);
        assert!(res.is_err());
    }

    #[test]
    fn test_doc_injection_defense() {
        let evil_path = "/tmp/does_not_exist; touch /tmp/doc_evil_flag";
        let res = DocCapability::detect_type(evil_path);
        // file command runs but does NOT spawn a shell to create /tmp/doc_evil_flag
        assert!(!Path::new("/tmp/doc_evil_flag").exists());
        let _ = res;
    }
}
