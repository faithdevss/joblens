//! Source adapters: fetch a public API or feed and turn it into [`RawJob`]s.
//!
//! Parsing is split from fetching so each adapter is tested on a saved response.
//! Errors are plain sentences for the Sources panel; one source failing never
//! affects another.

use std::{sync::LazyLock, time::Duration};

use quick_xml::{Reader, events::Event};
use regex::Regex;
use serde_json::Value;

use super::{RawJob, SalaryPeriod, dates, salary, text};

pub type FetchResult<T> = Result<T, String>;

// Some boards are slow: Lever takes ~20 s to send a large company's postings.
const TIMEOUT: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = concat!(
    "JobLens/",
    env!("CARGO_PKG_VERSION"),
    " (desktop job finder; +https://github.com/faithdevs/joblens)"
);

pub const MANUAL_SOURCE_ID: &str = "manual";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ats {
    Greenhouse,
    Lever,
    Ashby,
}

impl Ats {
    pub const ALL: [Ats; 3] = [Ats::Greenhouse, Ats::Lever, Ats::Ashby];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Greenhouse => "greenhouse",
            Self::Lever => "lever",
            Self::Ashby => "ashby",
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
        }
    }

    fn board_url(self, token: &str) -> String {
        match self {
            Self::Greenhouse => {
                format!("https://boards-api.greenhouse.io/v1/boards/{token}/jobs?content=true")
            }
            Self::Lever => format!("https://api.lever.co/v0/postings/{token}?mode=json"),
            Self::Ashby => format!(
                "https://api.ashbyhq.com/posting-api/job-board/{token}?includeCompensation=true"
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
    Ats { ats: Ats, token: String },
}

impl Adapter {
    /// Built-in boards by id; watchlist companies by their stored config.
    pub fn for_source(id: &str, config: &str) -> Option<Self> {
        match id {
            "remotive" => return Some(Self::Remotive),
            "remoteok" => return Some(Self::RemoteOk),
            "wwr" => return Some(Self::Wwr),
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
            // WWR's feed has a 60-minute TTL.
            Self::RemoteOk | Self::Wwr | Self::Ats { .. } => 3600,
        }
    }

    fn url(&self) -> String {
        match self {
            Self::Remotive => "https://remotive.com/api/remote-jobs".into(),
            Self::RemoteOk => "https://remoteok.com/api".into(),
            Self::Wwr => "https://weworkremotely.com/remote-jobs.rss".into(),
            Self::Ats { ats, token } => ats.board_url(token),
        }
    }

    fn parse(&self, body: &str) -> FetchResult<Vec<RawJob>> {
        match self {
            Self::Remotive => parse_remotive(body),
            Self::RemoteOk => parse_remoteok(body),
            Self::Wwr => parse_wwr(body),
            Self::Ats { ats, .. } => parse_board(*ats, body),
        }
    }

    pub async fn fetch(&self, client: &reqwest::Client) -> FetchResult<Vec<RawJob>> {
        let body = get(client, &self.url()).await?;
        self.parse(&body)
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
            404 => "Not found (404). The board may have moved or closed.".into(),
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

// ---- Company boards (watchlist) ----

/// A company's whole board, keeping remote roles only.
pub fn parse_board(ats: Ats, body: &str) -> FetchResult<Vec<RawJob>> {
    let v = json(body)?;
    let items = match ats {
        Ats::Greenhouse | Ats::Ashby => v.get("jobs").and_then(Value::as_array),
        Ats::Lever => v.as_array(),
    }
    .ok_or_else(unreadable)?;
    Ok(items
        .iter()
        .filter(|j| is_remote(ats, j))
        .filter_map(|j| board_job(ats, j))
        .collect())
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

/// A board link the user pasted, if it's one we recognize.
pub fn board_from_url(input: &str) -> Option<(Ats, String)> {
    let input = input.trim();
    [
        (Ats::Greenhouse, &*GREENHOUSE_URL),
        (Ats::Lever, &*LEVER_URL),
        (Ats::Ashby, &*ASHBY_URL),
    ]
    .into_iter()
    .find_map(|(ats, re)| {
        let token = re.captures(input)?.get(1)?.as_str();
        // "embed" alone is the path prefix, not a board.
        (token != "embed").then(|| (ats, token.to_string()))
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
            return Err(
                "That link isn’t a Greenhouse, Lever or Ashby board. Try the company name instead."
                    .into(),
            );
        }
        None => token_guesses(input)
            .into_iter()
            .flat_map(|t| Ats::ALL.into_iter().map(move |a| (a, t.clone())))
            .collect(),
    };
    let mut last_error = None;
    for (ats, token) in candidates {
        match get(client, &ats.board_url(&token)).await {
            Ok(body) => {
                let jobs = parse_board(ats, &body)?;
                let company = board_company(ats, &body).unwrap_or_else(|| {
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
        format!("Couldn’t find a Greenhouse, Lever or Ashby board for “{input}”. Paste its careers page link instead.")
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

/// Company name as the board states it (only Greenhouse does).
fn board_company(ats: Ats, body: &str) -> Option<String> {
    if ats != Ats::Greenhouse {
        return None;
    }
    let v: Value = serde_json::from_str(body).ok()?;
    let name = str_at(v.get("jobs")?.as_array()?.first()?, "company_name");
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

/// A single posting link we can fetch: (ATS, board, posting id).
pub fn posting_from_url(url: &str) -> Option<(Ats, String, String)> {
    [
        (Ats::Greenhouse, &*GREENHOUSE_JOB),
        (Ats::Lever, &*LEVER_JOB),
        (Ats::Ashby, &*ASHBY_JOB),
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
        return Err("We can only read Greenhouse, Lever and Ashby job links. Paste the description instead.".into());
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
        // Ashby has no single-posting endpoint; find it on the board.
        Ats::Ashby => {
            let body = get(client, &ats.board_url(&token)).await?;
            json(&body)?
                .get("jobs")
                .and_then(Value::as_array)
                .and_then(|jobs| {
                    jobs.iter()
                        .find(|j| str_at(j, "id").eq_ignore_ascii_case(&id))
                })
                .and_then(|j| board_job(ats, j))
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
        assert_eq!(Adapter::for_source("manual", "{}"), None);
    }

    /// Hits the real sites. `cargo test live_sources -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_sources() {
        tauri::async_runtime::block_on(async {
            let c = client();
            for a in [Adapter::Remotive, Adapter::RemoteOk, Adapter::Wwr] {
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
                "Not A Real Company Xyz",
            ] {
                match find_board(&c, name).await {
                    Ok(b) => println!(
                        "{name}: {:?} {} {} → {} remote jobs",
                        b.ats,
                        b.token,
                        b.company,
                        b.jobs.len()
                    ),
                    Err(e) => println!("{name}: ERROR {e}"),
                }
            }
        });
    }
}
