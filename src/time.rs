use std::time::Duration;

use chrono::{DateTime, Utc};

/// Sleeps until the given wall clock time.
///
/// Sleeps in short chunks and re-checks the wall clock in between, so that
/// clock adjustments or a suspended host don't make us oversleep.
pub async fn sleep_until(t: DateTime<Utc>) {
    const MAX_CHUNK: Duration = Duration::from_secs(60);

    while let Ok(remaining) = (t - Utc::now()).to_std() {
        if remaining.is_zero() {
            break;
        }
        tokio::time::sleep(remaining.min(MAX_CHUNK)).await;
    }
}
