use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum MediaEvent {
    TrackChanged {
        title: String,
        #[serde(default)]
        artist: String,
        #[serde(default)]
        album: String,
    },
    PlaybackPaused,
    PlaybackStopped,
}

#[derive(Debug)]
pub enum HelperStatus {
    Running,
    Exited { code: Option<i32> },
    IoError,
}

/// Commands sent from Rust to the Swift helper over stdin (newline-delimited JSON).
#[derive(Debug, Clone)]
pub enum HelperCommand {
    /// Re-query Music.app's current state and emit a corresponding event.
    Refresh,
}

impl HelperCommand {
    /// Serialise to the on-wire JSON line (including trailing newline).
    pub fn to_json_line(&self) -> &'static str {
        match self {
            HelperCommand::Refresh => "{\"command\":\"refresh\"}\n",
        }
    }
}

/// Parse a single line of JSON into a MediaEvent.
/// Returns None for unknown event types (forward compat) or malformed JSON (logs warning).
pub fn parse_event_line(line: &str) -> Option<MediaEvent> {
    match serde_json::from_str::<MediaEvent>(line) {
        Ok(event) => Some(event),
        Err(e) => {
            // Log warning but don't crash — malformed JSON or unknown event variant
            tracing::warn!("failed to parse event line: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_track_changed_event() {
        let line = r#"{"event":"track_changed","title":"T","artist":"A","album":"Al"}"#;
        let ev = parse_event_line(line).unwrap();
        assert_eq!(
            ev,
            MediaEvent::TrackChanged {
                title: "T".into(),
                artist: "A".into(),
                album: "Al".into(),
            }
        );
    }

    #[test]
    fn malformed_json_returns_none() {
        let result = parse_event_line("{not json}");
        assert!(result.is_none());
    }

    #[test]
    fn unknown_event_variant_returns_none() {
        let line = r#"{"event":"unknown_future_event","data":"x"}"#;
        let result = parse_event_line(line);
        assert!(result.is_none());
    }
}
