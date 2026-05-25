use crate::artwork::url::upscale_artwork_url;
use crate::constants::{ITUNES_SEARCH_LIMIT, ITUNES_SEARCH_URL};
use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ArtworkError {
    #[error("iTunes API request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
    #[error("no matching track found for {artist} - {title}")]
    NoMatch { artist: String, title: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackLookup {
    pub artwork_url: Option<String>,
    pub track_url: Option<String>,
    pub duration_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ITunesTrack {
    pub artwork_url100: Option<String>,
    pub track_view_url: Option<String>,
    pub track_time_millis: Option<u64>,
    pub track_name: Option<String>,
    pub artist_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ITunesSearchResponse {
    results: Vec<ITunesTrack>,
}

/// Pick the best match from iTunes search results.
/// Returns artwork (upscaled to 600x600) and trackViewUrl from the first result
/// with a non-empty artworkUrl100.
pub fn pick_best_match(results: &[ITunesTrack]) -> Option<TrackLookup> {
    results.iter().find_map(|track| {
        track
            .artwork_url100
            .as_deref()
            .filter(|u| !u.is_empty())
            .map(|url| TrackLookup {
                artwork_url: Some(upscale_artwork_url(url)),
                track_url: track
                    .track_view_url
                    .as_deref()
                    .filter(|u| !u.is_empty())
                    .map(str::to_owned),
                duration_secs: track
                    .track_time_millis
                    .map(|ms| ms / 1000)
                    .filter(|&s| s > 0),
            })
    })
}

/// Search iTunes API for artwork URL and track link for the given artist + title.
/// Returns None on no match or network error (warn + continue).
pub async fn search_track(
    client: &Client,
    artist: &str,
    title: &str,
) -> Result<Option<TrackLookup>, ArtworkError> {
    let response = client
        .get(ITUNES_SEARCH_URL)
        .query(&[
            ("term", format!("{artist} {title}")),
            ("media", "music".into()),
            ("limit", ITUNES_SEARCH_LIMIT.to_string()),
        ])
        .send()
        .await?
        .json::<ITunesSearchResponse>()
        .await?;

    Ok(pick_best_match(&response.results))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_best_match_returns_first_upscaled_with_track_url() {
        let fixture = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/artwork/fixtures/itunes_search.json"),
        )
        .unwrap();

        #[derive(serde::Deserialize)]
        struct Resp {
            results: Vec<ITunesTrack>,
        }
        let resp: Resp = serde_json::from_str(&fixture).unwrap();

        let lookup = pick_best_match(&resp.results).unwrap();
        let url = lookup.artwork_url.unwrap();
        assert!(url.contains("600x600"), "should be upscaled");
        assert!(!url.contains("100x100"), "should not have 100x100");
        assert_eq!(
            lookup.track_url.as_deref(),
            Some("https://music.apple.com/us/album/bohemian-rhapsody/1440811680?i=1440811690")
        );
    }

    #[test]
    fn pick_best_match_returns_none_for_empty_results() {
        let result = pick_best_match(&[]);
        assert!(result.is_none());
    }

    #[test]
    fn pick_best_match_skips_empty_url() {
        let tracks = vec![
            ITunesTrack {
                artwork_url100: Some(String::new()),
                track_view_url: None,
                track_time_millis: None,
                track_name: Some("Track 1".into()),
                artist_name: Some("Artist".into()),
            },
            ITunesTrack {
                artwork_url100: Some(
                    "https://is1-ssl.mzstatic.com/image/thumb/Music/100x100bb.jpg".into(),
                ),
                track_view_url: Some("https://music.apple.com/track/1".into()),
                track_time_millis: Some(157_000),
                track_name: Some("Track 2".into()),
                artist_name: Some("Artist".into()),
            },
        ];
        let lookup = pick_best_match(&tracks).unwrap();
        assert!(lookup.artwork_url.unwrap().contains("600x600"));
        assert_eq!(
            lookup.track_url.as_deref(),
            Some("https://music.apple.com/track/1")
        );
        assert_eq!(lookup.duration_secs, Some(157));
    }

    #[test]
    fn pick_best_match_handles_none_url() {
        let tracks = vec![ITunesTrack {
            artwork_url100: None,
            track_view_url: None,
            track_time_millis: None,
            track_name: Some("Track".into()),
            artist_name: Some("Artist".into()),
        }];
        let result = pick_best_match(&tracks);
        assert!(result.is_none());
    }
}
