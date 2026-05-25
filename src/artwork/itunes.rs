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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ITunesTrack {
    pub artwork_url100: Option<String>,
    pub track_name: Option<String>,
    pub artist_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ITunesSearchResponse {
    results: Vec<ITunesTrack>,
}

/// Pick the best artwork URL from a list of search results.
/// Returns the first result with a non-empty artworkUrl100, upscaled to 600x600.
pub fn pick_best_artwork(results: &[ITunesTrack]) -> Option<String> {
    results
        .iter()
        .find_map(|track| track.artwork_url100.as_deref().filter(|u| !u.is_empty()))
        .map(upscale_artwork_url)
}

/// Search iTunes API for artwork URL for the given artist + title.
/// Returns None on no match or network error (warn + continue).
pub async fn search_artwork(
    client: &Client,
    artist: &str,
    title: &str,
) -> Result<Option<String>, ArtworkError> {
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

    Ok(pick_best_artwork(&response.results))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_best_artwork_returns_first_upscaled() {
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

        let url = pick_best_artwork(&resp.results).unwrap();
        assert!(url.contains("600x600"), "should be upscaled");
        assert!(!url.contains("100x100"), "should not have 100x100");
    }

    #[test]
    fn pick_best_artwork_returns_none_for_empty_results() {
        let result = pick_best_artwork(&[]);
        assert!(result.is_none());
    }

    #[test]
    fn pick_best_artwork_skips_empty_url() {
        let tracks = vec![
            ITunesTrack {
                artwork_url100: Some(String::new()),
                track_name: Some("Track 1".into()),
                artist_name: Some("Artist".into()),
            },
            ITunesTrack {
                artwork_url100: Some(
                    "https://is1-ssl.mzstatic.com/image/thumb/Music/100x100bb.jpg".into(),
                ),
                track_name: Some("Track 2".into()),
                artist_name: Some("Artist".into()),
            },
        ];
        let url = pick_best_artwork(&tracks).unwrap();
        assert!(url.contains("600x600"));
    }

    #[test]
    fn pick_best_artwork_handles_none_url() {
        let tracks = vec![ITunesTrack {
            artwork_url100: None,
            track_name: Some("Track".into()),
            artist_name: Some("Artist".into()),
        }];
        let result = pick_best_artwork(&tracks);
        assert!(result.is_none());
    }
}
