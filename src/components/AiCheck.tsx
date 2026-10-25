import type { JobAnalysis } from "../bindings/JobAnalysis";
import type { Requirement } from "../bindings/Requirement";
import type { RequirementStatus } from "../bindings/RequirementStatus";
import { formatShortDate } from "../lib/format";
import { Icon } from "./Icon";

type AiCheckProps = {
  aiOn: boolean;
  analysis: JobAnalysis | null;
  /** Made before the profile last changed. */
  stale: boolean;
  running: boolean;
  onRun: (force: boolean) => void;
  onCancel: () => void;
};

const STATUS: Record<RequirementStatus, { icon: string; label: string }> = {
  matched: { icon: "check_circle", label: "On your resume" },
  worded_differently: { icon: "swap_horiz", label: "Maybe worded differently" },
  missing: { icon: "cancel", label: "Missing" },
};

function lineRef(lines: number[]): string {
  return lines.length ? ` · resume line ${lines.map((l) => l + 1).join(", ")}` : " · added by you";
}

function RequirementRow({ r }: { r: Requirement }) {
  const s = STATUS[r.status];
  return (
    <li className="req" data-status={r.status}>
      <Icon name={s.icon} />
      <div className="req-body">
        <span className="req-text">
          {r.text}
          {!r.required && <span className="muted"> · nice to have</span>}
        </span>
        <span className="sr-only">{s.label}.</span>
        {r.evidence.map((e) => (
          <span key={e.id} className="req-evidence">
            “{e.text}”<span className="muted">{lineRef(e.lines)}</span>
          </span>
        ))}
        {r.note && <span className="req-note">{r.note}</span>}
      </div>
    </li>
  );
}

/** "What they ask for": the AI requirement check, with evidence from the user's profile. */
export function AiCheck({ aiOn, analysis, stale, running, onRun, onCancel }: AiCheckProps) {
  if (!aiOn) {
    return (
      <section className="job-section">
        <h3>What they ask for</h3>
        <p className="muted">
          Turn on AI in Settings → Privacy &amp; AI to check each requirement against your resume.
        </p>
      </section>
    );
  }
  if (running) {
    return (
      <section className="job-section" aria-busy="true">
        <h3>What they ask for</h3>
        <div className="ai-running">
          <Icon name="progress_activity" className="spin" />
          <span>Checking this job against your resume… usually 10–20 seconds.</span>
          <button type="button" className="btn btn-ghost" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </section>
    );
  }
  if (!analysis) {
    return (
      <section className="job-section">
        <h3>What they ask for</h3>
        <p className="muted">
          Check each requirement against your resume, with the lines that back it up.
        </p>
        <button type="button" className="btn btn-secondary" onClick={() => onRun(false)}>
          <Icon name="auto_awesome" />
          Check with AI
        </button>
      </section>
    );
  }

  const missing = analysis.requirements.filter((r) => r.status === "missing");
  const worded = analysis.requirements.filter((r) => r.status === "worded_differently");
  return (
    <>
      <section className="job-section" aria-label="What they ask for">
        <div className="section-title-row">
          <h3>What they ask for</h3>
          <button type="button" className="btn btn-ghost" onClick={() => onRun(true)}>
            <Icon name="refresh" />
            Check again
          </button>
        </div>
        {stale && (
          <div className="flag" role="note">
            <Icon name="history" />
            <span>
              Your profile changed since this check, so it isn’t in the score. Check again to update
              it.
            </span>
          </div>
        )}
        {analysis.summary && <p className="ai-summary">{analysis.summary}</p>}
        {analysis.requirements.length === 0 ? (
          <p className="muted">No concrete requirements found in this posting.</p>
        ) : (
          <ul className="req-list">
            {analysis.requirements.map((r) => (
              <RequirementRow key={r.text} r={r} />
            ))}
          </ul>
        )}
        <p className="field-hint">
          Checked {formatShortDate(analysis.createdAt)} with {analysis.model}. Evidence is quoted
          from your profile.
        </p>
      </section>

      <section className="job-section" aria-label="Gaps">
        <h3>Gaps</h3>
        <div className="gaps">
          <div>
            <h4>Missing</h4>
            {missing.length ? (
              <ul>
                {missing.map((r) => (
                  <li key={r.text}>{r.text}</li>
                ))}
              </ul>
            ) : (
              <p className="muted">Nothing missing.</p>
            )}
          </div>
          <div>
            <h4>Maybe just worded differently</h4>
            {worded.length ? (
              <ul>
                {worded.map((r) => (
                  <li key={r.text}>
                    {r.text}
                    {r.note && <span className="muted"> — {r.note}</span>}
                  </li>
                ))}
              </ul>
            ) : (
              <p className="muted">Nothing to reword.</p>
            )}
          </div>
        </div>
      </section>
    </>
  );
}
