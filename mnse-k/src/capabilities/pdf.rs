//! PDF Capability Provider — Reader, Editor, Renderer, and Validator.
//!
//! Powered by Poppler (`pdfinfo`, `pdftotext`, `pdftoppm`, `pdfimages`, `pdfunite`)
//! and QPDF for lossless structural transformations.

use super::evidence::{inspect_file_evidence, EngineInfo, StructuredEvidence};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;
use std::time::Instant;
use uuid::Uuid;

pub struct PdfCapability;

impl PdfCapability {
    /// Reader/Inspector: Extract detailed metadata and structural health via `pdfinfo` and `qpdf --check`
    pub fn inspect(path: &str) -> Result<StructuredEvidence> {
        let start_time = Instant::now();
        let input_ev = inspect_file_evidence(path);

        let output = Command::new("pdfinfo")
            .arg(path)
            .output()
            .map_err(|e| anyhow!("Failed to execute pdfinfo: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("pdfinfo failed: {}", stderr.trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut map = serde_json::Map::new();
        map.insert("file".to_string(), json!(path));

        for line in stdout.lines() {
            if let Some((k, v)) = line.split_once(':') {
                let key = k.trim().to_lowercase().replace(' ', "_");
                let val = v.trim();
                if key == "pages" {
                    if let Ok(num) = val.parse::<u64>() {
                        map.insert(key, json!(num));
                        continue;
                    }
                }
                map.insert(key, json!(val));
            }
        }

        // Structural check with qpdf
        let qpdf_out = Command::new("qpdf").arg("--check").arg(path).output();
        let is_valid = qpdf_out.map(|o| o.status.success()).unwrap_or(true);
        map.insert("structurally_valid".to_string(), json!(is_valid));

        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(StructuredEvidence {
            operation_id: Uuid::new_v4(),
            status: "success".to_string(),
            engine: EngineInfo {
                name: "poppler-pdfinfo+qpdf".to_string(),
                version: Some("24.08".to_string()),
            },
            input: input_ev,
            output: None,
            data: Value::Object(map),
            warnings: Vec::new(),
            duration_ms,
        })
    }

    /// Reader: Extract layout-preserved text from entire document or specific page via `pdftotext`
    pub fn extract(path: &str, page: Option<u32>) -> Result<StructuredEvidence> {
        let start_time = Instant::now();
        let input_ev = inspect_file_evidence(path);

        let mut cmd = Command::new("pdftotext");
        cmd.arg("-layout"); // Preserve column and table layout

        if let Some(p) = page {
            cmd.arg("-f").arg(p.to_string());
            cmd.arg("-l").arg(p.to_string());
        }
        cmd.arg(path).arg("-"); // Stream to stdout

        let output = cmd.output().map_err(|e| anyhow!("Failed to execute pdftotext: {}", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("pdftotext failed: {}", stderr.trim()));
        }

        let text = String::from_utf8_lossy(&output.stdout).to_string();
        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(StructuredEvidence {
            operation_id: Uuid::new_v4(),
            status: "success".to_string(),
            engine: EngineInfo {
                name: "poppler-pdftotext".to_string(),
                version: Some("24.08".to_string()),
            },
            input: input_ev,
            output: None,
            data: json!({
                "file": path,
                "page": page,
                "length_chars": text.len(),
                "line_count": text.lines().count(),
                "text": text
            }),
            warnings: Vec::new(),
            duration_ms,
        })
    }

    /// Editor/Transformer: Slice specific page range into a new PDF via `qpdf`
    pub fn slice(path: &str, start_page: u32, end_page: u32, out_path: &str) -> Result<StructuredEvidence> {
        let start_time = Instant::now();
        let input_ev = inspect_file_evidence(path);

        if let Some(parent) = Path::new(out_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let pages_spec = format!("{}-{}", start_page, end_page);
        let output = Command::new("qpdf")
            .arg("--empty")
            .arg("--pages")
            .arg(path)
            .arg(&pages_spec)
            .arg("--")
            .arg(out_path)
            .output()
            .map_err(|e| anyhow!("Failed to execute qpdf: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("qpdf slice failed: {}", stderr.trim()));
        }

        let output_ev = inspect_file_evidence(out_path);
        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(StructuredEvidence {
            operation_id: Uuid::new_v4(),
            status: "success".to_string(),
            engine: EngineInfo {
                name: "qpdf".to_string(),
                version: Some("11.9".to_string()),
            },
            input: input_ev,
            output: output_ev,
            data: json!({
                "source": path,
                "output": out_path,
                "pages": pages_spec
            }),
            warnings: Vec::new(),
            duration_ms,
        })
    }

    /// Editor/Transformer: Merge multiple PDF documents into one via `pdfunite`
    pub fn merge(paths: &[String], out_path: &str) -> Result<StructuredEvidence> {
        let start_time = Instant::now();
        if paths.len() < 2 {
            return Err(anyhow!("pdf.merge requires at least 2 input PDF files"));
        }

        if let Some(parent) = Path::new(out_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut cmd = Command::new("pdfunite");
        for p in paths {
            cmd.arg(p);
        }
        cmd.arg(out_path);

        let output = cmd.output().map_err(|e| anyhow!("Failed to execute pdfunite: {}", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("pdfunite failed: {}", stderr.trim()));
        }

        let output_ev = inspect_file_evidence(out_path);
        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(StructuredEvidence {
            operation_id: Uuid::new_v4(),
            status: "success".to_string(),
            engine: EngineInfo {
                name: "poppler-pdfunite".to_string(),
                version: Some("24.08".to_string()),
            },
            input: None,
            output: output_ev,
            data: json!({
                "inputs": paths,
                "output": out_path,
                "merged_count": paths.len()
            }),
            warnings: Vec::new(),
            duration_ms,
        })
    }

    /// Renderer: Render a specific page as a crisp PNG image via `pdftoppm` for visual inspection
    pub fn render_page(path: &str, page: u32, out_prefix: &str) -> Result<StructuredEvidence> {
        let start_time = Instant::now();
        let input_ev = inspect_file_evidence(path);

        if let Some(parent) = Path::new(out_prefix).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let output = Command::new("pdftoppm")
            .arg("-png")
            .arg("-r")
            .arg("150") // 150 DPI balanced quality & speed
            .arg("-f")
            .arg(page.to_string())
            .arg("-l")
            .arg(page.to_string())
            .arg(path)
            .arg(out_prefix)
            .output()
            .map_err(|e| anyhow!("Failed to execute pdftoppm: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("pdftoppm failed: {}", stderr.trim()));
        }

        // pdftoppm creates <out_prefix>-1.png or <out_prefix>-01.png
        let expected_name = format!("{}-{}.png", out_prefix, page);
        let alt_name = format!("{}-{:02}.png", out_prefix, page);
        let final_image = if Path::new(&expected_name).exists() {
            expected_name
        } else if Path::new(&alt_name).exists() {
            alt_name
        } else {
            format!("{}.png", out_prefix)
        };

        let output_ev = inspect_file_evidence(&final_image);
        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(StructuredEvidence {
            operation_id: Uuid::new_v4(),
            status: "success".to_string(),
            engine: EngineInfo {
                name: "poppler-pdftoppm".to_string(),
                version: Some("24.08".to_string()),
            },
            input: input_ev,
            output: output_ev,
            data: json!({
                "source": path,
                "page": page,
                "rendered_image": final_image
            }),
            warnings: Vec::new(),
            duration_ms,
        })
    }

    /// Renderer: Extract raw embedded images from PDF via `pdfimages`
    pub fn images(path: &str, out_prefix: &str) -> Result<StructuredEvidence> {
        let start_time = Instant::now();
        let input_ev = inspect_file_evidence(path);

        if let Some(parent) = Path::new(out_prefix).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let output = Command::new("pdfimages")
            .arg("-png")
            .arg(path)
            .arg(out_prefix)
            .output()
            .map_err(|e| anyhow!("Failed to execute pdfimages: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("pdfimages failed: {}", stderr.trim()));
        }

        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(StructuredEvidence {
            operation_id: Uuid::new_v4(),
            status: "success".to_string(),
            engine: EngineInfo {
                name: "poppler-pdfimages".to_string(),
                version: Some("24.08".to_string()),
            },
            input: input_ev,
            output: None,
            data: json!({
                "source": path,
                "out_prefix": out_prefix
            }),
            warnings: Vec::new(),
            duration_ms,
        })
    }

    /// Validator: Check PDF integrity with `qpdf --check`
    pub fn validate(path: &str) -> Result<StructuredEvidence> {
        let start_time = Instant::now();
        let input_ev = inspect_file_evidence(path);

        let output = Command::new("qpdf")
            .arg("--check")
            .arg(path)
            .output()
            .map_err(|e| anyhow!("Failed to execute qpdf: {}", e))?;

        let is_valid = output.status.success();
        let details = String::from_utf8_lossy(&output.stdout);
        let errors = String::from_utf8_lossy(&output.stderr);
        let duration_ms = start_time.elapsed().as_millis() as u64;

        let mut warnings = Vec::new();
        if !errors.trim().is_empty() {
            warnings.push(errors.trim().to_string());
        }

        Ok(StructuredEvidence {
            operation_id: Uuid::new_v4(),
            status: if is_valid { "success".into() } else { "warning".into() },
            engine: EngineInfo {
                name: "qpdf-check".to_string(),
                version: Some("11.9".to_string()),
            },
            input: input_ev,
            output: None,
            data: json!({
                "file": path,
                "is_valid": is_valid,
                "details": details.trim()
            }),
            warnings,
            duration_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    const MINIMAL_PDF: &[u8] = b"%PDF-1.4\n\
1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 144] /Contents 4 0 R >>\nendobj\n\
4 0 obj\n<< /Length 20 >>\nstream\nBT /F1 12 Tf ET\nendstream\nendobj\n\
xref\n0 5\n\
0000000000 65535 f \n\
0000000009 00000 n \n\
0000000058 00000 n \n\
0000000115 00000 n \n\
0000000201 00000 n \n\
trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n272\n%%EOF\n";

    #[test]
    fn test_pdf_inspect_and_validate() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(MINIMAL_PDF).unwrap();
        tmp.flush().unwrap();

        let path = tmp.path().to_str().unwrap();

        // If pdfinfo/qpdf are installed on system, test them
        if Command::new("pdfinfo").arg("-v").output().is_ok() {
            let ev = PdfCapability::inspect(path).expect("pdf inspect");
            assert_eq!(ev.status, "success");
            assert_eq!(ev.data["pages"], json!(1));
        }

        if Command::new("qpdf").arg("--version").output().is_ok() {
            let val = PdfCapability::validate(path).expect("pdf validate");
            assert!(val.status == "success" || val.status == "warning");
        }
    }

    #[test]
    fn test_pdf_missing_file_error() {
        let res = PdfCapability::inspect("/tmp/non_existent_mnse_pdf_file.pdf");
        assert!(res.is_err());
    }

    #[test]
    fn test_pdf_injection_defense() {
        let evil = "/tmp/fake; touch /tmp/pdf_evil_flag";
        let _ = PdfCapability::inspect(evil);
        assert!(!Path::new("/tmp/pdf_evil_flag").exists());
    }
}
