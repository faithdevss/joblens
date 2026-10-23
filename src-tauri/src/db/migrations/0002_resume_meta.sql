-- Details of the uploaded resume file, shown on the Resume view.
ALTER TABLE profile ADD COLUMN resume_size_bytes INTEGER;
ALTER TABLE profile ADD COLUMN resume_pages INTEGER;
