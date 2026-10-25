-- Phase 5: application tracker.

-- Drafts frozen when the user marks a job applied ("letter version sent"). The working
-- draft is the newest row that isn't sent.
ALTER TABLE drafts ADD COLUMN sent INTEGER NOT NULL DEFAULT 0;

-- Every saved job is a tracked application.
INSERT INTO applications (job_id, status)
SELECT id, 'saved' FROM jobs
WHERE state = 'saved' AND id NOT IN (SELECT job_id FROM applications);
