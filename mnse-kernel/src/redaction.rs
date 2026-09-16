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

            EventType::UrlChanged { url, title, .. } => {
                *url = "[REDACTED:URL]".to_string();
                *title = Some("[REDACTED:TITLE]".to_string());
            }

            EventType::TabSwitched { url, title, .. } => {
                *url = "[REDACTED:URL]".to_string();
                *title = Some("[REDACTED:TITLE]".to_string());
            }

            EventType::TabUpdated { url, title, .. } => {
                *url = "[REDACTED:URL]".to_string();
                *title = Some("[REDACTED:TITLE]".to_string());
            }

            EventType::SelectionChanged { text_hash, char_count } => {
                *text_hash = Some("[REDACTED:SELECTION]".to_string());
                *char_count = 0;
            }

            _ => {}
        }
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

        if snapshot.browser_url.is_some() {
            snapshot.browser_url = Some("[REDACTED:URL]".to_string());
        }

        if snapshot.browser_title.is_some() {
            snapshot.browser_title = Some("[REDACTED:TITLE]".to_string());
        }

        if snapshot.browser_search_query.is_some() {
            snapshot.browser_search_query = Some("[REDACTED:SEARCH]".to_string());
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
}
