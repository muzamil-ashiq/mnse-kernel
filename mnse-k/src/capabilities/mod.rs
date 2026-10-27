//! Capability Registry & Dispatcher for MNSE Kernel & K Language.
//!
//! Provides the bridge between AI agents and local open-source specialist engines
//! (Poppler, QPDF, LibreOffice, Tesseract, Git, etc.), returning cryptographic
//! structured evidence for every execution.

pub mod doc;
pub mod evidence;
pub mod git;
pub mod manifest;
pub mod pdf;

pub use doc::DocCapability;
pub use evidence::{guess_media_type, hash_file, inspect_file_evidence, FileEvidence, StructuredEvidence};
pub use git::GitCapability;
pub use manifest::{global_registry, is_known_namespace, CapabilityManifest, ManifestRegistry};
pub use pdf::PdfCapability;

use anyhow::{anyhow, Result};
use serde_json::Value;

fn val_to_u32(v: &Value) -> Option<u32> {
    v.as_u64()
        .or_else(|| v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64))
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .map(|u| u as u32)
}

fn val_to_usize(v: &Value) -> Option<usize> {
    v.as_u64()
        .or_else(|| v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64))
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .map(|u| u as usize)
}

/// Dispatch capability invocation across namespaces (`pdf.*`, `doc.*`, `git.*`)
pub fn dispatch_capability(namespace: &str, operation: &str, args: &[Value]) -> Result<StructuredEvidence> {
    match namespace {
        "pdf" => match operation {
            "inspect" => {
                let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| anyhow!("pdf.inspect expects (path: string)"))?;
                PdfCapability::inspect(path)
            }
            "extract" => {
                let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| anyhow!("pdf.extract expects (path: string, [page: int])"))?;
                let page = args.get(1).and_then(val_to_u32);
                PdfCapability::extract(path, page)
            }
            "slice" => {
                let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| anyhow!("pdf.slice expects (path, start_page, end_page, out_path)"))?;
                let start = args.get(1).and_then(val_to_u32).ok_or_else(|| anyhow!("missing start_page"))?;
                let end = args.get(2).and_then(val_to_u32).ok_or_else(|| anyhow!("missing end_page"))?;
                let out = args.get(3).and_then(|v| v.as_str()).ok_or_else(|| anyhow!("missing out_path"))?;
                PdfCapability::slice(path, start, end, out)
            }
            "merge" => {
                if args.is_empty() {
                    return Err(anyhow!("pdf.merge expects (paths: [string], out_path: string) or (file1, file2, ..., out_file)"));
                }
                let (paths, out): (Vec<String>, String) = match &args[0] {
                    Value::Array(arr) => {
                        let p: Vec<String> = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                        let o = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| anyhow!("missing out_path"))?.to_string();
                        (p, o)
                    }
                    _ => {
                        if args.len() < 3 {
                            return Err(anyhow!("pdf.merge requires at least 2 input PDF files and 1 output path"));
                        }
                        let out_str = args.last().and_then(|v| v.as_str()).ok_or_else(|| anyhow!("missing out_path"))?.to_string();
                        let mut p = Vec::new();
                        for a in &args[..args.len() - 1] {
                            if let Some(s) = a.as_str() {
                                p.push(s.to_string());
                            }
                        }
                        (p, out_str)
                    }
                };
                PdfCapability::merge(&paths, &out)
            }
            "render_page" | "preview" => {
                let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| anyhow!("pdf.render_page expects (path: string, [page: int], [out_prefix: string])"))?;
                let page = args.get(1).and_then(val_to_u32).unwrap_or(1);
                let out_prefix = args.get(2).and_then(|v| v.as_str()).unwrap_or("/tmp/mnse_page_preview");
                PdfCapability::render_page(path, page, out_prefix)
            }
            "images" => {
                let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| anyhow!("pdf.images expects (path: string, out_prefix: string)"))?;
                let out = args.get(1).and_then(|v| v.as_str()).unwrap_or("img");
                PdfCapability::images(path, out)
            }
            "validate" => {
                let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| anyhow!("pdf.validate expects (path: string)"))?;
                PdfCapability::validate(path)
            }
            _ => Err(anyhow!(
                "Unknown pdf operation: '{}'. Available: inspect, extract, slice, merge, render_page, images, validate",
                operation
            )),
        },

        "doc" => match operation {
            "convert" => {
                let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| anyhow!("doc.convert expects (input_path: string, target_format: string, [out_dir: string])"))?;
                let format = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| anyhow!("missing target_format (e.g. 'pdf', 'txt', 'html')"))?;
                let out_dir = args.get(2).and_then(|v| v.as_str());
                DocCapability::convert(path, format, out_dir)
            }
            "ocr" => {
                let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| anyhow!("doc.ocr expects (image_path: string, [lang: string])"))?;
                let lang = args.get(1).and_then(|v| v.as_str());
                DocCapability::ocr(path, lang)
            }
            "detect_type" | "inspect" => {
                let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| anyhow!("doc.detect_type expects (path: string)"))?;
                DocCapability::detect_type(path)
            }
            _ => Err(anyhow!(
                "Unknown doc operation: '{}'. Available: convert, ocr, detect_type",
                operation
            )),
        },

        "git" => match operation {
            "status" => {
                let dir = args.first().and_then(|v| v.as_str());
                GitCapability::status(dir)
            }
            "diff" => {
                let dir = args.first().and_then(|v| v.as_str());
                GitCapability::diff(dir)
            }
            "log" => {
                let n = args.first().and_then(val_to_usize).unwrap_or(5);
                let dir = args.get(1).and_then(|v| v.as_str());
                GitCapability::log(n, dir)
            }
            _ => Err(anyhow!(
                "Unknown git operation: '{}'. Available: status, diff, log",
                operation
            )),
        },

        _ => {
            if let Some(evidence) = global_registry().dispatch(namespace, operation, args)? {
                Ok(evidence)
            } else {
                Err(anyhow!(
                    "Unknown capability namespace: '{}'. Available built-ins: pdf, doc, git, or register manifest in ~/.mnse/capabilities.d/*.toml",
                    namespace
                ))
            }
        }
    }
}
