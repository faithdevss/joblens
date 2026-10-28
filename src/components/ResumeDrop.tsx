import { type DragEvent, useRef, useState } from "react";
import type { Profile } from "../bindings/Profile";
import type { ResumeMeta } from "../bindings/ResumeMeta";
import { formatBytes, formatShortDate } from "../lib/format";
import { plural, profileCounts } from "../lib/profile";
import { Icon } from "./Icon";

type ResumeDropProps = {
  resume: ResumeMeta | null;
  profile: Profile;
  onImport: (file: File) => Promise<unknown>;
};

const ACCEPT =
  ".pdf,.docx,application/pdf,application/vnd.openxmlformats-officedocument.wordprocessingml.document";

/** Upload area: drop zone → reading → file card with what was found. */
export function ResumeDrop({ resume, profile, onImport }: ResumeDropProps) {
  const input = useRef<HTMLInputElement>(null);
  const [reading, setReading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const [confirmReplace, setConfirmReplace] = useState(false);

  const read = async (file: File | undefined) => {
    if (!file) return;
    setError(null);
    setConfirmReplace(false);
    setReading(file.name);
    try {
      await onImport(file);
    } catch (err) {
      setError(String(err));
    } finally {
      setReading(null);
      if (input.current) input.current.value = "";
    }
  };

  const dropProps = {
    onDragOver: (e: DragEvent) => {
      e.preventDefault();
      setDragOver(true);
    },
    onDragLeave: () => setDragOver(false),
    onDrop: (e: DragEvent) => {
      e.preventDefault();
      setDragOver(false);
      read(e.dataTransfer.files[0]);
    },
  };

  const picker = (
    <input
      ref={input}
      type="file"
      accept={ACCEPT}
      hidden
      data-testid="resume-file"
      onChange={(e) => read(e.target.files?.[0])}
    />
  );

  const errorBox = error && (
    <p className="upload-error" role="alert">
      <Icon name="error" />
      {error}
    </p>
  );

  if (reading) {
    return (
      <div className="upload-card" aria-busy="true">
        <Icon name="progress_activity" className="spin" />
        <div className="upload-card-text">
          <strong>{reading}</strong>
          <span>Reading sections, roles and dates on this computer…</span>
        </div>
      </div>
    );
  }

  if (!resume) {
    return (
      <div className="upload">
        <button
          type="button"
          className="drop-zone"
          data-over={dragOver || undefined}
          onClick={() => input.current?.click()}
          {...dropProps}
        >
          <Icon name="upload_file" />
          <span className="drop-zone-title">Drop your resume here</span>
          <span className="drop-zone-sub">
            Fills in contact, experience, skills and the rest below · PDF or Word (.docx) · stays on
            this computer
          </span>
          <span className="btn btn-secondary">Choose file</span>
        </button>
        {picker}
        {errorBox}
      </div>
    );
  }

  const counts = profileCounts(profile);
  const meta = [
    resume.pages ? plural(resume.pages, "page") : null,
    formatBytes(resume.sizeBytes),
    `read ${formatShortDate(resume.importedAt)}`,
  ].filter(Boolean);

  return (
    <div className="upload">
      <div className="upload-card" data-over={dragOver || undefined} {...dropProps}>
        <span className="upload-card-icon">
          <Icon name="description" />
        </span>
        <div className="upload-card-text">
          <strong>{resume.fileName}</strong>
          <span>{meta.join(" · ")}</span>
          <div className="found-chips">
            <span className="tag">{plural(counts.roles, "role")}</span>
            <span className="tag">{plural(counts.achievements, "achievement")}</span>
            <span className="tag">{plural(counts.skills, "skill")}</span>
            {counts.flags > 0 && (
              <span className="tag tag-warn">
                {plural(counts.flags, "thing", "things")} to check
              </span>
            )}
          </div>
        </div>
        {!confirmReplace && (
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => setConfirmReplace(true)}
          >
            <Icon name="swap_horiz" />
            Replace
          </button>
        )}
      </div>
      {confirmReplace && (
        <div className="confirm-row">
          <span>Reading a new file replaces your profile, including edits you made here.</span>
          <button type="button" className="btn btn-ghost" onClick={() => setConfirmReplace(false)}>
            Cancel
          </button>
          <button type="button" className="btn btn-primary" onClick={() => input.current?.click()}>
            Choose new file
          </button>
        </div>
      )}
      {picker}
      {errorBox}
    </div>
  );
}
