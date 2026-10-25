//! "Is this job still open?": fetches the original posting and reads the answer from
//! the response. Only the posting's own site is contacted.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum OpenStatus {
    Open,
    Closed,
    /// We couldn't tell; check the posting yourself.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OpenCheck {
    pub status: OpenStatus,
    pub note: String,
    pub checked_at: String,
}

const CLOSED_PHRASES: &[&str] = &[
    "no longer accepting applications",
    "no longer accepting applicants",
    "this job is no longer available",
    "this job is no longer open",
    "job is no longer available",
    "position has been filled",
    "this position is no longer available",
    "this job has expired",
    "this job posting has expired",
    "this posting has been closed",
    "this job has been closed",
    "job not found",
    "the job you are looking for is no longer",
    "this role has been filled",
];

/// Decides from the HTTP status, the final URL after redirects, and the page text.
pub fn judge(status: u16, final_url: &str, body: &str) -> (OpenStatus, String) {
    if status == 404 || status == 410 {
        return (
            OpenStatus::Closed,
            "The posting is gone from the site.".into(),
        );
    }
    if status >= 400 {
        return (
            OpenStatus::Unknown,
            format!("The site answered with an error ({status}). Check the posting yourself."),
        );
    }
    // Greenhouse sends closed jobs back to the board with ?error=true.
    if final_url.contains("error=true") {
        return (
            OpenStatus::Closed,
            "The site sent us back to its job list.".into(),
        );
    }
    let lower = body.to_lowercase();
    if let Some(p) = CLOSED_PHRASES.iter().find(|p| lower.contains(*p)) {
        return (OpenStatus::Closed, format!("The posting says “{p}”."));
    }
    (OpenStatus::Open, "The posting is still up.".into())
}

pub async fn check(http: &reqwest::Client, url: &str) -> OpenCheck {
    let checked_at = super::dates::now();
    let (status, note) = match http.get(url).send().await {
        Ok(resp) => {
            let code = resp.status().as_u16();
            let final_url = resp.url().to_string();
            // Only the start of the page matters, and it keeps big pages cheap.
            let body = resp.text().await.unwrap_or_default();
            let body: String = body.chars().take(400_000).collect();
            judge(code, &final_url, &body)
        }
        Err(e) if e.is_timeout() => (
            OpenStatus::Unknown,
            "The site didn’t answer in time.".into(),
        ),
        Err(_) => (OpenStatus::Unknown, "We couldn’t reach the site.".into()),
    };
    OpenCheck {
        status,
        note,
        checked_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_common_signals() {
        assert_eq!(judge(404, "", "").0, OpenStatus::Closed);
        assert_eq!(judge(500, "", "").0, OpenStatus::Unknown);
        assert_eq!(
            judge(200, "https://boards.greenhouse.io/acme?error=true", "").0,
            OpenStatus::Closed
        );
        let (s, note) = judge(
            200,
            "https://x",
            "<h1>Sorry, this position has been filled.</h1>",
        );
        assert_eq!(s, OpenStatus::Closed);
        assert!(note.contains("position has been filled"));
        assert_eq!(
            judge(
                200,
                "https://x",
                "<h1>Senior Engineer</h1><button>Apply</button>"
            )
            .0,
            OpenStatus::Open
        );
    }
}
