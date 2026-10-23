-- Key/value app settings (AI provider, UI prefs). Values are JSON.
CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Single-row resume profile. `data` is the reviewed profile JSON;
-- `resume_text` is the extracted plain text later phases cite lines from.
CREATE TABLE profile (
    id               INTEGER PRIMARY KEY CHECK (id = 1),
    data             TEXT NOT NULL,
    resume_file_name TEXT,
    resume_text      TEXT,
    parsed_at        TEXT,
    updated_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Single-row search settings (titles, keywords, country, timezone, salary, seniority) as JSON.
CREATE TABLE search_settings (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    data       TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE sources (
    id          TEXT PRIMARY KEY,
    kind        TEXT NOT NULL CHECK (kind IN ('board', 'watchlist', 'manual')),
    name        TEXT NOT NULL,
    config      TEXT NOT NULL DEFAULT '{}',
    enabled     INTEGER NOT NULL DEFAULT 1,
    last_run_at TEXT,
    last_error  TEXT,
    job_count   INTEGER NOT NULL DEFAULT 0
);

-- One row per real-world job after dedupe; per-source listings live in job_sources.
CREATE TABLE jobs (
    id              INTEGER PRIMARY KEY,
    canonical_key   TEXT NOT NULL UNIQUE,
    title           TEXT NOT NULL,
    company         TEXT NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    url             TEXT,
    location_text   TEXT,
    salary_min      REAL,
    salary_max      REAL,
    salary_currency TEXT,
    salary_period   TEXT CHECK (salary_period IN ('year', 'month', 'hour')),
    eligibility     TEXT NOT NULL DEFAULT 'not_stated'
                    CHECK (eligibility IN ('eligible', 'restricted', 'not_stated')),
    eligibility_note TEXT,
    posted_at       TEXT,
    first_seen_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    state           TEXT NOT NULL DEFAULT 'new' CHECK (state IN ('new', 'saved', 'dismissed'))
);
CREATE INDEX jobs_state_idx ON jobs (state);
CREATE INDEX jobs_posted_idx ON jobs (posted_at);

CREATE TABLE job_sources (
    job_id      INTEGER NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
    source_id   TEXT NOT NULL REFERENCES sources (id) ON DELETE CASCADE,
    external_id TEXT NOT NULL,
    url         TEXT,
    seen_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (source_id, external_id)
);
CREATE INDEX job_sources_job_idx ON job_sources (job_id);

CREATE TABLE drafts (
    id         INTEGER PRIMARY KEY,
    job_id     INTEGER NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
    kind       TEXT NOT NULL CHECK (kind IN ('cover_letter', 'email')),
    content    TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX drafts_job_idx ON drafts (job_id, kind);

CREATE TABLE applications (
    id             INTEGER PRIMARY KEY,
    job_id         INTEGER NOT NULL UNIQUE REFERENCES jobs (id) ON DELETE CASCADE,
    status         TEXT NOT NULL
                   CHECK (status IN ('saved', 'applied', 'interviewing', 'offer', 'rejected', 'closed')),
    applied_at     TEXT,
    resume_version TEXT,
    letter_draft_id INTEGER REFERENCES drafts (id) ON DELETE SET NULL,
    link           TEXT,
    notes          TEXT NOT NULL DEFAULT '',
    follow_up_at   TEXT,
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX applications_follow_up_idx ON applications (follow_up_at);
