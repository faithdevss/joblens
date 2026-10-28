//! Source adapters: fetch a public API or feed and turn it into [`RawJob`]s.
//!
//! Parsing is split from fetching so each adapter is tested on a saved response.
//! Errors are plain sentences for the Sources panel; one source failing never
//! affects another.

use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

use quick_xml::{Reader, events::Event};
use regex::Regex;
use serde_json::Value;
use tokio::sync::Semaphore;

use super::{RawJob, SalaryPeriod, dates, salary, text};

pub type FetchResult<T> = Result<T, String>;
type Parser = fn(&str) -> FetchResult<Vec<RawJob>>;

// Some boards are slow: Lever takes ~20 s to send a large company's postings.
const TIMEOUT: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = concat!(
    "JobLens/",
    env!("CARGO_PKG_VERSION"),
    " (desktop job finder; +https://github.com/faithdevs/joblens)"
);

pub const MANUAL_SOURCE_ID: &str = "manual";

/// The company boards we can read, for messages.
const BOARD_NAMES: &str = "Greenhouse, Lever, Ashby, Workable, Recruitee or SmartRecruiters";

// SmartRecruiters lists postings without descriptions, so each one is fetched on its own.
const SMARTRECRUITERS_PAGE: usize = 100;
const SMARTRECRUITERS_MAX_PAGES: usize = 10;
const SMARTRECRUITERS_PARALLEL: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ats {
    Greenhouse,
    Lever,
    Ashby,
    Workable,
    Recruitee,
    SmartRecruiters,
}

impl Ats {
    /// In the order a company name is tried; SmartRecruiters last as it costs the most.
    pub const ALL: [Ats; 6] = [
        Ats::Greenhouse,
        Ats::Lever,
        Ats::Ashby,
        Ats::Workable,
        Ats::Recruitee,
        Ats::SmartRecruiters,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Greenhouse => "greenhouse",
            Self::Lever => "lever",
            Self::Ashby => "ashby",
            Self::Workable => "workable",
            Self::Recruitee => "recruitee",
            Self::SmartRecruiters => "smartrecruiters",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.as_str() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Greenhouse => "Greenhouse",
            Self::Lever => "Lever",
            Self::Ashby => "Ashby",
            Self::Workable => "Workable",
            Self::Recruitee => "Recruitee",
            Self::SmartRecruiters => "SmartRecruiters",
        }
    }

    /// The board's listing. SmartRecruiters' is the first page of remote postings.
    fn board_url(self, token: &str) -> String {
        match self {
            Self::Greenhouse => {
                format!("https://boards-api.greenhouse.io/v1/boards/{token}/jobs?content=true")
            }
            Self::Lever => format!("https://api.lever.co/v0/postings/{token}?mode=json"),
            Self::Ashby => format!(
                "https://api.ashbyhq.com/posting-api/job-board/{token}?includeCompensation=true"
            ),
            Self::Workable => {
                format!("https://apply.workable.com/api/v1/widget/accounts/{token}?details=true")
            }
            Self::Recruitee => format!("https://{token}.recruitee.com/api/offers/"),
            Self::SmartRecruiters => format!(
                "https://api.smartrecruiters.com/v1/companies/{token}/postings?limit={SMARTRECRUITERS_PAGE}&locationType=REMOTE"
            ),
        }
    }
}

/// What a source row's id and config say to fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Adapter {
    Remotive,
    RemoteOk,
    Wwr,
    Jobicy,
    Ats { ats: Ats, token: String },
}

impl Adapter {
    /// Built-in boards by id; watchlist companies by their stored config.
    pub fn for_source(id: &str, config: &str) -> Option<Self> {
        match id {
            "remotive" => return Some(Self::Remotive),
            "remoteok" => return Some(Self::RemoteOk),
            "wwr" => return Some(Self::Wwr),
            "jobicy" => return Some(Self::Jobicy),
            _ => {}
        }
        let config: Value = serde_json::from_str(config).ok()?;
        let ats = Ats::parse(config.get("ats")?.as_str()?)?;
        let token = config.get("token")?.as_str()?.to_string();
        Some(Self::Ats { ats, token })
    }

    /// Shortest wait between fetches, from each source's published guidance.
    pub fn min_interval_secs(&self) -> i64 {
        match self {
            // Remotive asks for at most four requests a day.
            Self::Remotive => 6 * 3600,
            // One request per posting, so a large company is a lot of requests.
            Self::Ats {
                ats: Ats::SmartRecruiters,
                ..
            } => 3 * 3600,
            // WWR's feed has a 60-minute TTL.
            Self::RemoteOk | Self::Wwr | Self::Jobicy | Self::Ats { .. } => 3600,
        }
    }

    pub async fn fetch(&self, client: &reqwest::Client) -> FetchResult<Vec<RawJob>> {
        let (url, parse): (&str, Parser) = match self {
            Self::Remotive => ("https://remotive.com/api/remote-jobs", parse_remotive),
            Self::RemoteOk => ("https://remoteok.com/api", parse_remoteok),
            Self::Wwr => ("https://weworkremotely.com/remote-jobs.rss", parse_wwr),
            Self::Jobicy => (
                "https://jobicy.com/api/v2/remote-jobs?count=100",
                parse_jobicy,
            ),
            Self::Ats { ats, token } => return Ok(fetch_board(client, *ats, token).await?.0),
        };
        parse(&get(client, url).await?)
    }
}

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .expect("http client config is valid")
}

async fn get(client: &reqwest::Client, url: &str) -> FetchResult<String> {
    let res = client.get(url).send().await.map_err(describe)?;
    let status = res.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            404 => not_found(),
            429 => "The site asked us to slow down (429). Try again later.".into(),
            code => format!("The site returned an error (HTTP {code})."),
        });
    }
    res.text().await.map_err(describe)
}

fn describe(e: reqwest::Error) -> String {
    if e.is_timeout() {
        format!("Timed out after {} s.", TIMEOUT.as_secs())
    } else if e.is_connect() {
        "Couldn’t connect. Check your internet connection.".into()
    } else if e.is_decode() || e.is_body() {
        "The connection dropped while downloading.".into()
    } else {
        "The request failed.".into()
    }
}

fn not_found() -> String {
    "Not found (404). The board may have moved or closed.".into()
}

fn unreadable() -> String {
    "The site sent data we couldn’t read. Its format may have changed.".into()
}

fn json(body: &str) -> FetchResult<Value> {
    serde_json::from_str(body).map_err(|_| unreadable())
}

fn str_at<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("").trim()
}

fn id_at(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// A number sent as a number or as a string, as Recruitee does.
fn num_at(v: &Value, key: &str) -> Option<f64> {
    match v.get(key)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// "Engineering" from `{"department": {"label": "Engineering"}}`.
fn label_at<'a>(v: &'a Value, key: &str, field: &str) -> &'a str {
    v.get(key).map(|o| str_at(o, field)).unwrap_or("")
}

fn non_empty(parts: &[&str]) -> Vec<String> {
    parts
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn strings_at(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn is_remote_text(s: &str) -> bool {
    s.to_lowercase().contains("remote")
}

// ---- Job boards ----

pub fn parse_remotive(body: &str) -> FetchResult<Vec<RawJob>> {
    let v = json(body)?;
    let jobs = v
        .get("jobs")
        .and_then(Value::as_array)
        .ok_or_else(unreadable)?;
    Ok(jobs
        .iter()
        .map(|j| RawJob {
            external_id: id_at(j, "id"),
            title: text::decode_entities(str_at(j, "title")),
            company: text::decode_entities(str_at(j, "company_name")),
            url: str_at(j, "url").to_string(),
            description: text::html_to_text(str_at(j, "description")),
            location: str_at(j, "candidate_required_location").to_string(),
            salary: salary::parse(str_at(j, "salary")),
            posted_at: dates::from_iso(str_at(j, "publication_date")),
            tags: strings_at(j, "tags"),
        })
        .filter(|j| !j.external_id.is_empty() && !j.title.is_empty())
        .collect())
}

pub fn parse_remoteok(body: &str) -> FetchResult<Vec<RawJob>> {
    let v = json(body)?;
    let items = v.as_array().ok_or_else(unreadable)?;
    // The first item is the API's legal notice, not a job.
    Ok(items
        .iter()
        .filter(|j| j.get("position").is_some())
        .map(|j| {
            let fixed = |k: &str| text::fix_mojibake(str_at(j, k));
            let num = |k: &str| j.get(k).and_then(Value::as_f64);
            RawJob {
                external_id: id_at(j, "id"),
                title: text::decode_entities(&fixed("position")),
                company: text::decode_entities(&fixed("company")),
                url: str_at(j, "url").to_string(),
                description: text::html_to_text(&fixed("description")),
                location: fixed("location"),
                salary: salary::from_range(
                    num("salary_min"),
                    num("salary_max"),
                    Some("USD"),
                    Some(SalaryPeriod::Year),
                ),
                posted_at: dates::from_iso(str_at(j, "date")).or_else(|| {
                    j.get("epoch")
                        .and_then(Value::as_i64)
                        .and_then(dates::from_epoch_secs)
                }),
                tags: strings_at(j, "tags"),
            }
        })
        .filter(|j| !j.external_id.is_empty() && !j.title.is_empty())
        .collect())
}

/// We Work Remotely RSS. Titles are "Company: Job title".
pub fn parse_wwr(body: &str) -> FetchResult<Vec<RawJob>> {
    let mut reader = Reader::from_str(body);
    let mut jobs = Vec::new();
    let mut in_item = false;
    let mut field: Option<String> = None;
    let mut fields: Vec<(String, String)> = Vec::new();
    loop {
        match reader.read_event().map_err(|_| unreadable())? {
            Event::Start(e) => {
                let name = e.name().as_ref().to_string();
                if name == "item" {
                    in_item = true;
                    fields.clear();
                } else if in_item {
                    fields.push((name.clone(), String::new()));
                    field = Some(name);
                }
            }
            Event::Text(t) if field.is_some() => {
                if let Some((_, v)) = fields.last_mut() {
                    v.push_str(&t.xml10_content());
                }
            }
            Event::CData(t) if field.is_some() => {
                if let Some((_, v)) = fields.last_mut() {
                    v.push_str(&t.into_inner());
                }
            }
            // Kept as written and decoded below with the rest of the text.
            Event::GeneralRef(r) if field.is_some() => {
                if let Some((_, v)) = fields.last_mut() {
                    v.push('&');
                    v.push_str(&r.into_inner());
                    v.push(';');
                }
            }
            Event::End(e) => {
                if e.name().as_ref() == "item" {
                    in_item = false;
                    if let Some(job) = wwr_item(&fields) {
                        jobs.push(job);
                    }
                }
                field = None;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(jobs)
}

fn wwr_item(fields: &[(String, String)]) -> Option<RawJob> {
    let get = |k: &str| {
        fields
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| text::decode_entities(v).trim().to_string())
            .unwrap_or_default()
    };
    let full_title = get("title");
    let (company, title) = full_title
        .split_once(": ")
        .map(|(c, t)| (c.trim().to_string(), t.trim().to_string()))
        .unwrap_or_else(|| (String::new(), full_title.clone()));
    let link = get("link");
    let guid = get("guid");
    let external_id = if guid.is_empty() { link.clone() } else { guid };
    if external_id.is_empty() || title.is_empty() {
        return None;
    }
    let mut tags: Vec<String> = get("skills")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let category = get("category");
    if !category.is_empty() {
        tags.push(category);
    }
    let region = get("region");
    let location = if region.is_empty() {
        get("country")
    } else {
        region
    };
    Some(RawJob {
        external_id,
        title,
        company,
        url: link,
        description: text::html_to_text(&get("description")),
        location,
        salary: None,
        posted_at: dates::from_rfc2822(&get("pubDate")),
        tags,
    })
}

pub fn parse_jobicy(body: &str) -> FetchResult<Vec<RawJob>> {
    let v = json(body)?;
    let jobs = v
        .get("jobs")
        .and_then(Value::as_array)
        .ok_or_else(unreadable)?;
    Ok(jobs
        .iter()
        .map(|j| RawJob {
            external_id: id_at(j, "id"),
            title: text::decode_entities(str_at(j, "jobTitle")),
            company: text::decode_entities(str_at(j, "companyName")),
            url: str_at(j, "url").to_string(),
            description: text::html_to_text(str_at(j, "jobDescription")),
            location: str_at(j, "jobGeo").to_string(),
            salary: salary::from_range(
                num_at(j, "salaryMin"),
                num_at(j, "salaryMax"),
                Some(str_at(j, "salaryCurrency")).filter(|c| !c.is_empty()),
                match str_at(j, "salaryPeriod") {
                    "yearly" => Some(SalaryPeriod::Year),
                    "monthly" => Some(SalaryPeriod::Month),
                    "hourly" => Some(SalaryPeriod::Hour),
                    _ => None,
                },
            ),
            posted_at: dates::from_iso(str_at(j, "pubDate")),
            tags: strings_at(j, "jobIndustry"),
        })
        .filter(|j| !j.external_id.is_empty() && !j.title.is_empty())
        .collect())
}

// ---- Company boards (watchlist) ----

/// A company's remote roles and, when the board states it, the company's name.
async fn fetch_board(
    client: &reqwest::Client,
    ats: Ats,
    token: &str,
) -> FetchResult<(Vec<RawJob>, Option<String>)> {
    if ats == Ats::SmartRecruiters {
        return fetch_smartrecruiters(client, token).await;
    }
    let body = get(client, &ats.board_url(token)).await?;
    Ok((parse_board(ats, &body)?, board_company(ats, &body)))
}

/// The postings in a board's listing, in that board's own JSON shape.
fn board_items(ats: Ats, v: &Value) -> Option<&Vec<Value>> {
    match ats {
        Ats::Greenhouse | Ats::Ashby | Ats::Workable => v.get("jobs")?.as_array(),
        Ats::Lever => v.as_array(),
        Ats::Recruitee => v.get("offers")?.as_array(),
        Ats::SmartRecruiters => v.get("content")?.as_array(),
    }
}

/// A company's whole board, keeping remote roles only.
pub fn parse_board(ats: Ats, body: &str) -> FetchResult<Vec<RawJob>> {
    let v = json(body)?;
    let items = board_items(ats, &v).ok_or_else(unreadable)?;
    Ok(merge_locations(
        items
            .iter()
            .filter(|j| is_remote(ats, j))
            .filter_map(|j| board_job(ats, j)),
    ))
}

/// Workable lists a job once per location; this makes it one job with all of them.
fn merge_locations(jobs: impl Iterator<Item = RawJob>) -> Vec<RawJob> {
    let mut out: Vec<RawJob> = Vec::new();
    for job in jobs {
        match out.iter_mut().find(|j| j.external_id == job.external_id) {
            Some(j) => super::dedupe::add_places(&mut j.location, &job.location),
            None => out.push(job),
        }
    }
    out
}

/// SmartRecruiters' listing is paged and leaves out descriptions, so each remote
/// posting is fetched on its own, a few at a time.
async fn fetch_smartrecruiters(
    client: &reqwest::Client,
    token: &str,
) -> FetchResult<(Vec<RawJob>, Option<String>)> {
    let first_page = Ats::SmartRecruiters.board_url(token);
    let mut items: Vec<Value> = Vec::new();
    for page in 0..SMARTRECRUITERS_MAX_PAGES {
        let v = json(
            &get(
                client,
                &format!("{first_page}&offset={}", page * SMARTRECRUITERS_PAGE),
            )
            .await?,
        )?;
        let content = board_items(Ats::SmartRecruiters, &v).ok_or_else(unreadable)?;
        let total = v.get("totalFound").and_then(Value::as_u64).unwrap_or(0) as usize;
        items.extend(content.iter().cloned());
        if content.is_empty() || items.len() >= total {
            break;
        }
    }
    if items.is_empty() {
        // Any name gets an empty list back; one with no postings at all isn't a board.
        let v = json(
            &get(
                client,
                &format!("https://api.smartrecruiters.com/v1/companies/{token}/postings?limit=1"),
            )
            .await?,
        )?;
        if v.get("totalFound").and_then(Value::as_u64) == Some(0) {
            return Err(not_found());
        }
    }
    let company = items
        .first()
        .map(|j| label_at(j, "company", "name").to_string())
        .filter(|s| !s.is_empty());
    let parallel = Arc::new(Semaphore::new(SMARTRECRUITERS_PARALLEL));
    let mut tasks = Vec::new();
    for item in items
        .into_iter()
        .filter(|j| is_remote(Ats::SmartRecruiters, j))
    {
        let (client, parallel) = (client.clone(), parallel.clone());
        let url = smartrecruiters_posting_url(token, &id_at(&item, "id"));
        tasks.push(tauri::async_runtime::spawn(async move {
            let _turn = parallel.acquire().await;
            // Without its description the posting is still worth listing.
            let detail: Option<Value> = match get(&client, &url).await {
                Ok(body) => serde_json::from_str(&body).ok(),
                Err(_) => None,
            };
            board_job(Ats::SmartRecruiters, detail.as_ref().unwrap_or(&item))
        }));
    }
    let mut jobs = Vec::new();
    for task in tasks {
        if let Ok(Some(job)) = task.await {
            jobs.push(job);
        }
    }
    Ok((jobs, company))
}

fn smartrecruiters_posting_url(token: &str, id: &str) -> String {
    format!("https://api.smartrecruiters.com/v1/companies/{token}/postings/{id}")
}

fn is_remote(ats: Ats, j: &Value) -> bool {
    match ats {
        Ats::Greenhouse => {
            is_remote_text(str_at(j, "title"))
                || j.get("location")
                    .is_some_and(|l| is_remote_text(str_at(l, "name")))
        }
        Ats::Lever => {
            str_at(j, "workplaceType").eq_ignore_ascii_case("remote")
                || j.get("categories")
                    .is_some_and(|c| is_remote_text(str_at(c, "location")))
        }
        Ats::Ashby => {
            j.get("isRemote").and_then(Value::as_bool) == Some(true)
                || str_at(j, "workplaceType").eq_ignore_ascii_case("remote")
                || is_remote_text(str_at(j, "location"))
        }
        Ats::Workable => {
            j.get("telecommuting").and_then(Value::as_bool) == Some(true)
                || is_remote_text(str_at(j, "city"))
        }
        Ats::Recruitee => {
            j.get("remote").and_then(Value::as_bool) == Some(true)
                || is_remote_text(str_at(j, "location"))
        }
        Ats::SmartRecruiters => {
            j.get("location")
                .and_then(|l| l.get("remote"))
                .and_then(Value::as_bool)
                == Some(true)
        }
    }
}

/// One posting from a company board, in that board's own JSON shape.
pub fn board_job(ats: Ats, j: &Value) -> Option<RawJob> {
    let job = match ats {
        Ats::Greenhouse => RawJob {
            external_id: id_at(j, "id"),
            title: str_at(j, "title").to_string(),
            company: str_at(j, "company_name").to_string(),
            url: str_at(j, "absolute_url").to_string(),
            // Greenhouse sends the HTML entity-escaped.
            description: text::html_to_text(&text::decode_entities(str_at(j, "content"))),
            location: j
                .get("location")
                .map(|l| str_at(l, "name"))
                .unwrap_or("")
                .to_string(),
            salary: None,
            posted_at: dates::from_iso(str_at(j, "first_published"))
                .or_else(|| dates::from_iso(str_at(j, "updated_at"))),
            tags: j
                .get("departments")
                .and_then(Value::as_array)
                .map(|d| {
                    d.iter()
                        .map(|d| str_at(d, "name").to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
        },
        Ats::Lever => {
            let cats = j.get("categories").cloned().unwrap_or(Value::Null);
            let mut locations = strings_at(&cats, "allLocations");
            if locations.is_empty() && !str_at(&cats, "location").is_empty() {
                locations.push(str_at(&cats, "location").to_string());
            }
            let mut parts = vec![text::html_to_text(str_at(j, "description"))];
            for list in j
                .get("lists")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                parts.push(format!(
                    "{}\n{}",
                    str_at(list, "text"),
                    text::html_to_text(&format!("<ul>{}</ul>", str_at(list, "content")))
                ));
            }
            parts.push(text::html_to_text(str_at(j, "additional")));
            let salary = j.get("salaryRange").and_then(|r| {
                let period = match str_at(r, "interval") {
                    "per-year-salary" => Some(SalaryPeriod::Year),
                    "per-month-salary" => Some(SalaryPeriod::Month),
                    "per-hour-wage" => Some(SalaryPeriod::Hour),
                    _ => None,
                };
                salary::from_range(
                    r.get("min").and_then(Value::as_f64),
                    r.get("max").and_then(Value::as_f64),
                    Some(str_at(r, "currency")).filter(|c| !c.is_empty()),
                    period,
                )
            });
            RawJob {
                external_id: id_at(j, "id"),
                title: str_at(j, "text").to_string(),
                company: String::new(),
                url: str_at(j, "hostedUrl").to_string(),
                description: text::tidy(
                    &parts
                        .into_iter()
                        .filter(|p| !p.trim().is_empty())
                        .collect::<Vec<_>>()
                        .join("\n\n"),
                ),
                location: locations.join("; "),
                salary,
                posted_at: j
                    .get("createdAt")
                    .and_then(Value::as_i64)
                    .and_then(dates::from_epoch_millis),
                tags: [str_at(&cats, "department"), str_at(&cats, "team")]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
            }
        }
        Ats::Ashby => {
            let mut locations = vec![str_at(j, "location").to_string()];
            for l in j
                .get("secondaryLocations")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let name = str_at(l, "location");
                if !name.is_empty() && !locations.iter().any(|x| x == name) {
                    locations.push(name.to_string());
                }
            }
            let description = match str_at(j, "descriptionHtml") {
                "" => text::tidy(str_at(j, "descriptionPlain")),
                html => text::html_to_text(html),
            };
            let salary = j
                .get("compensation")
                .map(|c| str_at(c, "scrapeableCompensationSalarySummary"))
                .and_then(salary::parse);
            RawJob {
                external_id: id_at(j, "id"),
                title: str_at(j, "title").to_string(),
                company: String::new(),
                url: str_at(j, "jobUrl").to_string(),
                description,
                location: locations
                    .into_iter()
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join("; "),
                salary,
                posted_at: dates::from_iso(str_at(j, "publishedAt")),
                tags: [str_at(j, "department"), str_at(j, "team")]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
            }
        }
        Ats::Workable => {
            let place = non_empty(&[str_at(j, "city"), str_at(j, "state"), str_at(j, "country")])
                .join(", ");
            let remote = j.get("telecommuting").and_then(Value::as_bool) == Some(true);
            RawJob {
                external_id: id_at(j, "shortcode"),
                title: str_at(j, "title").to_string(),
                company: String::new(),
                url: str_at(j, "url").to_string(),
                description: text::html_to_text(str_at(j, "description")),
                location: match (remote, place.is_empty()) {
                    (true, true) => "Remote".into(),
                    (true, false) => format!("Remote, {place}"),
                    (false, _) => place,
                },
                salary: None,
                posted_at: dates::from_iso(str_at(j, "published_on"))
                    .or_else(|| dates::from_iso(str_at(j, "created_at"))),
                tags: non_empty(&[str_at(j, "department"), str_at(j, "function")]),
            }
        }
        Ats::Recruitee => {
            let mut countries: Vec<&str> = Vec::new();
            for l in j
                .get("locations")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let country = str_at(l, "country");
                if !country.is_empty() && !countries.contains(&country) {
                    countries.push(country);
                }
            }
            // A remote offer's location reads "Remote job"; its countries say where.
            let location = if j.get("remote").and_then(Value::as_bool) == Some(true)
                && !countries.is_empty()
            {
                format!("Remote - {}", countries.join(", "))
            } else {
                str_at(j, "location").to_string()
            };
            let salary = j.get("salary").and_then(|s| {
                let period = match str_at(s, "period") {
                    "year" => Some(SalaryPeriod::Year),
                    "month" => Some(SalaryPeriod::Month),
                    "hour" => Some(SalaryPeriod::Hour),
                    _ => None,
                };
                salary::from_range(
                    num_at(s, "min"),
                    num_at(s, "max"),
                    Some(str_at(s, "currency")).filter(|c| !c.is_empty()),
                    period,
                )
            });
            let mut tags = non_empty(&[str_at(j, "department")]);
            tags.extend(strings_at(j, "tags"));
            RawJob {
                external_id: id_at(j, "id"),
                title: str_at(j, "title").to_string(),
                company: str_at(j, "company_name").to_string(),
                url: str_at(j, "careers_url").to_string(),
                description: text::tidy(
                    &[str_at(j, "description"), str_at(j, "requirements")]
                        .into_iter()
                        .map(text::html_to_text)
                        .filter(|p| !p.trim().is_empty())
                        .collect::<Vec<_>>()
                        .join("\n\n"),
                ),
                location,
                salary,
                // "2026-10-15 09:31:36 UTC"
                posted_at: dates::from_iso(str_at(j, "published_at").trim_end_matches(" UTC")),
                tags,
            }
        }
        // A listing entry, or the fuller single posting that adds `jobAd` and `postingUrl`.
        Ats::SmartRecruiters => {
            let place = label_at(j, "location", "fullLocation")
                .split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty() && !p.eq_ignore_ascii_case("remote"))
                .collect::<Vec<_>>()
                .join(", ");
            let remote = is_remote(ats, j);
            let id = id_at(j, "id");
            let url = match str_at(j, "postingUrl") {
                "" => format!(
                    "https://jobs.smartrecruiters.com/{}/{id}",
                    label_at(j, "company", "identifier")
                ),
                u => u.to_string(),
            };
            let sections = j.get("jobAd").and_then(|a| a.get("sections"));
            let description = [
                "jobDescription",
                "qualifications",
                "additionalInformation",
                "companyDescription",
            ]
            .into_iter()
            .filter_map(|k| sections?.get(k))
            .map(|s| (str_at(s, "title"), text::html_to_text(str_at(s, "text"))))
            // A section with a heading but no text is left out.
            .filter(|(_, body)| !body.trim().is_empty())
            .map(|(title, body)| format!("{title}\n{body}"))
            .collect::<Vec<_>>()
            .join("\n\n");
            RawJob {
                external_id: id,
                title: str_at(j, "name").to_string(),
                company: label_at(j, "company", "name").to_string(),
                url,
                description: text::tidy(&description),
                location: match (remote, place.is_empty()) {
                    (true, true) => "Remote".into(),
                    (true, false) => format!("Remote, {place}"),
                    (false, _) => place,
                },
                salary: None,
                posted_at: dates::from_iso(str_at(j, "releasedDate")),
                tags: non_empty(&[
                    label_at(j, "department", "label"),
                    label_at(j, "function", "label"),
                ]),
            }
        }
    };
    (!job.external_id.is_empty() && !job.title.is_empty()).then_some(job)
}

// ---- Finding a company's board ----

static GREENHOUSE_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:boards-api\.greenhouse\.io/v1/boards/|(?:job-)?boards\.greenhouse\.io/(?:embed/job_board\?for=)?)([a-z0-9_-]+)").unwrap()
});
static LEVER_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:api\.lever\.co/v0/postings/|jobs\.lever\.co/)([a-z0-9_.-]+)").unwrap()
});
static ASHBY_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:jobs\.ashbyhq\.com/|api\.ashbyhq\.com/posting-api/job-board/)([a-z0-9_.%-]+)",
    )
    .unwrap()
});
static WORKABLE_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)apply\.workable\.com/(?:api/v1/widget/accounts/)?([a-z0-9_-]+)").unwrap()
});
// The older acme.workable.com boards.
static WORKABLE_SUBDOMAIN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:^|//)([a-z0-9-]+)\.workable\.com").unwrap());
static RECRUITEE_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)([a-z0-9-]+)\.recruitee\.com").unwrap());
static SMARTRECRUITERS_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:(?:jobs|careers)\.smartrecruiters\.com/|api\.smartrecruiters\.com/v1/companies/)([a-z0-9_-]+)").unwrap()
});

/// Path prefixes and hosts that a board pattern can catch but that aren't boards.
const NOT_BOARDS: [&str; 5] = ["embed", "j", "api", "apply", "www"];

/// A board link the user pasted, if it's one we recognize.
pub fn board_from_url(input: &str) -> Option<(Ats, String)> {
    let input = input.trim();
    [
        (Ats::Greenhouse, &*GREENHOUSE_URL),
        (Ats::Lever, &*LEVER_URL),
        (Ats::Ashby, &*ASHBY_URL),
        (Ats::Workable, &*WORKABLE_URL),
        (Ats::Workable, &*WORKABLE_SUBDOMAIN),
        (Ats::Recruitee, &*RECRUITEE_URL),
        (Ats::SmartRecruiters, &*SMARTRECRUITERS_URL),
    ]
    .into_iter()
    .find_map(|(ats, re)| {
        let token = re.captures(input)?.get(1)?.as_str();
        (!NOT_BOARDS.contains(&token.to_lowercase().as_str())).then(|| (ats, token.to_string()))
    })
}

/// Board names to try for a company name: "Acme Corp" → ["acmecorp", "acme-corp"].
pub fn token_guesses(name: &str) -> Vec<String> {
    let words: Vec<String> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let mut out = vec![words.concat()];
    if words.len() > 1 {
        out.push(words.join("-"));
    }
    out.retain(|t| !t.is_empty());
    out.dedup();
    out
}

/// A company board we found, with its jobs so the first fetch is already done.
pub struct FoundBoard {
    pub ats: Ats,
    pub token: String,
    pub company: String,
    pub jobs: Vec<RawJob>,
}

/// Finds a company's public board from a link or a name, trying each ATS in turn.
pub async fn find_board(client: &reqwest::Client, input: &str) -> FetchResult<FoundBoard> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Enter a company name or careers page link.".into());
    }
    let candidates: Vec<(Ats, String)> = match board_from_url(input) {
        Some(found) => vec![found],
        None if input.contains("://") || input.contains('/') => {
            return Err(format!(
                "That link isn’t a {BOARD_NAMES} board. Try the company name instead."
            ));
        }
        None => token_guesses(input)
            .into_iter()
            .flat_map(|t| Ats::ALL.into_iter().map(move |a| (a, t.clone())))
            .collect(),
    };
    let mut last_error = None;
    for (ats, token) in candidates {
        match fetch_board(client, ats, &token).await {
            Ok((jobs, company)) => {
                let company = company.unwrap_or_else(|| {
                    if board_from_url(input).is_some() {
                        title_case_token(&token)
                    } else {
                        input.to_string()
                    }
                });
                return Ok(FoundBoard {
                    ats,
                    token,
                    company,
                    jobs,
                });
            }
            Err(e) if e.starts_with("Not found") => {}
            Err(e) => last_error = Some(e),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        format!("Couldn’t find a {BOARD_NAMES} board for “{input}”. Paste its careers page link instead.")
    }))
}

/// "acme-corp" → "Acme Corp", a readable stand-in until the user renames it.
fn title_case_token(token: &str) -> String {
    token
        .split(['-', '_', '.'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Company name as the board's listing states it, where it does.
fn board_company(ats: Ats, body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    let name = match ats {
        Ats::Greenhouse | Ats::Recruitee => str_at(board_items(ats, &v)?.first()?, "company_name"),
        Ats::Workable => str_at(&v, "name"),
        Ats::Lever | Ats::Ashby | Ats::SmartRecruiters => "",
    };
    (!name.is_empty()).then(|| name.to_string())
}

// ---- Manual add ----

static GREENHOUSE_JOB: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)greenhouse\.io/(?:v1/boards/)?([a-z0-9_-]+)/jobs/(\d+)").unwrap()
});
static LEVER_JOB: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)lever\.co/(?:v0/postings/)?([a-z0-9_.-]+)/([0-9a-f]{8}-[0-9a-f-]{27})")
        .unwrap()
});
static ASHBY_JOB: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)ashbyhq\.com/([a-z0-9_.%-]+)/([0-9a-f]{8}-[0-9a-f-]{27})").unwrap()
});
static WORKABLE_JOB: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)apply\.workable\.com/([a-z0-9_-]+)/j/([0-9a-f]{10})\b").unwrap()
});
static RECRUITEE_JOB: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)([a-z0-9-]+)\.recruitee\.com/o/([a-z0-9-]+)").unwrap());
static SMARTRECRUITERS_JOB: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:jobs|careers)\.smartrecruiters\.com/([a-z0-9_-]+)/(\d+)").unwrap()
});

/// A single posting link we can fetch: (ATS, board, posting id).
/// The id is Workable's shortcode and Recruitee's slug, as their links carry those.
pub fn posting_from_url(url: &str) -> Option<(Ats, String, String)> {
    [
        (Ats::Greenhouse, &*GREENHOUSE_JOB),
        (Ats::Lever, &*LEVER_JOB),
        (Ats::Ashby, &*ASHBY_JOB),
        (Ats::Workable, &*WORKABLE_JOB),
        (Ats::Recruitee, &*RECRUITEE_JOB),
        (Ats::SmartRecruiters, &*SMARTRECRUITERS_JOB),
    ]
    .into_iter()
    .find_map(|(ats, re)| {
        let c = re.captures(url)?;
        Some((ats, c[1].to_string(), c[2].to_lowercase()))
    })
}

/// Fetches one posting from a supported link. Remote or not, the user chose it.
pub async fn fetch_posting(client: &reqwest::Client, url: &str) -> FetchResult<RawJob> {
    let Some((ats, token, id)) = posting_from_url(url) else {
        return Err(format!(
            "We can only read {BOARD_NAMES} job links. Paste the description instead."
        ));
    };
    let job = match ats {
        Ats::Greenhouse => {
            let body = get(
                client,
                &format!("https://boards-api.greenhouse.io/v1/boards/{token}/jobs/{id}"),
            )
            .await?;
            board_job(ats, &json(&body)?)
        }
        Ats::Lever => {
            let body = get(
                client,
                &format!("https://api.lever.co/v0/postings/{token}/{id}"),
            )
            .await?;
            board_job(ats, &json(&body)?)
        }
        Ats::SmartRecruiters => {
            let body = get(client, &smartrecruiters_posting_url(&token, &id)).await?;
            board_job(ats, &json(&body)?)
        }
        // No single-posting endpoint; find it on the board.
        Ats::Ashby | Ats::Workable | Ats::Recruitee => {
            let body = get(client, &ats.board_url(&token)).await?;
            let v = json(&body)?;
            let key = match ats {
                Ats::Workable => "shortcode",
                Ats::Recruitee => "slug",
                _ => "id",
            };
            board_items(ats, &v).and_then(|items| {
                merge_locations(
                    items
                        .iter()
                        .filter(|j| str_at(j, key).eq_ignore_ascii_case(&id))
                        .filter_map(|j| board_job(ats, j)),
                )
                .into_iter()
                .next()
            })
        }
    };
    let mut job =
        job.ok_or_else(|| "That posting wasn’t found. It may have closed.".to_string())?;
    if job.company.is_empty() {
        job.company = token;
    }
    Ok(job)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/fixtures/sources/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    #[test]
    fn reads_remotive() {
        let jobs = parse_remotive(&fixture("remotive.json")).unwrap();
        assert_eq!(jobs.len(), 5);
        let j = &jobs[0];
        assert_eq!(j.external_id, "2091158");
        assert_eq!(j.company, "Askable");
        assert_eq!(j.location, "Worldwide");
        assert_eq!(j.posted_at.as_deref(), Some("2026-10-17T16:04:29Z"));
        assert!(j.url.starts_with("https://remotive.com/"));
        assert!(!j.description.contains('<'));
        assert!(j.tags.iter().any(|t| t == "C++"));
    }

    #[test]
    fn reads_remoteok_and_skips_legal_notice() {
        let jobs = parse_remoteok(&fixture("remoteok.json")).unwrap();
        assert_eq!(jobs.len(), 6);
        assert!(jobs.iter().all(|j| !j.description.contains("â€")));
        let lemon = jobs.iter().find(|j| j.company == "Lemon.io").unwrap();
        let s = lemon.salary.as_ref().unwrap();
        assert_eq!(
            (s.min, s.max, s.currency.as_deref()),
            (Some(50_000.0), Some(150_000.0), Some("USD"))
        );
        assert!(
            jobs.iter()
                .find(|j| j.company == "Engine")
                .unwrap()
                .salary
                .is_none()
        );
        assert!(jobs[0].description.contains("we’re transforming"));
    }

    #[test]
    fn reads_wwr_rss() {
        let jobs = parse_wwr(&fixture("wwr.rss")).unwrap();
        assert_eq!(jobs.len(), 7);
        let j = &jobs[0];
        assert_eq!(j.company, "Committee for Children");
        assert_eq!(j.title, "Senior Product Manager - Edtech Innovation");
        assert_eq!(j.location, "Anywhere in the World");
        assert_eq!(j.posted_at.as_deref(), Some("2026-10-23T16:55:36Z"));
        assert!(j.url.starts_with("https://weworkremotely.com/remote-jobs/"));
        assert!(j.description.contains("Product Strategy & Vision"));
        assert!(!j.description.contains("&lt;") && !j.description.contains("<p>"));
    }

    #[test]
    fn reads_jobicy() {
        let jobs = parse_jobicy(&fixture("jobicy.json")).unwrap();
        assert_eq!(jobs.len(), 5);
        let j = &jobs[0];
        assert_eq!(j.external_id, "154015");
        assert_eq!(j.company, "ExtraHop");
        assert_eq!(j.location, "UK");
        assert_eq!(j.posted_at.as_deref(), Some("2026-10-25T04:25:15Z"));
        assert!(j.url.starts_with("https://jobicy.com/jobs/"));
        assert!(!j.description.contains('<'));
        assert_eq!(j.tags, ["Technical Support"]);
        let s = j.salary.as_ref().unwrap();
        assert_eq!(
            (s.min, s.max, s.currency.as_deref(), s.period),
            (
                Some(30_000.0),
                Some(35_000.0),
                Some("GBP"),
                Some(SalaryPeriod::Year)
            )
        );
        assert!(jobs[2].salary.is_none());
    }

    #[test]
    fn company_boards_keep_remote_roles_only() {
        let gh = parse_board(Ats::Greenhouse, &fixture("greenhouse.json")).unwrap();
        let titles: Vec<&str> = gh.iter().map(|j| j.title.as_str()).collect();
        assert!(
            !titles.contains(&"Benefits Analyst"),
            "Bangalore office role kept"
        );
        assert_eq!(gh.len(), 4);
        assert_eq!(gh[0].company, "GitLab");
        assert!(!gh[0].description.contains("&lt;") && !gh[0].description.contains("<div"));

        let lever = parse_board(Ats::Lever, &fixture("lever.json")).unwrap();
        assert_eq!(lever.len(), 1);
        assert!(
            lever[0].description.contains("What You'll Do") || !lever[0].description.is_empty()
        );

        let ashby = parse_board(Ats::Ashby, &fixture("ashby.json")).unwrap();
        assert_eq!(ashby.len(), 5);
        let em = &ashby[0];
        assert!(em.location.starts_with("Remote - European Union; Spain"));
        let s = em.salary.as_ref().unwrap();
        assert_eq!(
            (s.min, s.currency.as_deref()),
            (Some(150_000.0), Some("EUR"))
        );

        let recruitee = parse_board(Ats::Recruitee, &fixture("recruitee.json")).unwrap();
        assert_eq!(recruitee.len(), 1, "Utrecht office roles kept");
        let ae = &recruitee[0];
        assert_eq!(ae.title, "Account Executive - North America");
        assert_eq!(ae.company, "Channable");
        assert_eq!(ae.location, "Remote - United States");
        assert_eq!(ae.posted_at.as_deref(), Some("2026-08-06T16:09:53Z"));
        let s = ae.salary.as_ref().unwrap();
        assert_eq!(
            (s.min, s.max, s.currency.as_deref()),
            (Some(200_000.0), Some(250_000.0), Some("USD"))
        );
        assert!(!ae.description.contains('<'));
    }

    #[test]
    fn workable_lists_each_job_once() {
        let jobs = parse_board(Ats::Workable, &fixture("workable.json")).unwrap();
        assert_eq!(jobs.len(), 3);
        let j = &jobs[0];
        assert_eq!(j.external_id, "7B77708105");
        assert_eq!(
            j.location,
            "Remote, Portugal; Remote, Poland; Remote, Ukraine; Remote, Croatia; Remote, Serbia; Remote, Spain"
        );
        assert_eq!(j.url, "https://apply.workable.com/j/7B77708105");
        assert_eq!(j.posted_at.as_deref(), Some("2026-10-06T00:00:00Z"));
        assert!(!j.description.contains('<') && !j.description.is_empty());
        assert!(jobs[2].location.contains("Remote, lisbon, Portugal"));
        assert_eq!(
            board_company(Ats::Workable, &fixture("workable.json")).as_deref(),
            Some("RemoFirst")
        );
    }

    #[test]
    fn reads_smartrecruiters_listing_and_posting() {
        let listed = parse_board(Ats::SmartRecruiters, &fixture("smartrecruiters.json")).unwrap();
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[1].location, "Remote, Texas, United States");
        assert_eq!(listed[1].company, "ServiceNow");
        assert!(
            listed[1].description.is_empty(),
            "listings carry no description"
        );

        let posting: Value =
            serde_json::from_str(&fixture("smartrecruiters_posting.json")).unwrap();
        let j = board_job(Ats::SmartRecruiters, &posting).unwrap();
        assert_eq!(j.external_id, listed[0].external_id);
        assert_eq!(j.location, "Remote, Los Angeles, CALIFORNIA, United States");
        assert!(
            j.url
                .starts_with("https://jobs.smartrecruiters.com/ServiceNow/")
        );
        assert!(j.description.starts_with("Job Description"));
        assert!(j.description.contains("Qualifications"));
        assert!(!j.description.contains("<p>"));
        assert_eq!(j.posted_at.as_deref(), Some("2026-10-23T21:07:31Z"));
        assert_eq!(j.tags, ["Sales", "Consulting"]);
    }

    #[test]
    fn recognizes_board_links() {
        assert_eq!(
            board_from_url("https://job-boards.greenhouse.io/gitlab"),
            Some((Ats::Greenhouse, "gitlab".into()))
        );
        assert_eq!(
            board_from_url("https://boards.greenhouse.io/embed/job_board?for=figma"),
            Some((Ats::Greenhouse, "figma".into()))
        );
        assert_eq!(
            board_from_url("jobs.lever.co/spotify/"),
            Some((Ats::Lever, "spotify".into()))
        );
        assert_eq!(
            board_from_url("https://jobs.ashbyhq.com/ashby"),
            Some((Ats::Ashby, "ashby".into()))
        );
        assert_eq!(
            board_from_url("https://apply.workable.com/remofirst/"),
            Some((Ats::Workable, "remofirst".into()))
        );
        assert_eq!(
            board_from_url("https://zego.workable.com"),
            Some((Ats::Workable, "zego".into()))
        );
        assert_eq!(
            board_from_url("https://apply.workable.com/j/7B77708105"),
            None
        );
        assert_eq!(
            board_from_url("https://channable.recruitee.com/"),
            Some((Ats::Recruitee, "channable".into()))
        );
        assert_eq!(
            board_from_url("https://careers.smartrecruiters.com/ServiceNow"),
            Some((Ats::SmartRecruiters, "ServiceNow".into()))
        );
        assert_eq!(board_from_url("https://example.com/careers"), None);
        assert_eq!(token_guesses("Acme Corp."), vec!["acmecorp", "acme-corp"]);
        assert_eq!(token_guesses("GitLab"), vec!["gitlab"]);
        assert_eq!(title_case_token("acme-corp"), "Acme Corp");
    }

    #[test]
    fn recognizes_posting_links() {
        assert_eq!(
            posting_from_url("https://job-boards.greenhouse.io/gitlab/jobs/8860302002"),
            Some((Ats::Greenhouse, "gitlab".into(), "8860302002".into()))
        );
        assert_eq!(
            posting_from_url(
                "https://jobs.lever.co/spotify/2a55d6c9-c564-443a-be3e-a3851db5a93b/apply"
            ),
            Some((
                Ats::Lever,
                "spotify".into(),
                "2a55d6c9-c564-443a-be3e-a3851db5a93b".into()
            ))
        );
        assert_eq!(
            posting_from_url("https://jobs.ashbyhq.com/ashby/7458d4e9-da2e-47bd-98cb-adfda43d42b2"),
            Some((
                Ats::Ashby,
                "ashby".into(),
                "7458d4e9-da2e-47bd-98cb-adfda43d42b2".into()
            ))
        );
        assert_eq!(
            posting_from_url("https://apply.workable.com/remofirst/j/7B77708105/"),
            Some((Ats::Workable, "remofirst".into(), "7b77708105".into()))
        );
        assert_eq!(
            posting_from_url("https://channable.recruitee.com/o/account-executive-north-america"),
            Some((
                Ats::Recruitee,
                "channable".into(),
                "account-executive-north-america".into()
            ))
        );
        assert_eq!(
            posting_from_url(
                "https://jobs.smartrecruiters.com/ServiceNow/744000157570619-sr-advisory-solution-consultant"
            ),
            Some((
                Ats::SmartRecruiters,
                "ServiceNow".into(),
                "744000157570619".into()
            ))
        );
        assert_eq!(
            posting_from_url("https://remotive.com/remote-jobs/x-123"),
            None
        );
    }

    #[test]
    fn adapters_from_source_rows() {
        assert_eq!(
            Adapter::for_source("remotive", "{}"),
            Some(Adapter::Remotive)
        );
        assert_eq!(
            Adapter::for_source(
                "greenhouse:gitlab",
                r#"{"ats":"greenhouse","token":"gitlab"}"#
            ),
            Some(Adapter::Ats {
                ats: Ats::Greenhouse,
                token: "gitlab".into()
            })
        );
        assert_eq!(Adapter::for_source("jobicy", "{}"), Some(Adapter::Jobicy));
        assert_eq!(
            Adapter::for_source(
                "smartrecruiters:servicenow",
                r#"{"ats":"smartrecruiters","token":"ServiceNow"}"#
            )
            .map(|a| a.min_interval_secs()),
            Some(3 * 3600)
        );
        assert_eq!(Adapter::for_source("manual", "{}"), None);
    }

    fn board(ats: Ats, body: Value) -> Vec<RawJob> {
        parse_board(ats, &body.to_string()).unwrap()
    }

    #[test]
    fn jobicy_edge_cases() {
        assert!(parse_jobicy(r#"{"jobs": []}"#).unwrap().is_empty());
        assert!(parse_jobicy(r#"{"success": false}"#).is_err());
        assert!(parse_jobicy("<html>down</html>").is_err());
        let jobs = parse_jobicy(
            &serde_json::json!({"jobs": [
                {"id": 7, "jobTitle": "Dev &amp; Ops", "companyName": "A&amp;B",
                 "salaryMin": "40000", "salaryMax": 60000, "salaryCurrency": "eur",
                 "salaryPeriod": "monthly", "pubDate": "not a date"},
                {"id": 8, "jobTitle": "Writer", "salaryMin": 0, "salaryPeriod": "weekly"},
                {"jobTitle": "No id"},
                {"id": 9, "jobTitle": ""}
            ]})
            .to_string(),
        )
        .unwrap();
        assert_eq!(jobs.len(), 2, "postings without an id or title are skipped");
        assert_eq!(jobs[0].external_id, "7");
        assert_eq!(
            (jobs[0].title.as_str(), jobs[0].company.as_str()),
            ("Dev & Ops", "A&B")
        );
        let s = jobs[0].salary.as_ref().unwrap();
        assert_eq!(
            (s.min, s.max, s.currency.as_deref(), s.period),
            (
                Some(40_000.0),
                Some(60_000.0),
                Some("EUR"),
                Some(SalaryPeriod::Month)
            )
        );
        assert_eq!(jobs[0].posted_at, None);
        assert!(jobs[1].salary.is_none(), "a zero salary is no salary");
        assert!(jobs[1].tags.is_empty());
    }

    #[test]
    fn workable_edge_cases() {
        let jobs = board(
            Ats::Workable,
            serde_json::json!({"name": "Acme", "jobs": [
                {"shortcode": "AAAAAAAAAA", "title": "Remote anywhere", "telecommuting": true,
                 "created_at": "2026-10-01"},
                {"shortcode": "BBBBBBBBBB", "title": "Office", "telecommuting": false,
                 "city": "Berlin", "country": "Germany"},
                {"shortcode": "CCCCCCCCCC", "title": "Repeat", "telecommuting": true, "country": "Spain"},
                {"shortcode": "CCCCCCCCCC", "title": "Repeat", "telecommuting": true, "country": "Spain"},
                {"shortcode": "CCCCCCCCCC", "title": "Repeat", "telecommuting": true, "country": "Italy"}
            ]}),
        );
        let titles: Vec<&str> = jobs.iter().map(|j| j.title.as_str()).collect();
        assert_eq!(titles, ["Remote anywhere", "Repeat"], "office role dropped");
        assert_eq!(jobs[0].location, "Remote");
        assert_eq!(
            jobs[0].posted_at.as_deref(),
            Some("2026-10-01T00:00:00Z"),
            "falls back to created_at"
        );
        assert!(jobs[0].description.is_empty());
        assert_eq!(jobs[1].location, "Remote, Spain; Remote, Italy");
        assert_eq!(
            board_company(Ats::Workable, r#"{"name": "", "jobs": []}"#),
            None
        );
        assert!(parse_board(Ats::Workable, r#"{"name": "Acme"}"#).is_err());
    }

    #[test]
    fn recruitee_edge_cases() {
        let jobs = board(
            Ats::Recruitee,
            serde_json::json!({"offers": [
                {"id": 1, "title": "Remote, no countries", "remote": true, "location": "Remote job",
                 "locations": [], "salary": {"min": null, "max": null, "period": null, "currency": null},
                 "published_at": "2026-10-15 09:31:36"},
                {"id": 2, "title": "Says remote in text", "remote": false,
                 "location": "Remote - Netherlands", "salary": {"min": 50000, "max": 70000, "period": "year", "currency": "EUR"}},
                {"id": 3, "title": "Office", "remote": false, "location": "Utrecht, Netherlands"},
                {"id": 4, "title": "Repeat countries", "remote": true, "location": "Remote job",
                 "locations": [{"country": "Spain"}, {"country": "Spain"}, {"country": ""}]}
            ]}),
        );
        assert_eq!(jobs.len(), 3);
        assert_eq!(jobs[0].location, "Remote job");
        assert!(jobs[0].salary.is_none());
        assert_eq!(jobs[0].posted_at.as_deref(), Some("2026-10-15T09:31:36Z"));
        assert_eq!(jobs[1].location, "Remote - Netherlands");
        assert_eq!(jobs[1].salary.as_ref().unwrap().min, Some(50_000.0));
        assert_eq!(jobs[2].location, "Remote - Spain");
        assert!(jobs[0].tags.is_empty());
        assert_eq!(board_company(Ats::Recruitee, r#"{"offers": []}"#), None);
    }

    #[test]
    fn smartrecruiters_edge_cases() {
        let jobs = board(
            Ats::SmartRecruiters,
            serde_json::json!({"content": [
                {"id": "1", "name": "Gaps in place", "company": {"identifier": "Acme", "name": "Acme"},
                 "location": {"remote": true, "fullLocation": "Changsha, , China"}},
                {"id": "2", "name": "Only remote", "company": {"identifier": "Acme"},
                 "location": {"remote": true, "fullLocation": "Remote"}},
                {"id": "3", "name": "Not flagged", "location": {"fullLocation": "Remote, US"}},
                {"id": "4", "name": "No location"}
            ]}),
        );
        assert_eq!(jobs.len(), 2, "only postings flagged remote");
        assert_eq!(jobs[0].location, "Remote, Changsha, China");
        assert_eq!(jobs[0].url, "https://jobs.smartrecruiters.com/Acme/1");
        assert_eq!(jobs[1].location, "Remote");
        assert_eq!(jobs[1].company, "", "filled from the watchlist entry later");

        // Empty sections are left out; headings stay with their text.
        let posting = serde_json::json!({
            "id": "5", "name": "Engineer", "location": {"remote": true},
            "postingUrl": "https://jobs.smartrecruiters.com/Acme/5-engineer",
            "jobAd": {"sections": {
                "companyDescription": {"title": "Company Description", "text": ""},
                "jobDescription": {"title": "Job Description", "text": "<p>Build things.</p>"},
                "qualifications": {"title": "Qualifications"}
            }}
        });
        let j = board_job(Ats::SmartRecruiters, &posting).unwrap();
        assert_eq!(j.description, "Job Description\nBuild things.");
        assert_eq!(j.url, "https://jobs.smartrecruiters.com/Acme/5-engineer");
        assert!(board_job(Ats::SmartRecruiters, &serde_json::json!({"id": "6"})).is_none());
    }

    #[test]
    fn link_edge_cases() {
        for not_a_board in [
            "https://www.workable.com/",
            "https://apply.workable.com/",
            "https://apply.workable.com/j/7B77708105",
            "https://www.recruitee.com/",
            "https://api.recruitee.com/c/1/offers",
            "https://jobs.smartrecruiters.com/",
            "https://careers.bunq.com/o/head-of-product",
            "workable",
        ] {
            assert_eq!(board_from_url(not_a_board), None, "{not_a_board}");
        }
        assert_eq!(
            board_from_url("HTTPS://APPLY.WORKABLE.COM/RemoFirst?utm_source=x"),
            Some((Ats::Workable, "RemoFirst".into()))
        );
        assert_eq!(
            board_from_url("hostaway.recruitee.com"),
            Some((Ats::Recruitee, "hostaway".into()))
        );
        assert_eq!(
            board_from_url("https://api.smartrecruiters.com/v1/companies/BoschGroup/postings"),
            Some((Ats::SmartRecruiters, "BoschGroup".into()))
        );
        // A posting link names the board too.
        assert_eq!(
            board_from_url("https://apply.workable.com/remofirst/j/7B77708105/"),
            Some((Ats::Workable, "remofirst".into()))
        );

        // Workable's short links don't say which company, so they can't be fetched.
        assert_eq!(
            posting_from_url("https://apply.workable.com/j/7B77708105"),
            None
        );
        assert_eq!(
            posting_from_url("https://hostaway.recruitee.com/o/senior-pm/c/new"),
            Some((Ats::Recruitee, "hostaway".into(), "senior-pm".into()))
        );
        assert_eq!(
            posting_from_url("https://jobs.smartrecruiters.com/ServiceNow/744000157570619"),
            Some((
                Ats::SmartRecruiters,
                "ServiceNow".into(),
                "744000157570619".into()
            ))
        );
        assert_eq!(
            posting_from_url("https://jobs.smartrecruiters.com/ServiceNow"),
            None
        );
    }

    #[test]
    fn adapter_config_edge_cases() {
        for config in [
            "",
            "not json",
            "{}",
            r#"{"ats":"workday","token":"acme"}"#,
            r#"{"ats":"workable"}"#,
            r#"{"ats":"workable","token":7}"#,
        ] {
            assert_eq!(Adapter::for_source("x:acme", config), None, "{config}");
        }
        for ats in Ats::ALL {
            assert_eq!(Ats::parse(ats.as_str()), Some(ats));
        }
    }

    /// Hits the real sites. `cargo test live_sources -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_sources() {
        tauri::async_runtime::block_on(async {
            let c = client();
            for a in [
                Adapter::Remotive,
                Adapter::RemoteOk,
                Adapter::Wwr,
                Adapter::Jobicy,
            ] {
                match a.fetch(&c).await {
                    Ok(jobs) => println!(
                        "{a:?}: {} jobs, first: {:?}",
                        jobs.len(),
                        jobs.first().map(|j| (&j.company, &j.title, &j.location))
                    ),
                    Err(e) => println!("{a:?}: ERROR {e}"),
                }
            }
            for name in [
                "gitlab",
                "https://jobs.lever.co/spotify",
                "https://jobs.ashbyhq.com/ashby",
                "https://apply.workable.com/remofirst",
                "https://hostaway.recruitee.com",
                "https://careers.smartrecruiters.com/BoschGroup",
                "Not A Real Company Xyz",
            ] {
                match find_board(&c, name).await {
                    Ok(b) => println!(
                        "{name}: {:?} {} {} → {} remote jobs, {} with descriptions, {} after merging reposts",
                        b.ats,
                        b.token,
                        b.company,
                        b.jobs.len(),
                        b.jobs.iter().filter(|j| !j.description.is_empty()).count(),
                        super::super::dedupe::merge_reposts(b.jobs.clone()).len()
                    ),
                    Err(e) => println!("{name}: ERROR {e}"),
                }
            }
        });
    }
}
