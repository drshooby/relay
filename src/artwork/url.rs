use crate::constants::{ITUNES_ARTWORK_SIZE_LARGE, ITUNES_ARTWORK_SIZE_SMALL};

/// Replace 100x100 thumbnail size with 600x600 in an iTunes artwork URL.
pub fn upscale_artwork_url(url: &str) -> String {
    url.replace(ITUNES_ARTWORK_SIZE_SMALL, ITUNES_ARTWORK_SIZE_LARGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upscale_replaces_100x100_with_600x600() {
        let input = "https://is1-ssl.mzstatic.com/image/thumb/Music123/v4/ab/cd/ef/abcdef-cover/100x100bb.jpg";
        let output = upscale_artwork_url(input);
        assert!(output.contains("600x600"));
        assert!(!output.contains("100x100"));
    }

    #[test]
    fn upscale_no_change_if_no_100x100() {
        let input = "https://is1-ssl.mzstatic.com/image/thumb/Music123/v4/ab/cd/ef/abcdef-cover/300x300bb.jpg";
        let output = upscale_artwork_url(input);
        assert_eq!(output, input);
    }
}
