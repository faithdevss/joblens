-- Phase 2: what the feed shows per job, and when each source last fetched cleanly.
ALTER TABLE jobs ADD COLUMN tags TEXT NOT NULL DEFAULT '[]';
ALTER TABLE jobs ADD COLUMN salary_text TEXT;
ALTER TABLE sources ADD COLUMN last_ok_at TEXT;

-- Built-in job boards, plus the source jobs added by hand are listed under.
INSERT INTO sources (id, kind, name) VALUES
    ('remotive', 'board', 'Remotive'),
    ('remoteok', 'board', 'Remote OK'),
    ('wwr', 'board', 'We Work Remotely'),
    ('manual', 'manual', 'Added by you');
