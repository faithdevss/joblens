use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

use serde::Serialize;
use tauri::{State, ipc::InvokeBody};
use ts_rs::TS;

use crate::{
    ai::{self, AiOverview, AiSettings, Provider, analysis},
    alerts::{self, AlertSettings},
    apply::{
        export,
        gap::{self, EditAction, GapCheck},
        prep::{self, InterviewPrep},
        writer::{self, Draft, DraftKind, DraftLength, WriterState},
    },
    data::{self, DataOverview},
    db::{self, store},
    error::{AppError, AppResult},
    jobs::{
        Feed, JobDetail, JobState, ManualJobInput, RawJob, RefreshReport, SourceStatus, Weights,
        feed, refresh, sources,
        still_open::{self, OpenCheck},
    },
    profile::{Profile, SearchSettings, SetupStatus, StoredProfile},
    resume,
    searches::{self, SavedSearch},
    state::AppState,
    tracker::{self, Application, ApplicationEdit},
};

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub db_path: String,
    pub schema_version: u32,
    #[ts(type = "number")]
    pub db_size_bytes: u64,
}

#[tauri::command]
pub fn app_info(app: tauri::AppHandle, state: State<'_, AppState>) -> AppResult<AppInfo> {
    let conn = lock(&state);
    let schema_version = db::schema_version(&conn)?;
    let db_size_bytes = std::fs::metadata(&state.db_path)
        .map(|m| m.len())
        .unwrap_or(0);
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        data_dir: state.data_dir.display().to_string(),
        db_path: state.db_path.display().to_string(),
        schema_version,
        db_size_bytes,
    })
}

fn lock(state: &AppState) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
    state.db.lock().expect("db mutex poisoned")
}

#[tauri::command]
pub fn get_profile(state: State<'_, AppState>) -> AppResult<Option<StoredProfile>> {
    store::get_profile(&lock(&state))
}

#[tauri::command]
pub fn save_profile(state: State<'_, AppState>, profile: Profile) -> AppResult<()> {
    store::save_profile(&lock(&state), &profile)
}

/// Takes the raw file bytes as the request body and its name in `x-file-name`
/// (URI-encoded). Parses offline, keeps a copy of the file, replaces the profile.
#[tauri::command]
pub async fn import_resume(
    state: State<'_, AppState>,
    request: tauri::ipc::Request<'_>,
) -> AppResult<StoredProfile> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(AppError::Other(
            "expected the resume file as raw bytes".into(),
        ));
    };
    let file_name = request
        .headers()
        .get("x-file-name")
        .and_then(|v| v.to_str().ok())
        .map(percent_decode)
        .unwrap_or_default();
    let file_name = Path::new(&file_name)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("resume")
        .to_string();

    let bytes = bytes.clone();
    let (parsed, bytes, file_name) = tauri::async_runtime::spawn_blocking(move || {
        resume::read(&file_name, &bytes).map(|p| (p, bytes, file_name))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))??;

    // Keep only the latest resume file, for backups and re-reading later.
    let dir = state.data_dir.join("resume");
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(&file_name), &bytes)?;

    let conn = lock(&state);
    store::save_import(
        &conn,
        &parsed.profile,
        &parsed.lines,
        &file_name,
        bytes.len() as u64,
        parsed.pages,
    )?;
    store::get_profile(&conn)?.ok_or_else(|| AppError::Other("profile was not saved".into()))
}

#[tauri::command]
pub fn get_search_settings(state: State<'_, AppState>) -> AppResult<SearchSettings> {
    store::get_search_settings(&lock(&state))
}

#[tauri::command]
pub fn save_search_settings(state: State<'_, AppState>, settings: SearchSettings) -> AppResult<()> {
    store::save_search_settings(&lock(&state), &settings)
}

#[tauri::command]
pub fn get_setup_status(state: State<'_, AppState>) -> AppResult<SetupStatus> {
    store::get_setup_status(&lock(&state))
}

#[tauri::command]
pub fn set_setup_status(state: State<'_, AppState>, status: SetupStatus) -> AppResult<()> {
    store::set_setup_status(&lock(&state), status)
}

/// Async so scoring every job runs off the main thread and doesn't freeze the window.
#[tauri::command]
pub async fn get_feed(state: State<'_, AppState>) -> AppResult<Feed> {
    feed::feed(&lock(&state))
}

#[tauri::command]
pub fn get_job(state: State<'_, AppState>, id: i64) -> AppResult<JobDetail> {
    feed::job_detail(&lock(&state), id)
}

#[tauri::command]
pub fn set_job_state(state: State<'_, AppState>, id: i64, job_state: JobState) -> AppResult<()> {
    feed::set_state(&lock(&state), id, job_state)
}

#[tauri::command]
pub fn list_sources(state: State<'_, AppState>) -> AppResult<Vec<SourceStatus>> {
    refresh::list_sources(&lock(&state))
}

#[tauri::command]
pub fn set_source_enabled(state: State<'_, AppState>, id: String, enabled: bool) -> AppResult<()> {
    refresh::set_enabled(&lock(&state), &id, enabled)
}

/// Fetches every enabled source, or just `only`. Sources fetched too recently are skipped.
#[tauri::command]
pub async fn refresh_sources(
    state: State<'_, AppState>,
    only: Option<Vec<String>>,
) -> AppResult<RefreshReport> {
    let _running = state.refresh_lock.lock().await;
    let report = refresh::refresh(&state.db, &state.http, only).await?;
    // The user sees these now; record them so a later background run doesn't announce them.
    let conn = lock(&state);
    alerts::new_matches(&conn, &alerts::get_settings(&conn)?, false)?;
    Ok(report)
}

/// Finds a company's job board (Greenhouse, Lever, Ashby, Workable, Recruitee or
/// SmartRecruiters) from a name or link and adds it.
#[tauri::command]
pub async fn add_watchlist_company(
    state: State<'_, AppState>,
    input: String,
) -> AppResult<SourceStatus> {
    let found = sources::find_board(&state.http, &input)
        .await
        .map_err(AppError::Other)?;
    let conn = lock(&state);
    let id = refresh::add_watchlist(&conn, &found)?;
    refresh::list_sources(&conn)?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| AppError::Other("company was not saved".into()))
}

#[tauri::command]
pub fn remove_source(state: State<'_, AppState>, id: String) -> AppResult<()> {
    refresh::remove_source(&lock(&state), &id)
}

/// Adds a job by hand: fetched from a supported link, or from the pasted description.
/// Returns the job's id.
#[tauri::command]
pub async fn add_manual_job(state: State<'_, AppState>, input: ManualJobInput) -> AppResult<i64> {
    let url = input.url.trim();
    let pasted = !input.description.trim().is_empty();
    let fetched = if !url.is_empty() && sources::posting_from_url(url).is_some() {
        match sources::fetch_posting(&state.http, url).await {
            Ok(job) => Some(job),
            // The user also pasted the text, so use that instead.
            Err(_) if pasted => None,
            Err(e) => return Err(AppError::Other(e)),
        }
    } else {
        None
    };
    let raw = match fetched {
        Some(job) => job,
        None => manual_raw(&input)?,
    };
    refresh::add_manual(&lock(&state), &raw)
}

fn manual_raw(input: &ManualJobInput) -> AppResult<RawJob> {
    let (title, company, description) = (
        input.title.trim(),
        input.company.trim(),
        input.description.trim(),
    );
    if description.is_empty() {
        return Err(AppError::Other(if input.url.trim().is_empty() {
            "Paste the job description.".into()
        } else {
            "We can’t read that link. Paste the job description instead.".into()
        }));
    }
    if title.is_empty() || company.is_empty() {
        return Err(AppError::Other("Add the job title and company.".into()));
    }
    let url = input.url.trim().to_string();
    Ok(RawJob {
        external_id: if url.is_empty() {
            format!("pasted-{}", crate::jobs::dates::now())
        } else {
            url.clone()
        },
        title: title.into(),
        company: company.into(),
        url,
        description: crate::jobs::text::tidy(description),
        location: input.location.trim().into(),
        ..Default::default()
    })
}

#[tauri::command]
pub fn get_weights(state: State<'_, AppState>) -> AppResult<Weights> {
    store::get_weights(&lock(&state))
}

#[tauri::command]
pub fn save_weights(state: State<'_, AppState>, weights: Weights) -> AppResult<()> {
    store::save_weights(&lock(&state), &weights)
}

#[tauri::command]
pub fn get_ai_settings(state: State<'_, AppState>) -> AppResult<AiSettings> {
    store::get_ai_settings(&lock(&state))
}

/// AI settings plus what's installed. Runs the CLIs, so it takes a moment.
#[tauri::command]
pub async fn get_ai_overview(state: State<'_, AppState>) -> AppResult<AiOverview> {
    let settings = store::get_ai_settings(&lock(&state))?;
    let providers = tauri::async_runtime::spawn_blocking(ai::detect)
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(AiOverview {
        settings,
        providers,
    })
}

/// Turns AI on with a provider, or off with `None`. Turning it on needs the user's consent.
#[tauri::command]
pub fn set_ai_provider(
    state: State<'_, AppState>,
    provider: Option<Provider>,
    consent: bool,
) -> AppResult<AiSettings> {
    let settings = match provider {
        None => AiSettings::default(),
        Some(Provider::Codex) => {
            return Err(AppError::Other("Codex isn’t supported yet.".into()));
        }
        Some(p) if consent => AiSettings {
            provider: Some(p),
            consented_at: Some(crate::jobs::dates::now()),
        },
        Some(_) => {
            return Err(AppError::Other(
                "Agree to what gets sent before turning AI on.".into(),
            ));
        }
    };
    store::save_ai_settings(&lock(&state), &settings)?;
    Ok(settings)
}

/// Runs (or reuses) the AI analysis for one job and returns its updated detail.
#[tauri::command]
pub async fn analyze_job(state: State<'_, AppState>, id: i64, force: bool) -> AppResult<JobDetail> {
    let (provider, items, title, company, description) = {
        let conn = lock(&state);
        let Some(provider) = store::get_ai_settings(&conn)?.provider else {
            return Err(AppError::Other(
                "AI is off. Turn it on in Settings → Privacy & AI.".into(),
            ));
        };
        let profile = store::get_profile(&conn)?
            .map(|p| p.profile)
            .unwrap_or_default();
        let items = analysis::profile_items(&profile);
        if items.is_empty() {
            return Err(AppError::Other(
                "Add your resume first, so there’s something to compare.".into(),
            ));
        }
        if !force && analysis::load(&conn, id, &analysis::profile_hash(&items))?.is_some() {
            return feed::job_detail(&conn, id);
        }
        let (title, company, description): (String, String, String) = conn.query_row(
            "SELECT title, company, description FROM jobs WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        (provider, items, title, company, description)
    };

    let run = AiRun::start(&state, format!("analysis:{id}"))?;
    let (cwd, cancel) = (run.cwd.clone(), run.cancel.clone());
    let hash = analysis::profile_hash(&items);
    let result = tauri::async_runtime::spawn_blocking(move || {
        analysis::analyze(
            provider,
            &title,
            &company,
            &description,
            &items,
            &cwd,
            &cancel,
        )
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?;
    drop(run);

    let a = result.map_err(AppError::Other)?;
    let conn = lock(&state);
    analysis::save(&conn, id, &hash, &a)?;
    feed::job_detail(&conn, id)
}

#[tauri::command]
pub fn cancel_analysis(state: State<'_, AppState>, id: i64) {
    cancel_run(&state, &format!("analysis:{id}"));
}

fn cancel_run(state: &AppState, key: &str) {
    if let Some(flag) = state.ai_runs.lock().expect("ai runs poisoned").get(key) {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// A registered, cancellable AI run. Unregisters itself when dropped.
struct AiRun<'a> {
    state: &'a AppState,
    key: String,
    cancel: Arc<AtomicBool>,
    /// An empty folder, so the CLI picks up no project files.
    cwd: std::path::PathBuf,
}

impl<'a> AiRun<'a> {
    fn start(state: &'a AppState, key: String) -> AppResult<Self> {
        let cancel = Arc::new(AtomicBool::new(false));
        state
            .ai_runs
            .lock()
            .expect("ai runs poisoned")
            .insert(key.clone(), cancel.clone());
        let cwd = state.data_dir.join("ai");
        std::fs::create_dir_all(&cwd)?;
        Ok(Self {
            state,
            key,
            cancel,
            cwd,
        })
    }
}

impl Drop for AiRun<'_> {
    fn drop(&mut self) {
        self.state
            .ai_runs
            .lock()
            .expect("ai runs poisoned")
            .remove(&self.key);
    }
}

// ---- Resume gap check ----

#[tauri::command]
pub fn get_gap_check(state: State<'_, AppState>, id: i64) -> AppResult<GapCheck> {
    gap::check(&lock(&state), id)
}

/// Accepts (with the user's final text), rejects or undoes a suggested resume edit.
#[tauri::command]
pub fn decide_edit(
    state: State<'_, AppState>,
    id: i64,
    edit_id: String,
    action: EditAction,
) -> AppResult<GapCheck> {
    let conn = lock(&state);
    gap::decide(&conn, id, &edit_id, action)?;
    gap::check(&conn, id)
}

fn ai_provider(conn: &rusqlite::Connection) -> AppResult<Provider> {
    store::get_ai_settings(conn)?
        .provider
        .ok_or_else(|| AppError::Other("AI is off. Turn it on in Settings → Privacy & AI.".into()))
}

/// Asks the AI for rewordings of the user's own resume lines that fit this job.
#[tauri::command]
pub async fn suggest_edits(state: State<'_, AppState>, id: i64) -> AppResult<GapCheck> {
    let (provider, prompt, profile) = {
        let conn = lock(&state);
        let provider = ai_provider(&conn)?;
        let g = gap::check(&conn, id)?;
        let profile = store::get_profile(&conn)?
            .map(|p| p.profile)
            .unwrap_or_default();
        if gap::rewritable(&profile).is_empty() {
            return Err(AppError::Other(
                "Your profile has no achievements or summary to reword yet.".into(),
            ));
        }
        let (title, company, description): (String, String, String) = conn.query_row(
            "SELECT title, company, description FROM jobs WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let prompt = gap::suggest_prompt(&title, &company, &description, &g.requirements, &profile);
        (provider, prompt, profile)
    };
    let run = AiRun::start(&state, format!("suggest:{id}"))?;
    let (cwd, cancel) = (run.cwd.clone(), run.cancel.clone());
    let result = tauri::async_runtime::spawn_blocking(move || {
        gap::suggest(provider, &prompt, &profile, &cwd, &cancel)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?;
    drop(run);
    let edits = result.map_err(AppError::Other)?;
    let conn = lock(&state);
    gap::save_suggestions(&conn, id, &edits)?;
    gap::check(&conn, id)
}

// ---- Application writer ----

#[tauri::command]
pub fn get_writer(state: State<'_, AppState>, id: i64, kind: DraftKind) -> AppResult<WriterState> {
    writer::state(&lock(&state), id, kind)
}

/// Writes a new draft, replacing the current one. Templates, or the AI when `use_ai`.
#[tauri::command]
pub async fn write_draft(
    state: State<'_, AppState>,
    id: i64,
    kind: DraftKind,
    length: DraftLength,
    highlights: Vec<String>,
    use_ai: bool,
) -> AppResult<Draft> {
    let (ctx, provider) = {
        let conn = lock(&state);
        writer::require_saved(&conn, id)?;
        let ctx = writer::Context::load(&conn, id)?;
        if !use_ai {
            let mut d = writer::from_rules(&ctx, kind, length, &highlights);
            writer::save(&conn, id, &mut d)?;
            return Ok(d);
        }
        (ctx, ai_provider(&conn)?)
    };
    let run = AiRun::start(&state, format!("write:{id}"))?;
    let (cwd, cancel) = (run.cwd.clone(), run.cancel.clone());
    let result = tauri::async_runtime::spawn_blocking(move || {
        writer::from_ai(provider, &ctx, kind, length, &highlights, &cwd, &cancel)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?;
    drop(run);
    let mut d = result.map_err(AppError::Other)?;
    writer::save(&lock(&state), id, &mut d)?;
    Ok(d)
}

/// Loads the working draft, applies `f`, saves it.
fn with_draft(
    state: &AppState,
    id: i64,
    kind: DraftKind,
    f: impl FnOnce(&writer::Context, &mut Draft) -> Result<(), String>,
) -> AppResult<Draft> {
    let conn = lock(state);
    writer::require_saved(&conn, id)?;
    let ctx = writer::Context::load(&conn, id)?;
    let mut d = writer::load(&conn, id, kind)?
        .ok_or_else(|| AppError::Other("Write a draft first.".into()))?;
    f(&ctx, &mut d).map_err(AppError::Other)?;
    writer::save(&conn, id, &mut d)?;
    Ok(d)
}

/// Adds a new wording of one paragraph; earlier ones are kept.
#[tauri::command]
pub async fn rewrite_paragraph(
    state: State<'_, AppState>,
    id: i64,
    kind: DraftKind,
    index: usize,
    use_ai: bool,
) -> AppResult<Draft> {
    if !use_ai {
        return with_draft(&state, id, kind, |ctx, d| {
            writer::rewrite_from_rules(ctx, d, index)
        });
    }
    let (ctx, mut d, provider) = {
        let conn = lock(&state);
        writer::require_saved(&conn, id)?;
        let d = writer::load(&conn, id, kind)?
            .ok_or_else(|| AppError::Other("Write a draft first.".into()))?;
        (writer::Context::load(&conn, id)?, d, ai_provider(&conn)?)
    };
    let run = AiRun::start(&state, format!("write:{id}"))?;
    let (cwd, cancel) = (run.cwd.clone(), run.cancel.clone());
    let result = tauri::async_runtime::spawn_blocking(move || {
        writer::rewrite_with_ai(provider, &ctx, &mut d, index, &cwd, &cancel).map(|()| d)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?;
    drop(run);
    let mut d = result.map_err(AppError::Other)?;
    writer::save(&lock(&state), id, &mut d)?;
    Ok(d)
}

/// Shows an earlier (or later) wording of a paragraph.
#[tauri::command]
pub fn set_paragraph_version(
    state: State<'_, AppState>,
    id: i64,
    kind: DraftKind,
    index: usize,
    version: u32,
) -> AppResult<Draft> {
    with_draft(&state, id, kind, |_, d| {
        let p = d
            .paragraphs
            .get_mut(index)
            .ok_or("That paragraph no longer exists.")?;
        if version as usize >= p.versions.len() {
            return Err("That draft no longer exists.".into());
        }
        p.current = version;
        Ok(())
    })
}

/// Replaces a paragraph with text the user typed; new sentences are checked again.
#[tauri::command]
pub fn edit_paragraph(
    state: State<'_, AppState>,
    id: i64,
    kind: DraftKind,
    index: usize,
    text: String,
) -> AppResult<Draft> {
    with_draft(&state, id, kind, |ctx, d| {
        writer::edit_paragraph(ctx, d, index, &text)
    })
}

/// "It's true — keep" (`keep`) or "Remove" for a flagged sentence.
#[tauri::command]
pub fn resolve_sentence(
    state: State<'_, AppState>,
    id: i64,
    kind: DraftKind,
    paragraph: usize,
    sentence: usize,
    keep: bool,
) -> AppResult<Draft> {
    with_draft(&state, id, kind, |_, d| {
        writer::resolve(d, paragraph, sentence, keep)
    })
}

/// Subject, greeting and sign-off: free text with no claims to check.
#[tauri::command]
pub fn update_draft_text(
    state: State<'_, AppState>,
    id: i64,
    kind: DraftKind,
    subject: String,
    greeting: String,
    sign_off: String,
) -> AppResult<Draft> {
    with_draft(&state, id, kind, |_, d| {
        d.subject = subject;
        d.greeting = greeting;
        d.sign_off = sign_off;
        Ok(())
    })
}

#[derive(Debug, Clone, Copy, serde::Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ExportFormat {
    Text,
    Docx,
    Pdf,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportResult {
    /// The plain text (every format), e.g. for the clipboard.
    pub text: String,
    /// Where the file was saved; `None` for plain text.
    pub path: Option<String>,
}

/// Exports the working draft. Refused while the honesty check has open flags.
#[tauri::command]
pub fn export_draft(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: i64,
    kind: DraftKind,
    format: ExportFormat,
) -> AppResult<ExportResult> {
    let conn = lock(&state);
    let d = writer::load(&conn, id, kind)?
        .ok_or_else(|| AppError::Other("Write a draft first.".into()))?;
    let open = d.open_flags();
    if open > 0 {
        return Err(AppError::Other(format!(
            "{open} sentence{} still need{} checking. Keep or remove {} first.",
            if open == 1 { "" } else { "s" },
            if open == 1 { "s" } else { "" },
            if open == 1 { "it" } else { "them" },
        )));
    }
    let text = export::text(&d);
    let (bytes, ext) = match format {
        ExportFormat::Text => return Ok(ExportResult { text, path: None }),
        ExportFormat::Docx => (export::docx(&d)?, "docx"),
        ExportFormat::Pdf => (export::pdf(&d), "pdf"),
    };
    let company: String =
        conn.query_row("SELECT company FROM jobs WHERE id = ?1", [id], |r| r.get(0))?;
    let path = export::free_path(
        &download_dir(&app, &state)?,
        &export::file_name(&d, &company, ext),
    );
    std::fs::write(&path, bytes)?;
    Ok(ExportResult {
        text,
        path: Some(path.display().to_string()),
    })
}

/// Where exported files go: the user's Downloads folder.
fn download_dir(app: &tauri::AppHandle, state: &AppState) -> AppResult<std::path::PathBuf> {
    use tauri::Manager;
    let dir = app
        .path()
        .download_dir()
        .unwrap_or_else(|_| state.data_dir.join("exports"));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

// ---- Application tracker ----

#[tauri::command]
pub fn list_applications(state: State<'_, AppState>) -> AppResult<Vec<Application>> {
    tracker::list(&lock(&state))
}

#[tauri::command]
pub fn update_application(
    state: State<'_, AppState>,
    id: i64,
    edit: ApplicationEdit,
) -> AppResult<Application> {
    tracker::update(&lock(&state), id, &edit, &tracker::today())
}

/// Saves the job if needed and marks it applied, recording what was sent.
#[tauri::command]
pub fn mark_applied(state: State<'_, AppState>, id: i64) -> AppResult<Application> {
    tracker::mark_applied(&lock(&state), id, &tracker::today())
}

#[tauri::command]
pub fn mark_followed_up(state: State<'_, AppState>, id: i64) -> AppResult<Application> {
    tracker::followed_up(&lock(&state), id, &tracker::today())
}

/// Writes every application to a .csv in Downloads and returns its path.
#[tauri::command]
pub fn export_applications_csv(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> AppResult<String> {
    let apps = tracker::list(&lock(&state))?;
    let path = export::free_path(
        &download_dir(&app, &state)?,
        &format!("JobLens applications {}.csv", tracker::today()),
    );
    std::fs::write(&path, tracker::csv(&apps))?;
    Ok(path.display().to_string())
}

// ---- Your data ----

#[tauri::command]
pub fn get_data_overview(state: State<'_, AppState>) -> AppResult<DataOverview> {
    data::overview(&lock(&state), &state.data_dir, &state.db_path)
}

/// Writes a backup zip to Downloads and returns its path.
#[tauri::command]
pub fn create_backup(app: tauri::AppHandle, state: State<'_, AppState>) -> AppResult<String> {
    let path = export::free_path(
        &download_dir(&app, &state)?,
        &format!("JobLens backup {}.zip", tracker::today()),
    );
    let version = app.package_info().version.to_string();
    data::backup(&lock(&state), &state.data_dir, &path, &version)?;
    Ok(path.display().to_string())
}

/// Takes a backup zip as the raw request body and replaces all data with it.
#[tauri::command]
pub async fn restore_backup(
    state: State<'_, AppState>,
    request: tauri::ipc::Request<'_>,
) -> AppResult<()> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(AppError::Other(
            "expected the backup file as raw bytes".into(),
        ));
    };
    let restore = data::read_backup(bytes, &state.data_dir.join("restore"))?;
    let mut conn = lock(&state);
    let result = data::swap(&mut conn, &state.data_dir, &state.db_path, Some(restore));
    std::fs::remove_dir_all(state.data_dir.join("restore")).ok();
    result
}

/// Deletes the database, stored resume and AI scratch folder; starts fresh.
#[tauri::command]
pub fn delete_all_data(state: State<'_, AppState>, confirm: bool) -> AppResult<()> {
    if !confirm {
        return Err(AppError::Other("Confirm before deleting your data.".into()));
    }
    let mut conn = lock(&state);
    data::swap(&mut conn, &state.data_dir, &state.db_path, None)
}

// ---- Later: interview prep, still-open check ----

#[tauri::command]
pub fn get_interview_prep(state: State<'_, AppState>, id: i64) -> AppResult<InterviewPrep> {
    prep::prep(&lock(&state), id)
}

#[tauri::command]
pub fn set_topic_prepared(
    state: State<'_, AppState>,
    id: i64,
    key: String,
    prepared: bool,
) -> AppResult<()> {
    prep::set_prepared(&lock(&state), id, &key, prepared)
}

/// Fetches the original posting to see whether it's still open.
#[tauri::command]
pub async fn check_still_open(state: State<'_, AppState>, id: i64) -> AppResult<OpenCheck> {
    let url: Option<String> = {
        let conn = lock(&state);
        conn.query_row(
            "SELECT COALESCE(NULLIF(j.url, ''),
                (SELECT url FROM job_sources js WHERE js.job_id = j.id AND url IS NOT NULL AND url != '' LIMIT 1))
             FROM jobs j WHERE j.id = ?1",
            [id],
            |r| r.get(0),
        )?
    };
    let url = url.ok_or_else(|| AppError::Other("This job has no link to check.".into()))?;
    Ok(still_open::check(&state.http, &url).await)
}

#[tauri::command]
pub fn cancel_writing(state: State<'_, AppState>, id: i64) {
    cancel_run(&state, &format!("write:{id}"));
    cancel_run(&state, &format!("suggest:{id}"));
}

/// Decodes `encodeURIComponent` output. Invalid escapes are kept as-is.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(b) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::percent_decode;

    #[test]
    fn decodes_uri_components() {
        assert_eq!(percent_decode("S%C3%A1m%20CV.pdf"), "Sám CV.pdf");
        assert_eq!(percent_decode("100%"), "100%");
    }
}

// ---- Saved searches ----

#[tauri::command]
pub fn list_searches(state: State<'_, AppState>) -> AppResult<Vec<SavedSearch>> {
    searches::list(&lock(&state))
}

/// Copies the active search under a new name and switches to it.
#[tauri::command]
pub fn create_search(state: State<'_, AppState>, name: String) -> AppResult<Vec<SavedSearch>> {
    let conn = lock(&state);
    searches::create(&conn, &name)?;
    searches::list(&conn)
}

#[tauri::command]
pub fn rename_search(
    state: State<'_, AppState>,
    id: i64,
    name: String,
) -> AppResult<Vec<SavedSearch>> {
    let conn = lock(&state);
    searches::rename(&conn, id, &name)?;
    searches::list(&conn)
}

#[tauri::command]
pub fn delete_search(state: State<'_, AppState>, id: i64) -> AppResult<Vec<SavedSearch>> {
    let conn = lock(&state);
    searches::delete(&conn, id)?;
    searches::list(&conn)
}

/// Switches the feed to another search and returns its settings.
#[tauri::command]
pub fn set_active_search(state: State<'_, AppState>, id: i64) -> AppResult<SearchSettings> {
    searches::set_active(&lock(&state), id)
}

#[tauri::command]
pub fn set_search_alerts(
    state: State<'_, AppState>,
    id: i64,
    on: bool,
) -> AppResult<Vec<SavedSearch>> {
    let conn = lock(&state);
    searches::set_alerts(&conn, id, on)?;
    searches::list(&conn)
}

// ---- Background refresh + alerts ----

#[tauri::command]
pub fn get_alert_settings(state: State<'_, AppState>) -> AppResult<AlertSettings> {
    alerts::get_settings(&lock(&state))
}

#[tauri::command]
pub fn save_alert_settings(
    state: State<'_, AppState>,
    settings: AlertSettings,
) -> AppResult<AlertSettings> {
    alerts::save_settings(&lock(&state), &settings)
}

/// Shows a sample notification so the user can allow notifications for JobLens.
#[tauri::command]
pub fn send_test_alert(app: tauri::AppHandle) -> AppResult<()> {
    alerts::notify(
        &app,
        "JobLens alerts are on",
        "You'll hear about great new matches like this.",
    )
}
