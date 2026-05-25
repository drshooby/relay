use crate::constants::{DISCORD_RETRY_BASE_MS, DISCORD_RETRY_MAX_MS};

/// Compute the next backoff delay in milliseconds using exponential doubling.
/// Caps at DISCORD_RETRY_MAX_MS.
pub fn next_backoff_ms(current_ms: u64) -> u64 {
    current_ms.saturating_mul(2).min(DISCORD_RETRY_MAX_MS)
}

/// Returns initial backoff (first retry delay).
pub fn initial_backoff_ms() -> u64 {
    DISCORD_RETRY_BASE_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_until_max() {
        assert_eq!(next_backoff_ms(1000), 2000);
        assert_eq!(next_backoff_ms(20000), 30000); // would be 40k but capped at 30k
        assert_eq!(next_backoff_ms(30000), 30000); // already at max, stays
    }

    #[test]
    fn backoff_stays_at_max() {
        assert_eq!(next_backoff_ms(DISCORD_RETRY_MAX_MS), DISCORD_RETRY_MAX_MS);
    }
}
