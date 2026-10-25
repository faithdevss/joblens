-- Phase 6: interview topics the user marked as prepared, per job.
CREATE TABLE prep_marks (
    job_id     INTEGER NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
    topic      TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (job_id, topic)
);
