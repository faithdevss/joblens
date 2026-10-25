//! Posting dates from sources, stored as RFC 3339 UTC ("2026-10-23T16:55:36Z").

use chrono::{DateTime, NaiveDateTime, SecondsFormat, TimeZone, Utc};

fn fmt(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn now() -> String {
    fmt(Utc::now())
}

/// ISO 8601 with or without an offset. No offset is read as UTC.
pub fn from_iso(s: &str) -> Option<String> {
    let s = s.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(fmt(dt.with_timezone(&Utc)));
    }
    for f in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, f) {
            return Some(fmt(dt.and_utc()));
        }
    }
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| fmt(dt.and_utc()))
}

/// RSS `pubDate`, e.g. "Fri, 23 Oct 2026 16:55:36 +0000".
pub fn from_rfc2822(s: &str) -> Option<String> {
    DateTime::parse_from_rfc2822(s.trim())
        .ok()
        .map(|dt| fmt(dt.with_timezone(&Utc)))
}

pub fn from_epoch_secs(secs: i64) -> Option<String> {
    Utc.timestamp_opt(secs, 0).single().map(fmt)
}

pub fn from_epoch_millis(ms: i64) -> Option<String> {
    Utc.timestamp_millis_opt(ms).single().map(fmt)
}

/// Seconds since an RFC 3339 time, or `None` if it can't be read.
pub fn age_secs(iso: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|dt| (Utc::now() - dt.with_timezone(&Utc)).num_seconds())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_source_formats() {
        assert_eq!(
            from_iso("2026-10-17T16:04:29").as_deref(),
            Some("2026-10-17T16:04:29Z")
        );
        assert_eq!(
            from_iso("2026-10-15T15:24:46-04:00").as_deref(),
            Some("2026-10-15T19:24:46Z")
        );
        assert_eq!(
            from_iso("2024-03-04T14:29:08.532+00:00").as_deref(),
            Some("2024-03-04T14:29:08Z")
        );
        assert_eq!(
            from_rfc2822("Fri, 23 Oct 2026 16:55:36 +0000").as_deref(),
            Some("2026-10-23T16:55:36Z")
        );
        assert_eq!(
            from_epoch_secs(1792673908).as_deref(),
            Some("2026-10-22T12:58:28Z")
        );
        assert_eq!(
            from_epoch_millis(1792767640768).as_deref(),
            Some("2026-10-23T15:00:40Z")
        );
        assert_eq!(from_iso("yesterday"), None);
    }
}
