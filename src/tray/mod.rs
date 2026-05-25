use crate::constants::TRAY_ERROR_HELPER_MESSAGE;
use crate::media::event::HelperStatus;

pub mod event_loop;
pub mod icons;

/// State of the tray icon and menu label.
#[derive(Debug, Clone, PartialEq)]
pub enum TrayState {
    /// Currently playing a track
    Playing { title: String, artist: String },
    /// Not playing (idle)
    Idle,
    /// Error (helper or Discord failure)
    Error { message: String, detail: String },
}

impl TrayState {
    /// Returns the menu label for the status line.
    pub fn label(&self) -> String {
        match self {
            TrayState::Playing { title, artist } => {
                format!("Now Playing: {} \u{2014} {}", title, artist)
            }
            TrayState::Idle => "Relay: Idle".to_string(),
            TrayState::Error { message, .. } => format!("Relay: {}", message),
        }
    }

    /// Mini-debug line shown below the status when in error (menu only).
    pub fn error_detail(&self) -> Option<&str> {
        match self {
            TrayState::Error { detail, .. } => Some(detail.as_str()),
            _ => None,
        }
    }

    pub fn icon_variant(&self) -> icons::TrayIconVariant {
        match self {
            TrayState::Error { .. } => icons::TrayIconVariant::Error,
            TrayState::Playing { .. } | TrayState::Idle => icons::TrayIconVariant::Normal,
        }
    }

    /// Convert a HelperStatus to a TrayState (for graceful degradation wiring).
    pub fn from_helper_status(status: &HelperStatus) -> Option<Self> {
        match status {
            HelperStatus::Running => None,
            HelperStatus::Exited { code } => {
                let detail = match code {
                    Some(c) => format!("helper exited with code {}", c),
                    None => "helper exited".to_string(),
                };
                Some(TrayState::Error {
                    message: TRAY_ERROR_HELPER_MESSAGE.to_string(),
                    detail,
                })
            }
            HelperStatus::IoError => Some(TrayState::Error {
                message: TRAY_ERROR_HELPER_MESSAGE.to_string(),
                detail: "helper io error".to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_helper_status_exited_gives_error() {
        let status = HelperStatus::Exited { code: Some(1) };
        let state = TrayState::from_helper_status(&status).unwrap();
        assert!(matches!(state, TrayState::Error { .. }));
        assert!(state.label().contains(TRAY_ERROR_HELPER_MESSAGE));
        assert_eq!(state.error_detail(), Some("helper exited with code 1"));
    }

    #[test]
    fn from_helper_status_running_gives_none() {
        let result = TrayState::from_helper_status(&HelperStatus::Running);
        assert!(result.is_none());
    }

    #[test]
    fn icon_variant_mapping() {
        assert_eq!(
            TrayState::Idle.icon_variant(),
            icons::TrayIconVariant::Normal
        );
        let err = TrayState::Error {
            message: "x".into(),
            detail: "y".into(),
        };
        assert_eq!(err.icon_variant(), icons::TrayIconVariant::Error);
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
    fn label_error() {
        let state = TrayState::Error {
            message: TRAY_ERROR_HELPER_MESSAGE.into(),
            detail: "helper exited".into(),
        };
        assert_eq!(
            state.label(),
            format!("Relay: {}", TRAY_ERROR_HELPER_MESSAGE)
        );
    }
}
