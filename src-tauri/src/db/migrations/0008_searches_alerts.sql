-- Phase 6: several named searches; the feed uses the active one (settings key `active_search`).
CREATE TABLE searches (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    data       TEXT NOT NULL,
    alerts     INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
INSERT INTO searches (id, name, data) SELECT 1, 'My search', data FROM search_settings;
DROP TABLE search_settings;

-- Jobs already announced (or already seen after a manual refresh), so each is announced once.
CREATE TABLE job_alerts (
    job_id     INTEGER PRIMARY KEY REFERENCES jobs (id) ON DELETE CASCADE,
    search_id  INTEGER REFERENCES searches (id) ON DELETE SET NULL,
    score      INTEGER NOT NULL,
    notified   INTEGER NOT NULL DEFAULT 0,
    alerted_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
