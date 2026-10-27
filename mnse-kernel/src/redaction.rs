//! Zero Trust PII Redaction Engine.
//!
//! Enforces ADR 002: Hardens privacy by default. Redacts window titles, editor selections,
//! clipboard previews, and browser URLs when privacy mode is active.

use mnse_types::{ContextSnapshot, Event, EventType};

pub struct Redactor {
    pub privacy_mode: bool,
}

impl Redactor {
    pub fn new(privacy_mode: bool) -> Self {
        Self { privacy_mode }
    }

    /// Apply redaction rules in-place to an incoming Event
    pub fn redact_event(&self, event: &mut Event) {
        if !self.privacy_mode {
            return;
        }

        match &mut event.event_type {
            EventType::WindowFocused { window_title, .. } => {
                *window_title = "[REDACTED:TITLE]".to_string();
            }

            EventType::ClipboardChanged { content_preview, .. } => {
                *content_preview = Some("[REDACTED:CLIPBOARD]".to_string());
            }

            EventType::SelectionChanged { text_hash, char_count } => {
                *text_hash = Some("[REDACTED:SELECTION]".to_string());
                *char_count = 0;
            }

            EventType::ProcessExecuted { command, stdout_tail, stderr_tail, .. } => {
                *command = Self::redact_text(command);
                if let Some(out) = stdout_tail {
                    *out = Self::redact_text(out);
                }
                if let Some(err) = stderr_tail {
                    *err = Self::redact_text(err);
                }
            }

            _ => {}
        }
    }

    /// Redact sensitive text (API keys, AWS keys, GitHub tokens, Bearer tokens, private keys, passwords)
    pub fn redact_text(input: &str) -> String {
        if input.is_empty() {
            return String::new();
        }

        let mut text = input.to_string();

        // 1. Redact Private Key blocks: -----BEGIN ... PRIVATE KEY----- ... -----END ... PRIVATE KEY-----
        while let Some(start) = text.find("-----BEGIN ") {
            if let Some(end_marker) = text[start..].find("-----END ") {
                let tail = &text[start + end_marker..];
                if let Some(dashes) = tail[9..].find("-----") {
                    let end_pos = start + end_marker + 9 + dashes + 5;
                    text.replace_range(start..end_pos, "[REDACTED:PRIVATE_KEY]");
                    continue;
                }
            }
            break;
        }

        // 2. Scan and redact token patterns
        let mut result = String::with_capacity(text.len());
        let words = text.split_inclusive(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '"' || c == '\'');

        for part in words {
            // Check for URL with credentials: https://user:pass@host
            if part.contains("://") && part.contains('@') {
                if let Some(proto_end) = part.find("://") {
                    if let Some(at_idx) = part.find('@') {
                        let user_pass = &part[proto_end + 3..at_idx];
                        if let Some(colon_idx) = user_pass.find(':') {
                            let clean = format!("{}{}:[REDACTED:PASSWORD]{}", &part[..proto_end + 3 + colon_idx], "", &part[at_idx..]);
                            result.push_str(&clean);
                            continue;
                        }
                    }
                }
            }

            // Check for key=value secret patterns: api_key=..., token=..., password=..., secret=...
            let lower = part.to_ascii_lowercase();
            let mut matched_secret_assign = false;
            for prefix in &["api_key=", "apikey=", "api-key=", "token=", "secret=", "password=", "secret_key="] {
                if let Some(idx) = lower.find(prefix) {
                    let val_start = idx + prefix.len();
                    if val_start < part.len() {
                        let val = &part[val_start..];
                        let val_trimmed = val.trim_end_matches(|c: char| c.is_whitespace() || c == ';' || c == '"' || c == '\'');
                        let trailing = &val[val_trimmed.len()..];

                        if val_trimmed.starts_with("[REDACTED:") {
                            result.push_str(part);
                            matched_secret_assign = true;
                            break;
                        }

                        if val_trimmed.trim_matches(|c: char| c == '"' || c == '\'').len() >= 4 {
                            let prefix_str = &part[..val_start];
                            result.push_str(prefix_str);
                            result.push_str("[REDACTED:SECRET]");
                            result.push_str(trailing);
                            matched_secret_assign = true;
                            break;
                        }
                    }
                }
            }
            if matched_secret_assign {
                continue;
            }

            // Check for Bearer token: "Bearer eyJhbG..."
            if lower.starts_with("bearer ") {
                let token_part = &part[7..];
                let token_trimmed = token_part.trim_end_matches(|c: char| c.is_whitespace() || c == ';' || c == '"' || c == '\'');
                let trailing = &token_part[token_trimmed.len()..];
                if token_trimmed == "[REDACTED:TOKEN]" {
                    result.push_str(part);
                    continue;
                }
                if token_trimmed.len() >= 16 {
                    result.push_str("Bearer [REDACTED:TOKEN]");
                    result.push_str(trailing);
                    continue;
                }
            }

            // Check for AWS Access Key ID: AKIA[0-9A-Z]{16}
            if let Some(akia_idx) = part.find("AKIA") {
                if part.len() >= akia_idx + 20 {
                    let candidate = &part[akia_idx..akia_idx + 20];
                    if candidate[4..].chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
                        let redacted = format!("{}[REDACTED:AWS_KEY]{}", &part[..akia_idx], &part[akia_idx + 20..]);
                        result.push_str(&redacted);
                        continue;
                    }
                }
            }

            // Check for GitHub personal access tokens: ghp_, gho_, ghu_, ghs_, ghr_
            let mut matched_gh = false;
            for gh_prefix in &["ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
                if let Some(gh_idx) = part.find(gh_prefix) {
                    if part.len() >= gh_idx + 40 {
                        let candidate = &part[gh_idx..gh_idx + 40];
                        if candidate[4..].chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                            let redacted = format!("{}[REDACTED:GITHUB_TOKEN]{}", &part[..gh_idx], &part[gh_idx + 40..]);
                            result.push_str(&redacted);
                            matched_gh = true;
                            break;
                        }
                    }
                }
            }
            if matched_gh {
                continue;
            }

            result.push_str(part);
        }

        result
    }

    /// Apply redaction rules to a ContextSnapshot
    pub fn redact_snapshot(&self, snapshot: &mut ContextSnapshot) {
        if !snapshot.privacy_mode {
            return;
        }

        if snapshot.window_title.is_some() {
            snapshot.window_title = Some("[REDACTED:TITLE]".to_string());
        }

        if snapshot.clipboard_preview.is_some() {
            snapshot.clipboard_preview = Some("[REDACTED:CLIPBOARD]".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_redaction() {
        let redactor = Redactor::new(true);
        let mut event = Event::new(
            EventType::WindowFocused {
                app_name: "zed".into(),
                window_title: "Secret File.rs".into(),
                window_class: "zed".into(),
                window_id: "1".into(),
                pid: None,
                workspace: None,
            },
            "os",
        );

        redactor.redact_event(&mut event);

        if let EventType::WindowFocused { window_title, .. } = &event.event_type {
            assert_eq!(window_title, "[REDACTED:TITLE]");
        } else {
            panic!("Event type changed");
        }
    }

    #[test]
    fn test_text_redaction_secrets() {
        // AWS Access Key
        let s1 = "export AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE and deploy";
        let r1 = Redactor::redact_text(s1);
        assert!(!r1.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(r1.contains("[REDACTED:AWS_KEY]"));

        // GitHub Token
        let s2 = "curl -H 'Authorization: token ghp_1234567890abcdefghijklmnopqrstuvwxyzAB' https://api.github.com";
        let r2 = Redactor::redact_text(s2);
        assert!(!r2.contains("ghp_1234567890abcdefghijklmnopqrstuvwxyzAB"));
        assert!(r2.contains("[REDACTED:GITHUB_TOKEN]"));

        // Private Key
        let s3 = "my cert: -----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA0\n-----END RSA PRIVATE KEY----- end";
        let r3 = Redactor::redact_text(s3);
        assert!(!r3.contains("MIIEowIBAAKCAQEA0"));
        assert!(r3.contains("[REDACTED:PRIVATE_KEY]"));

        // URL Credentials
        let s4 = "git clone https://alice:secretpassword999@gitlab.com/group/repo.git";
        let r4 = Redactor::redact_text(s4);
        assert!(!r4.contains("secretpassword999"));
        assert!(r4.contains("[REDACTED:PASSWORD]"));

        // Assignment Secret
        let s5 = "export api_key=sk_live_998877665544332211";
        let r5 = Redactor::redact_text(s5);
        assert!(!r5.contains("sk_live_998877665544332211"));
        assert!(r5.contains("[REDACTED:SECRET]"));
    }

    #[test]
    fn test_redactor_invariants_fuzzing() {
        let secrets = [
            "AKIAIOSFODNN7EXAMPLE",
            "ghp_abcdefghijklmnopqrstuvwxyz1234567890",
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\n-----END RSA PRIVATE KEY-----",
            "https://user:verysecretpass123@example.com",
            "api_key=super_secret_production_key_xyz",
        ];

        // Property Invariant 1: Injected secrets are NEVER leaked in output
        for secret in &secrets {
            let wrapped = format!("prefix noise {} suffix noise", secret);
            let redacted = Redactor::redact_text(&wrapped);
            if secret.starts_with("https://user:") {
                assert!(!redacted.contains("verysecretpass123"));
            } else if secret.starts_with("api_key=") {
                assert!(!redacted.contains("super_secret_production_key_xyz"));
            } else {
                assert!(!redacted.contains(*secret));
            }

            // Property Invariant 2: Redaction is strictly idempotent
            let double_redacted = Redactor::redact_text(&redacted);
            assert_eq!(redacted, double_redacted, "Redaction must be idempotent");
        }

        // Property Invariant 3: Zero panics on arbitrary pseudo-random strings
        let mut seed: u64 = 0xDEADBEEF;
        for _ in 0..1000 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let mut pseudo_rand = String::new();
            for j in 0..30 {
                let byte = ((seed >> (j % 32)) & 0xFF) as u8;
                if byte.is_ascii_graphic() || byte == b' ' || byte == b'\n' {
                    pseudo_rand.push(byte as char);
                }
            }
            let res = Redactor::redact_text(&pseudo_rand);
            let idemp = Redactor::redact_text(&res);
            assert_eq!(res, idemp);
        }
    }
}
