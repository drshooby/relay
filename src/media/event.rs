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
        #[serde(
            default,
            rename = "elapsed",
            deserialize_with = "deserialize_optional_u64"
        )]
        elapsed_secs: Option<u64>,
        #[serde(
            default,
            rename = "duration",
            deserialize_with = "deserialize_optional_u64"
        )]
        duration_secs: Option<u64>,
    },
    PositionChanged {
        #[serde(rename = "elapsed", deserialize_with = "deserialize_u64")]
        elapsed_secs: u64,
    },
    PlaybackPaused,
    PlaybackStopped,
    PermissionDenied,
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

fn deserialize_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse::<u64>().map_err(serde::de::Error::custom)
}

fn deserialize_optional_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        Some(value) if value.is_empty() => Ok(None),
        Some(value) => value
            .parse::<u64>()
            .map(Some)
            .map_err(serde::de::Error::custom),
        None => Ok(None),
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
                elapsed_secs: None,
                duration_secs: None,
            }
        );
    }

    #[test]
    fn parses_track_changed_with_elapsed() {
        let line =
            r#"{"event":"track_changed","title":"T","artist":"A","album":"Al","elapsed":"127"}"#;
        let ev = parse_event_line(line).unwrap();
        assert_eq!(
            ev,
            MediaEvent::TrackChanged {
                title: "T".into(),
                artist: "A".into(),
                album: "Al".into(),
                elapsed_secs: Some(127),
                duration_secs: None,
            }
        );
    }

    #[test]
    fn parses_track_changed_with_duration() {
        let line =
            r#"{"event":"track_changed","title":"T","artist":"A","album":"Al","duration":"157"}"#;
        let ev = parse_event_line(line).unwrap();
        assert_eq!(
            ev,
            MediaEvent::TrackChanged {
                title: "T".into(),
                artist: "A".into(),
                album: "Al".into(),
                elapsed_secs: None,
                duration_secs: Some(157),
            }
        );
    }

    #[test]
    fn parses_position_changed_event() {
        let line = r#"{"event":"position_changed","elapsed":"240"}"#;
        let ev = parse_event_line(line).unwrap();
        assert_eq!(ev, MediaEvent::PositionChanged { elapsed_secs: 240 });
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

    #[test]
    fn invalid_elapsed_on_position_changed_returns_none() {
        let line = r#"{"event":"position_changed","elapsed":"not-a-number"}"#;
        let result = parse_event_line(line);
        assert!(result.is_none());
    }

    #[test]
    fn parses_permission_denied_event() {
        let line = r#"{"event":"permission_denied"}"#;
        let ev = parse_event_line(line).unwrap();
        assert_eq!(ev, MediaEvent::PermissionDenied);
    }
}
