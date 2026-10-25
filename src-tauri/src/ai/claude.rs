//! Claude Code (`claude`) in headless mode.
//!
//! Flags from the Phase 3 spike (Claude Code 2.1.x):
//! `-p --output-format json --json-schema <schema>` returns one JSON object whose
//! `structured_output` is already checked against the schema. `--tools ""`,
//! `--strict-mcp-config`, `--setting-sources ""` and `--disable-slash-commands` keep it a
//! plain model call: no tools, no MCP servers, no user hooks. `--bare` can't be used: it
//! skips claude.ai sign-ins. Auth: `claude auth status --json` → `loggedIn`.

use std::{path::Path, sync::atomic::AtomicBool, time::Duration};

use serde_json::Value;

use super::{Provider, ProviderStatus, runner};

/// Fast and good enough for reading job posts; each job is analyzed once and cached.
pub const MODEL: &str = "haiku";
/// Writing letters needs better prose; it runs only when the user asks for a draft.
pub const WRITING_MODEL: &str = "sonnet";
const TIMEOUT: Duration = Duration::from_secs(120);
const QUICK: Duration = Duration::from_secs(15);

pub fn status() -> ProviderStatus {
    let label = Provider::Claude.label().to_string();
    let Some(path) = runner::find_binary("claude") else {
        return ProviderStatus {
            provider: Provider::Claude,
            label,
            installed: false,
            path: None,
            version: None,
            logged_in: None,
            supported: true,
            note: "Not installed. Install Claude Code, sign in with `claude`, then check again."
                .into(),
        };
    };
    let never = AtomicBool::new(false);
    let tmp = std::env::temp_dir();
    let version = runner::run(&path, &["--version"], "", &tmp, QUICK, &never)
        .ok()
        .filter(|o| o.success)
        .map(|o| o.stdout.trim().to_string())
        .filter(|v| !v.is_empty());
    let logged_in = runner::run(
        &path,
        &["auth", "status", "--json"],
        "",
        &tmp,
        QUICK,
        &never,
    )
    .ok()
    .and_then(|o| serde_json::from_str::<Value>(&o.stdout).ok())
    .and_then(|v| v.get("loggedIn").and_then(Value::as_bool));
    let note = match logged_in {
        Some(true) => "Ready.".into(),
        Some(false) => "Installed but not signed in. Run `claude` in a terminal to sign in.".into(),
        None => "Installed. We couldn’t check whether you’re signed in.".into(),
    };
    ProviderStatus {
        provider: Provider::Claude,
        label,
        installed: true,
        path: Some(path.display().to_string()),
        version,
        logged_in,
        supported: true,
        note,
    }
}

/// Asks once and returns the structured answer, or a plain-words error.
pub fn ask(
    model: &str,
    system: &str,
    prompt: &str,
    schema: &Value,
    cwd: &Path,
    cancel: &AtomicBool,
) -> Result<Value, String> {
    let binary = runner::find_binary("claude")
        .ok_or_else(|| "Claude Code isn’t installed on this computer.".to_string())?;
    let schema = schema.to_string();
    let args = [
        "-p",
        "--output-format",
        "json",
        "--json-schema",
        &schema,
        "--model",
        model,
        "--system-prompt",
        system,
        "--tools",
        "",
        "--strict-mcp-config",
        "--setting-sources",
        "",
        "--disable-slash-commands",
        "--no-session-persistence",
    ];
    let out =
        runner::run(&binary, &args, prompt, cwd, TIMEOUT, cancel).map_err(|e| e.to_string())?;
    parse_response(&out.stdout, &out.stderr)
}

/// Reads `claude -p --output-format json` output.
pub fn parse_response(stdout: &str, stderr: &str) -> Result<Value, String> {
    let Ok(v) = serde_json::from_str::<Value>(stdout.trim()) else {
        return Err(explain(stderr, stdout));
    };
    let failed = v.get("is_error").and_then(Value::as_bool) == Some(true)
        || v.get("subtype")
            .and_then(Value::as_str)
            .is_some_and(|s| s != "success");
    if failed {
        let msg = v.get("result").and_then(Value::as_str).unwrap_or("");
        return Err(explain(stderr, msg));
    }
    v.get("structured_output")
        .filter(|o| o.is_object())
        .cloned()
        .ok_or_else(|| "Claude answered, but not in the format we asked for.".into())
}

fn explain(stderr: &str, detail: &str) -> String {
    let text = format!("{stderr}\n{detail}").to_lowercase();
    if text.contains("log in")
        || text.contains("login")
        || text.contains("not logged")
        || text.contains("authenticat")
    {
        "Claude Code isn’t signed in. Run `claude` in a terminal to sign in.".into()
    } else if text.contains("rate limit")
        || text.contains("usage limit")
        || text.contains("limit reached")
    {
        "Claude’s usage limit was reached. Try again later.".into()
    } else if text.contains("unrecognized_model") {
        "This Claude Code version doesn’t know the model we use. Update Claude Code.".into()
    } else {
        let first = detail
            .trim()
            .lines()
            .chain(stderr.trim().lines())
            .find(|l| !l.trim().is_empty());
        match first {
            Some(l) => format!(
                "Claude Code failed: {}",
                l.chars().take(200).collect::<String>()
            ),
            None => "Claude Code failed without saying why.".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_structured_output() {
        let out = r#"{"type":"result","subtype":"success","is_error":false,"result":"{}","structured_output":{"skills":["React"],"years":5}}"#;
        assert_eq!(parse_response(out, "").unwrap()["years"], 5);
    }

    #[test]
    fn explains_failures() {
        let e =
            parse_response("", "[claude-code:unrecognized_model] {\"model\":\"x\"}").unwrap_err();
        assert!(e.contains("Update Claude Code"), "{e}");
        let out = r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login"}"#;
        assert!(
            parse_response(out, "")
                .unwrap_err()
                .contains("isn’t signed in")
        );
        let out = r#"{"type":"result","subtype":"success","is_error":false,"result":"sure!"}"#;
        assert!(parse_response(out, "").unwrap_err().contains("format"));
        let out = r#"{"type":"result","subtype":"error_max_turns","is_error":false}"#;
        assert!(parse_response(out, "").is_err());
    }
}
