use crate::media::event::HelperStatus;

/// State of the tray icon and menu label.
#[derive(Debug, Clone, PartialEq)]
pub enum TrayState {
    /// Currently playing a track
    Playing { title: String, artist: String },
    /// Not playing (idle)
    Idle,
    /// User toggled off
    Disabled,
    /// Error (e.g., helper crashed)
    Error { message: String },
}

impl TrayState {
    /// Returns the menu label for the "Now Playing" item.
    pub fn label(&self) -> String {
        match self {
            TrayState::Playing { title, artist } => {
                format!("Now Playing: {} \u{2014} {}", title, artist)
            }
            TrayState::Idle => "Relay: Idle".to_string(),
            TrayState::Disabled => "Relay: Disabled".to_string(),
            TrayState::Error { message } => format!("Relay: {}", message),
        }
    }

    /// Build label from a HelperStatus (for graceful degradation — Task 11 fully wires this).
    pub fn from_helper_failure(message: String) -> Self {
        TrayState::Error { message }
    }

    /// Convert a HelperStatus to a TrayState (for graceful degradation wiring).
    pub fn from_helper_status(status: &HelperStatus) -> Option<Self> {
        match status {
            HelperStatus::Running => None,
            HelperStatus::Exited { code } => {
                let msg = match code {
                    Some(c) => format!("media access unavailable (exit {})", c),
                    None => "media access unavailable".to_string(),
                };
                Some(TrayState::Error { message: msg })
            }
            HelperStatus::IoError => Some(TrayState::Error {
                message: "media access unavailable".to_string(),
            }),
        }
    }
}

/// Events emitted from tray UI to the Tokio pipeline.
#[derive(Debug, Clone)]
pub enum TrayEvent {
    ToggleEnabled(bool),
    Quit,
}

pub mod event_loop; // Task 12 will implement this

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_helper_status_exited_gives_error() {
        let status = HelperStatus::Exited { code: Some(1) };
        let state = TrayState::from_helper_status(&status).unwrap();
        assert!(matches!(state, TrayState::Error { .. }));
        assert!(state.label().contains("media access unavailable"));
    }

    #[test]
    fn from_helper_status_running_gives_none() {
        let result = TrayState::from_helper_status(&HelperStatus::Running);
        assert!(result.is_none());
    }

    #[test]
    fn label_playing() {
        let state = TrayState::Playing {
            title: "Bohemian Rhapsody".into(),
            artist: "Queen".into(),
        };
        assert_eq!(
            state.label(),
            "Now Playing: Bohemian Rhapsody \u{2014} Queen"
        );
    }

    #[test]
    fn label_idle() {
        assert_eq!(TrayState::Idle.label(), "Relay: Idle");
    }

    #[test]
    fn label_disabled() {
        assert_eq!(TrayState::Disabled.label(), "Relay: Disabled");
    }

    #[test]
    fn label_error() {
        let state = TrayState::Error {
            message: "media access unavailable".into(),
        };
        assert_eq!(state.label(), "Relay: media access unavailable");
    }
}
