use discord_rich_presence::activity::{
    Activity, ActivityType, Assets, Button, StatusDisplayType, Timestamps,
};

use crate::config::DisplayConfig;
use crate::constants::{
    DISCORD_ACTIVITY_NAME, DISCORD_ASSET_RELAY_BADGE, DISCORD_ASSET_RELAY_BADGE_TEXT,
    DISCORD_BUTTON_LISTEN_LABEL, DISCORD_PRIVATE_TITLE,
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
/// - `track`: track metadata (title, artist, album).
/// - `artwork_url`: optional URL for large image (600x600). When `None`, uses
///   `DISCORD_ASSET_RELAY_BADGE` as the large image and omits the small overlay.
/// - `track_url`: optional Apple Music link for the listen button.
/// - `started_at` / `duration_secs`: used for the progress bar.
/// - `display`: per-field visibility toggles. Defaults (all `true`) reproduce the
///   pre-#32 behaviour.
pub fn build_activity(
    track: &TrackInfo,
    artwork_url: Option<&str>,
    track_url: Option<&str>,
    started_at: i64,
    duration_secs: Option<u64>,
    display: &DisplayConfig,
) -> Activity<'static> {
    // Apply display filters.
    // show_artwork = false: force badge as large image, omit small image.
    let effective_artwork = if display.show_artwork {
        artwork_url
    } else {
        None
    };

    let mut assets = match effective_artwork {
        Some(url) => Assets::new()
            .large_image(url.to_owned())
            .small_image(DISCORD_ASSET_RELAY_BADGE.to_owned())
            .small_text(DISCORD_ASSET_RELAY_BADGE_TEXT.to_owned()),
        None => Assets::new().large_image(DISCORD_ASSET_RELAY_BADGE.to_owned()),
    };

    // show_album = false: skip large_text (album tooltip).
    if display.show_album {
        assets = assets.large_text(track.album.clone());
    }

    // show_title = false: replace details with generic privacy label.
    let details = if display.show_title {
        track.title.clone()
    } else {
        DISCORD_PRIVATE_TITLE.to_owned()
    };

    let mut activity = Activity::new()
        .name(DISCORD_ACTIVITY_NAME.to_owned())
        .details(details)
        .assets(assets)
        .activity_type(ActivityType::Listening)
        .status_display_type(StatusDisplayType::Name);

    // show_artist = false: skip .state() call entirely.
    if display.show_artist {
        activity = activity.state(track.artist.clone());
    }

    let mut timestamps = Timestamps::new().start(started_at);
    if let Some(end) = compute_ended_at(started_at, duration_secs) {
        timestamps = timestamps.end(end);
    }
    activity = activity.timestamps(timestamps);

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
    use crate::config::DisplayConfig;

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
            &DisplayConfig::default(),
        );
    }

    #[test]
    fn build_activity_with_no_artwork_does_not_panic() {
        let track = sample_track();
        let _activity = build_activity(
            &track,
            None,
            None,
            1_000_000,
            None,
            &DisplayConfig::default(),
        );
    }

    #[test]
    fn build_activity_uses_listening_type() {
        let track = sample_track();
        let activity = build_activity(
            &track,
            None,
            None,
            1_000_000,
            None,
            &DisplayConfig::default(),
        );
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
        let activity = build_activity(
            &track,
            None,
            None,
            1_000_000,
            None,
            &DisplayConfig::default(),
        );
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        assert_eq!(
            json.get("details").and_then(|v| v.as_str()),
            Some("Bohemian Rhapsody")
        );
    }

    #[test]
    fn build_activity_state_is_artist_only() {
        let track = sample_track();
        let activity = build_activity(
            &track,
            None,
            None,
            1_000_000,
            None,
            &DisplayConfig::default(),
        );
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        assert_eq!(json.get("state").and_then(|v| v.as_str()), Some("Queen"));
    }

    #[test]
    fn build_activity_includes_progress_bar_timestamps() {
        let track = sample_track();
        let activity = build_activity(
            &track,
            None,
            None,
            1_000,
            Some(157),
            &DisplayConfig::default(),
        );
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
        let activity = build_activity(&track, None, None, 1_000, None, &DisplayConfig::default());
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
            &DisplayConfig::default(),
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
        let activity = build_activity(
            &track,
            None,
            None,
            1_000_000,
            None,
            &DisplayConfig::default(),
        );
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
            &DisplayConfig::default(),
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
        let activity = build_activity(
            &track,
            None,
            None,
            1_000_000,
            None,
            &DisplayConfig::default(),
        );
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        assert!(json.get("buttons").is_none());
    }

    // --- #34 artwork fallback tests ---

    #[test]
    fn build_activity_no_artwork_uses_badge_as_large_and_omits_small() {
        let track = sample_track();
        let activity = build_activity(
            &track,
            None,
            None,
            1_000_000,
            None,
            &DisplayConfig::default(),
        );
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        let assets = json.get("assets").expect("assets should be present");
        assert_eq!(
            assets.get("large_image").and_then(|v| v.as_str()),
            Some(DISCORD_ASSET_RELAY_BADGE),
            "large_image should be relay badge when no artwork"
        );
        assert!(
            assets.get("small_image").is_none(),
            "small_image must be absent when no artwork (no doubled branding)"
        );
        assert!(
            assets.get("small_text").is_none(),
            "small_text must be absent when no artwork"
        );
    }

    #[test]
    fn build_activity_with_artwork_keeps_small_badge() {
        let track = sample_track();
        let activity = build_activity(
            &track,
            Some("https://example.com/art.jpg"),
            None,
            1_000_000,
            None,
            &DisplayConfig::default(),
        );
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        let assets = json.get("assets").expect("assets should be present");
        assert_eq!(
            assets.get("large_image").and_then(|v| v.as_str()),
            Some("https://example.com/art.jpg"),
            "large_image should be artwork URL when provided"
        );
        assert_eq!(
            assets.get("small_image").and_then(|v| v.as_str()),
            Some(DISCORD_ASSET_RELAY_BADGE),
            "small_image should be relay badge overlay when artwork is present"
        );
    }

    // --- #32 display toggle tests ---

    #[test]
    fn build_activity_show_title_false_uses_private_label() {
        let track = sample_track();
        let display = DisplayConfig {
            show_title: false,
            ..DisplayConfig::default()
        };
        let activity = build_activity(&track, None, None, 1_000_000, None, &display);
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        assert_eq!(
            json.get("details").and_then(|v| v.as_str()),
            Some(DISCORD_PRIVATE_TITLE),
            "details should be private label when show_title is false"
        );
    }

    #[test]
    fn build_activity_show_artist_false_omits_state() {
        let track = sample_track();
        let display = DisplayConfig {
            show_artist: false,
            ..DisplayConfig::default()
        };
        let activity = build_activity(&track, None, None, 1_000_000, None, &display);
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        assert!(
            json.get("state").is_none(),
            "state field should be absent when show_artist is false"
        );
    }

    #[test]
    fn build_activity_show_album_false_omits_large_text() {
        let track = sample_track();
        let display = DisplayConfig {
            show_album: false,
            ..DisplayConfig::default()
        };
        let activity = build_activity(
            &track,
            Some("https://example.com/art.jpg"),
            None,
            1_000_000,
            None,
            &display,
        );
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        let assets = json.get("assets").expect("assets should be present");
        assert!(
            assets.get("large_text").is_none(),
            "large_text should be absent when show_album is false"
        );
    }

    #[test]
    fn build_activity_show_artwork_false_uses_badge_as_large() {
        let track = sample_track();
        let display = DisplayConfig {
            show_artwork: false,
            ..DisplayConfig::default()
        };
        // Pass an artwork URL — it should be ignored when show_artwork is false.
        let activity = build_activity(
            &track,
            Some("https://example.com/art.jpg"),
            None,
            1_000_000,
            None,
            &display,
        );
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        let assets = json.get("assets").expect("assets should be present");
        assert_eq!(
            assets.get("large_image").and_then(|v| v.as_str()),
            Some(DISCORD_ASSET_RELAY_BADGE),
            "large_image should be badge when show_artwork is false"
        );
        assert!(
            assets.get("small_image").is_none(),
            "small_image should be absent when show_artwork is false"
        );
    }

    #[test]
    fn build_activity_all_show_false_minimal_card() {
        let track = sample_track();
        let display = DisplayConfig {
            show_title: false,
            show_artist: false,
            show_album: false,
            show_artwork: false,
        };
        let activity = build_activity(
            &track,
            Some("https://example.com/art.jpg"),
            None,
            1_000_000,
            None,
            &display,
        );
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        assert_eq!(
            json.get("details").and_then(|v| v.as_str()),
            Some(DISCORD_PRIVATE_TITLE),
            "details should be private label"
        );
        assert!(json.get("state").is_none(), "state should be absent");
        let assets = json.get("assets").expect("assets should be present");
        assert_eq!(
            assets.get("large_image").and_then(|v| v.as_str()),
            Some(DISCORD_ASSET_RELAY_BADGE),
            "large_image should be badge"
        );
        assert!(
            assets.get("large_text").is_none(),
            "large_text should be absent"
        );
        assert!(
            assets.get("small_image").is_none(),
            "small_image should be absent"
        );
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

    // --- #37 resume-from-cache test ---

    /// After ClearActivity (pause), a resume re-publishes with the correct started_at.
    /// The pipeline caches the track and last_elapsed. On resume it computes a new
    /// started_at from the projected elapsed. This verifies that compute_started_at
    /// produces a started_at < now when elapsed is supplied (i.e., the clock-back
    /// calculation works), and that a paused-then-resumed track no longer has a
    /// `paused` parameter that would suppress timestamps.
    #[test]
    fn cached_activity_replay_after_clear() {
        let now_secs: i64 = 10_000;
        let projected_elapsed: Option<u64> = Some(90); // 90s into track at resume time
        let debounce_ms = 0u64; // no debounce on resume

        let started_at = compute_started_at(now_secs, projected_elapsed, debounce_ms);
        // started_at should be now − 90 = 9910
        assert_eq!(started_at, 9_910);

        // Building the activity should include timestamps (no paused flag suppressing them).
        let track = TrackInfo {
            title: "Test Track".into(),
            artist: "Test Artist".into(),
            album: "Test Album".into(),
        };
        let activity = build_activity(
            &track,
            None,
            None,
            started_at,
            Some(200),
            &DisplayConfig::default(),
        );
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        let timestamps = json
            .get("timestamps")
            .expect("resumed activity must include timestamps");
        assert_eq!(
            timestamps.get("start").and_then(|v| v.as_i64()),
            Some(9_910),
            "start should reflect projected elapsed"
        );
        assert_eq!(
            timestamps.get("end").and_then(|v| v.as_i64()),
            Some(10_110), // 9910 + 200
            "end should reflect track duration"
        );
    }
}
