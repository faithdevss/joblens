-- Phase 4: suggested resume edits and the user's decisions on them, per job.

-- What the user did with a suggested edit. `data` is the suggestion as it was shown, so
-- it can still be listed (and undone) after the profile changes.
CREATE TABLE resume_edits (
    job_id     INTEGER NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
    edit_id    TEXT NOT NULL,
    data       TEXT NOT NULL,
    decision   TEXT NOT NULL CHECK (decision IN ('accepted', 'rejected')),
    -- The text that went into the profile (the user may have edited the suggestion).
    final_text TEXT NOT NULL DEFAULT '',
    -- ID of the profile item an accepted edit created, e.g. a new skill.
    applied_id TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (job_id, edit_id)
);

-- AI-suggested rewordings, cached per job. Suggestions whose resume line has since
-- changed are hidden when read.
CREATE TABLE edit_suggestions (
    job_id     INTEGER PRIMARY KEY REFERENCES jobs (id) ON DELETE CASCADE,
    provider   TEXT NOT NULL,
    model      TEXT NOT NULL,
    data       TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
