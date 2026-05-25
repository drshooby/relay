use discord_rich_presence::activity::{
    Activity, ActivityType, Assets, Button, StatusDisplayType, Timestamps,
};

use crate::constants::{
    DISCORD_ACTIVITY_NAME, DISCORD_ASSET_DEFAULT_ART, DISCORD_ASSET_RELAY_BADGE,
    DISCORD_ASSET_RELAY_BADGE_TEXT, DISCORD_BUTTON_LISTEN_LABEL,
};

#[derive(Debug, Clone)]
pub struct TrackInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
}

/// Compute Discord activity `start` timestamp from elapsed playback position.
///
/// - `debounce_ms`: extra elapsed to account for track-change debounce delay.
/// - Returns `now_secs` when elapsed is unknown.
pub fn compute_started_at(now_secs: i64, elapsed_secs: Option<u64>, debounce_ms: u64) -> i64 {
    match elapsed_secs {
        Some(elapsed) => {
            let compensated_secs = elapsed.saturating_mul(1000).saturating_add(debounce_ms) / 1000;
            now_secs.saturating_sub(compensated_secs as i64)
        }
        None => now_secs,
    }
}

/// Compute Discord activity `end` timestamp for the progress bar.
pub fn compute_ended_at(started_at: i64, duration_secs: Option<u64>) -> Option<i64> {
    duration_secs
        .filter(|&d| d > 0)
        .map(|d| started_at.saturating_add(d as i64))
}

/// Build a Discord activity payload for a playing track.
///
/// - `details`: track title (primary line).
/// - `state`: artist name (secondary line).
/// - `artwork_url`: optional URL for large image (600x600).
/// - `track_url`: optional Apple Music link for the listen button.
/// - `started_at` / `duration_secs`: when both are set, Discord shows a progress bar.
pub fn build_activity(
    track: &TrackInfo,
    artwork_url: Option<&str>,
    track_url: Option<&str>,
    started_at: i64,
    duration_secs: Option<u64>,
) -> Activity<'static> {
    let mut timestamps = Timestamps::new().start(started_at);
    if let Some(end) = compute_ended_at(started_at, duration_secs) {
        timestamps = timestamps.end(end);
    }

    let large_image = artwork_url.unwrap_or(DISCORD_ASSET_DEFAULT_ART).to_owned();

    let assets = Assets::new()
        .large_image(large_image)
        .large_text(track.album.clone())
        .small_image(DISCORD_ASSET_RELAY_BADGE.to_owned())
        .small_text(DISCORD_ASSET_RELAY_BADGE_TEXT.to_owned());

    let mut activity = Activity::new()
        .name(DISCORD_ACTIVITY_NAME.to_owned())
        .details(track.title.clone())
        .state(track.artist.clone())
        .timestamps(timestamps)
        .assets(assets)
        .activity_type(ActivityType::Listening)
        .status_display_type(StatusDisplayType::Name);

    if let Some(url) = track_url.filter(|u| !u.is_empty()) {
        activity = activity.buttons(vec![Button::new(
            DISCORD_BUTTON_LISTEN_LABEL.to_owned(),
            url.to_owned(),
        )]);
    }

    activity
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_track() -> TrackInfo {
        TrackInfo {
            title: "Bohemian Rhapsody".into(),
            artist: "Queen".into(),
            album: "A Night at the Opera".into(),
        }
    }

    #[test]
    fn build_activity_with_artwork_does_not_panic() {
        let track = sample_track();
        let _activity = build_activity(
            &track,
            Some("https://example.com/art.jpg"),
            None,
            1_000_000,
            Some(300),
        );
    }

    #[test]
    fn build_activity_with_no_artwork_does_not_panic() {
        let track = sample_track();
        let _activity = build_activity(&track, None, None, 1_000_000, None);
    }

    #[test]
    fn build_activity_uses_listening_type() {
        let track = sample_track();
        let activity = build_activity(&track, None, None, 1_000_000, None);
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        assert_eq!(
            json.get("type").and_then(|v| v.as_u64()),
            Some(2),
            "activity_type should be 2 (Listening)"
        );
    }

    #[test]
    fn build_activity_details_is_track_title() {
        let track = sample_track();
        let activity = build_activity(&track, None, None, 1_000_000, None);
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        assert_eq!(
            json.get("details").and_then(|v| v.as_str()),
            Some("Bohemian Rhapsody")
        );
    }

    #[test]
    fn build_activity_state_is_artist_only() {
        let track = sample_track();
        let activity = build_activity(&track, None, None, 1_000_000, None);
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        assert_eq!(json.get("state").and_then(|v| v.as_str()), Some("Queen"));
    }

    #[test]
    fn build_activity_includes_progress_bar_timestamps() {
        let track = sample_track();
        let activity = build_activity(&track, None, None, 1_000, Some(157));
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        let timestamps = json
            .get("timestamps")
            .expect("timestamps should be present");
        assert_eq!(
            timestamps.get("start").and_then(|v| v.as_i64()),
            Some(1_000)
        );
        assert_eq!(timestamps.get("end").and_then(|v| v.as_i64()), Some(1_157));
    }

    #[test]
    fn build_activity_omits_end_without_duration() {
        let track = sample_track();
        let activity = build_activity(&track, None, None, 1_000, None);
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        let timestamps = json
            .get("timestamps")
            .expect("timestamps should be present");
        assert_eq!(
            timestamps.get("start").and_then(|v| v.as_i64()),
            Some(1_000)
        );
        assert!(timestamps.get("end").is_none());
    }

    #[test]
    fn build_activity_includes_relay_badge_overlay() {
        let track = sample_track();
        let activity = build_activity(
            &track,
            Some("https://example.com/art.jpg"),
            None,
            1_000_000,
            None,
        );
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        let assets = json.get("assets").expect("assets should be present");
        assert_eq!(
            assets.get("small_image").and_then(|v| v.as_str()),
            Some(DISCORD_ASSET_RELAY_BADGE)
        );
        assert_eq!(
            assets.get("small_text").and_then(|v| v.as_str()),
            Some(DISCORD_ASSET_RELAY_BADGE_TEXT)
        );
    }

    #[test]
    fn build_activity_uses_apple_music_name() {
        let track = sample_track();
        let activity = build_activity(&track, None, None, 1_000_000, None);
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        assert_eq!(
            json.get("name").and_then(|v| v.as_str()),
            Some(DISCORD_ACTIVITY_NAME)
        );
        assert_eq!(
            json.get("status_display_type").and_then(|v| v.as_u64()),
            Some(0),
            "status_display_type should be Name (Listening to Apple Music in member list)"
        );
    }

    #[test]
    fn build_activity_includes_button_when_track_url_present() {
        let track = sample_track();
        let activity = build_activity(
            &track,
            None,
            Some("https://music.apple.com/us/album/example"),
            1_000_000,
            None,
        );
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        let buttons = json.get("buttons").and_then(|v| v.as_array()).unwrap();
        assert_eq!(buttons.len(), 1);
        assert_eq!(
            buttons[0].get("label").and_then(|v| v.as_str()),
            Some(DISCORD_BUTTON_LISTEN_LABEL)
        );
    }

    #[test]
    fn build_activity_omits_button_when_track_url_absent() {
        let track = sample_track();
        let activity = build_activity(&track, None, None, 1_000_000, None);
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        assert!(json.get("buttons").is_none());
    }

    #[test]
    fn compute_started_at_with_elapsed_and_debounce() {
        let started = compute_started_at(1_000, Some(127), 1_000);
        assert_eq!(started, 872);
    }

    #[test]
    fn compute_started_at_compensates_fractional_debounce_ms() {
        let started = compute_started_at(1_000, Some(127), 1_500);
        assert_eq!(started, 872);
    }

    #[test]
    fn compute_started_at_without_elapsed_uses_now() {
        let started = compute_started_at(1_000, None, 1_500);
        assert_eq!(started, 1_000);
    }

    #[test]
    fn compute_ended_at_adds_duration_to_start() {
        assert_eq!(compute_ended_at(1_000, Some(157)), Some(1_157));
    }

    #[test]
    fn compute_ended_at_none_for_zero_duration() {
        assert_eq!(compute_ended_at(1_000, Some(0)), None);
        assert_eq!(compute_ended_at(1_000, None), None);
    }
}
