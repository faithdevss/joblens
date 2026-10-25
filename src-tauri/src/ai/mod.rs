//! Optional AI, through AI command-line tools already installed and signed in on this
//! computer. Off by default; the app works fully without it.
//!
//! Supported: Claude Code (`claude`). Codex is detected but not selectable until its
//! headless flags have been checked the way Claude's were (see docs/PLAN.md).

pub mod analysis;
pub mod claude;
pub mod runner;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Provider {
    Claude,
    Codex,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct AiSettings {
    /// `None` = AI off.
    pub provider: Option<Provider>,
    /// When the user agreed to send job and resume text to the provider.
    pub consented_at: Option<String>,
}

/// What we found for one provider on this computer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderStatus {
    pub provider: Provider,
    pub label: String,
    pub installed: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    /// `None` when we can't tell.
    pub logged_in: Option<bool>,
    /// JobLens can use it now.
    pub supported: bool,
    /// One plain sentence about the state, e.g. how to sign in.
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiOverview {
    pub settings: AiSettings,
    pub providers: Vec<ProviderStatus>,
}

/// Checks every provider. Runs their CLIs, so call it off the main thread.
pub fn detect() -> Vec<ProviderStatus> {
    vec![claude::status(), codex_status()]
}

fn codex_status() -> ProviderStatus {
    let path = runner::find_binary("codex");
    ProviderStatus {
        provider: Provider::Codex,
        label: Provider::Codex.label().into(),
        installed: path.is_some(),
        path: path.map(|p| p.display().to_string()),
        version: None,
        logged_in: None,
        supported: false,
        note: "Codex support is coming once we’ve tested it. Use Claude Code for now.".into(),
    }
}
