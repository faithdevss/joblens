//! Running an AI CLI as a child process with a timeout and a cancel flag.

use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

/// Apps started from the Dock, Start menu or a desktop launcher don't get the shell's
/// PATH, so also look where installers usually put these tools.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    if let Some(home) = home.map(PathBuf::from) {
        for d in [
            ".local/bin",
            ".claude/local",
            ".npm-global/bin",
            ".bun/bin",
            ".volta/bin",
            ".cargo/bin",
        ] {
            dirs.push(home.join(d));
        }
    }
    #[cfg(windows)]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        // npm's global installs.
        dirs.push(PathBuf::from(appdata).join("npm"));
    }
    #[cfg(not(windows))]
    for d in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "/snap/bin",
    ] {
        dirs.push(PathBuf::from(d));
    }
    dirs
}

/// File names a command can have: plain on macOS and Linux; on Windows an .exe,
/// or the .cmd shim npm installs.
fn file_names(name: &str) -> Vec<String> {
    if cfg!(windows) {
        ["exe", "cmd"]
            .iter()
            .map(|ext| format!("{name}.{ext}"))
            .collect()
    } else {
        vec![name.to_string()]
    }
}

pub fn find_binary(name: &str) -> Option<PathBuf> {
    let names = file_names(name);
    search_dirs()
        .into_iter()
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .find(|p| p.is_file())
}

/// PATH for the child: its own folder first (npm installs need `node` beside them),
/// then the usual places.
fn child_path(binary: &Path) -> std::ffi::OsString {
    let mut dirs = search_dirs();
    if let Some(parent) = binary.parent() {
        dirs.insert(0, parent.to_path_buf());
    }
    std::env::join_paths(dirs).unwrap_or_default()
}

#[derive(Debug)]
pub struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RunError {
    Spawn(String),
    TimedOut(Duration),
    Cancelled,
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "Couldn’t start it: {e}"),
            Self::TimedOut(d) => write!(f, "No answer after {} s.", d.as_secs()),
            Self::Cancelled => write!(f, "Cancelled."),
        }
    }
}

/// Runs `binary args…` in `cwd`, writing `stdin` to it. Kills it on timeout or when
/// `cancel` is set.
pub fn run(
    binary: &Path,
    args: &[&str],
    stdin: &str,
    cwd: &Path,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<Output, RunError> {
    let mut command = Command::new(binary);
    command
        .args(args)
        .current_dir(cwd)
        .env("PATH", child_path(binary))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        // Don't flash a console window.
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|e| RunError::Spawn(e.to_string()))?;

    let mut input = child.stdin.take().expect("stdin is piped");
    let stdin = stdin.to_string();
    // Write and read on their own threads so a full pipe can't block us.
    let writer = thread::spawn(move || {
        let _ = input.write_all(stdin.as_bytes());
    });
    let read = |mut r: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = r.read_to_end(&mut buf);
            String::from_utf8_lossy(&buf).into_owned()
        })
    };
    let out = read(Box::new(child.stdout.take().expect("stdout is piped")));
    let err = read(Box::new(child.stderr.take().expect("stderr is piped")));

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => return Err(RunError::Spawn(e.to_string())),
        }
        let stop = if cancel.load(Ordering::Relaxed) {
            Some(RunError::Cancelled)
        } else if started.elapsed() > timeout {
            Some(RunError::TimedOut(timeout))
        } else {
            None
        };
        if let Some(e) = stop {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        thread::sleep(Duration::from_millis(100));
    };
    let _ = writer.join();
    Ok(Output {
        success: status.success(),
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sh() -> PathBuf {
        PathBuf::from("/bin/sh")
    }

    #[test]
    fn passes_stdin_and_reads_output() {
        let never = AtomicBool::new(false);
        let out = run(
            &sh(),
            &["-c", "cat; echo oops >&2"],
            "hello",
            Path::new("/"),
            Duration::from_secs(5),
            &never,
        )
        .unwrap();
        assert!(out.success);
        assert_eq!(out.stdout, "hello");
        assert_eq!(out.stderr.trim(), "oops");
    }

    #[test]
    fn stops_on_timeout_and_cancel() {
        let never = AtomicBool::new(false);
        let e = run(
            &sh(),
            &["-c", "sleep 5"],
            "",
            Path::new("/"),
            Duration::from_millis(200),
            &never,
        )
        .unwrap_err();
        assert_eq!(e, RunError::TimedOut(Duration::from_millis(200)));
        let cancelled = AtomicBool::new(true);
        let e = run(
            &sh(),
            &["-c", "sleep 5"],
            "",
            Path::new("/"),
            Duration::from_secs(5),
            &cancelled,
        )
        .unwrap_err();
        assert_eq!(e, RunError::Cancelled);
    }

    #[test]
    fn reports_a_missing_binary() {
        let never = AtomicBool::new(false);
        let e = run(
            Path::new("/nope/claude"),
            &[],
            "",
            Path::new("/"),
            Duration::from_secs(1),
            &never,
        )
        .unwrap_err();
        assert!(matches!(e, RunError::Spawn(_)));
    }
}
