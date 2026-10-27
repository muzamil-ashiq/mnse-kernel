//! Hermetic Enterprise Integration Tests for MNSE Capabilities & Evidence Pipeline.

use mnse_k::capabilities::{dispatch_capability, inspect_file_evidence, StructuredEvidence};
use serde_json::json;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_dispatch_doc_detect_type_integration() {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(b"{\"mnse\": \"v2.0\", \"status\": \"online\"}\n").unwrap();
    tmp.flush().unwrap();

    let path_str = tmp.path().to_str().unwrap();
    let ev = dispatch_capability("doc", "detect_type", &[json!(path_str)]).expect("dispatch doc.detect_type");

    assert_eq!(ev.status, "success");
    assert_eq!(ev.engine.name, "linux-file-mime");
    assert!(ev.input.is_some());
    let input_ev = ev.input.unwrap();
    assert_eq!(input_ev.path, path_str);
    assert!(!input_ev.sha256.is_empty());
}

#[test]
fn test_dispatch_unknown_namespace_and_operation() {
    let err_ns = dispatch_capability("invalid_ns", "op", &[]);
    assert!(err_ns.is_err());
    assert!(err_ns.unwrap_err().to_string().contains("Unknown capability namespace"));

    let err_op = dispatch_capability("doc", "invalid_op", &[]);
    assert!(err_op.is_err());
    assert!(err_op.unwrap_err().to_string().contains("Unknown doc operation"));
}

#[test]
fn test_structured_evidence_serialization_roundtrip() {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(b"sample payload for evidence\n").unwrap();
    tmp.flush().unwrap();

    let ev = inspect_file_evidence(tmp.path()).unwrap();
    let structured = StructuredEvidence {
        operation_id: uuid::Uuid::new_v4(),
        status: "success".into(),
        engine: mnse_k::capabilities::evidence::EngineInfo {
            name: "test-engine".into(),
            version: Some("1.0.0".into()),
        },
        input: Some(ev),
        output: None,
        data: json!({ "processed": true, "items": [1, 2, 3] }),
        warnings: vec!["test warning".into()],
        duration_ms: 42,
    };

    let serialized = serde_json::to_string(&structured).expect("serialize evidence");
    let deserialized: StructuredEvidence = serde_json::from_str(&serialized).expect("deserialize evidence");

    assert_eq!(deserialized.status, "success");
    assert_eq!(deserialized.engine.name, "test-engine");
    assert_eq!(deserialized.warnings.len(), 1);
    assert_eq!(deserialized.duration_ms, 42);
    assert_eq!(deserialized.data["items"][0], 1);
}

#[tokio::test]
async fn test_dynamic_manifest_k_end_to_end() {
    let toml_manifest = r#"
[capability]
namespace = "sensor"
description = "Sensory probing capability"
version = "3.0.0"

[operations.echo]
description = "Echoes value"
command = "echo"
args = ["-n", "{0}"]
"#;

    mnse_k::capabilities::global_registry()
        .load_manifest_str(toml_manifest)
        .expect("load manifest");

    assert!(mnse_k::capabilities::is_known_namespace("sensor"));

    // Evaluate full K code calling the dynamically loaded capability
    let res = mnse_k::execute("sensor.echo \"hello_dynamic_k\"").await.expect("execute dynamic capability in K");
    assert_eq!(res["status"], "success");
    assert_eq!(res["engine"]["name"], "dynamic:sensor");
    assert_eq!(res["data"]["output"], "hello_dynamic_k");
}
