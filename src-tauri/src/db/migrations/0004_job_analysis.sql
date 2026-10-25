-- Phase 3: AI requirement analysis per job, cached until the profile changes.
CREATE TABLE job_analysis (
    job_id       INTEGER PRIMARY KEY REFERENCES jobs (id) ON DELETE CASCADE,
    provider     TEXT NOT NULL,
    model        TEXT NOT NULL,
    -- Hash of the profile the analysis was made against; a different hash means stale.
    profile_hash TEXT NOT NULL,
    data         TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
