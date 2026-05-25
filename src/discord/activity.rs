use discord_rich_presence::activity::{Activity, ActivityType, Assets, Timestamps};

#[derive(Debug, Clone)]
pub struct TrackInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
}

/// Build a Discord activity payload for a playing track.
///
/// - `artwork_url`: optional URL for large image (600x600).
/// - `started_at`: Unix timestamp (seconds) when track started playing.
///
/// The returned `Activity` owns its string data via `Cow::Owned`, so it can
/// outlive the `track` and `artwork_url` references.
pub fn build_activity(
    track: &TrackInfo,
    artwork_url: Option<&str>,
    started_at: i64,
) -> Activity<'static> {
    let timestamps = Timestamps::new().start(started_at);

    let large_image = artwork_url.unwrap_or("relay_default").to_owned();

    let assets = Assets::new()
        .large_image(large_image)
        .large_text(track.album.clone());

    // Format state as "Artist · Album"
    let state = format!("{} \u{00b7} {}", track.artist, track.album);

    Activity::new()
        .details(track.title.clone())
        .state(state)
        .timestamps(timestamps)
        .assets(assets)
        .activity_type(ActivityType::Listening)
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
        let _activity = build_activity(&track, Some("https://example.com/art.jpg"), 1_000_000);
    }

    #[test]
    fn build_activity_with_no_artwork_does_not_panic() {
        let track = sample_track();
        let _activity = build_activity(&track, None, 1_000_000);
    }

    #[test]
    fn build_activity_uses_listening_type() {
        // ActivityType::Listening serialises as 2 per the crate's Serialize_repr derive.
        let track = sample_track();
        let activity = build_activity(&track, None, 1_000_000);
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
        let activity = build_activity(&track, None, 1_000_000);
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        assert_eq!(
            json.get("details").and_then(|v| v.as_str()),
            Some("Bohemian Rhapsody")
        );
    }

    #[test]
    fn build_activity_state_contains_artist_and_album() {
        let track = sample_track();
        let activity = build_activity(&track, None, 1_000_000);
        let json = serde_json::to_value(&activity).expect("activity should serialise");
        let state = json
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        assert!(state.contains("Queen"), "state should contain artist");
        assert!(
            state.contains("A Night at the Opera"),
            "state should contain album"
        );
    }
}
