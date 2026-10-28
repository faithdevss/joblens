import { openUrl } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import type { JobDetail } from "../bindings/JobDetail";
import type { JobState } from "../bindings/JobState";
import type { JobSummary } from "../bindings/JobSummary";
import type { OpenCheck } from "../bindings/OpenCheck";
import type { Weights } from "../bindings/Weights";
import { descriptionBlocks, formatAge, jobDate, salaryText } from "../lib/feed";
import { ipc } from "../lib/ipc";
import type { ChangeProfile } from "../lib/useAppData";
import { AiCheck } from "./AiCheck";
import { EligibilityBadge } from "./EligibilityBadge";
import { GapCheckPanel } from "./GapCheckPanel";
import { Icon } from "./Icon";
import { PrepPanel } from "./PrepPanel";
import { WriterPanel } from "./WriterPanel";

type JobDetailPaneProps = {
  job: JobSummary;
  detail: JobDetail | null;
  resumeLines: string[];
  onSetState: (job: JobSummary, state: JobState) => void;
  onMarkApplied: (job: JobSummary) => void;
  weights: Weights;
  onWeights: (weights: Weights) => void;
  aiOn: boolean;
  analyzing: boolean;
  onAnalyze: (force: boolean) => void;
  onCancelAnalysis: () => void;
  changeProfile: ChangeProfile;
  /** An accepted resume edit changed the profile. */
  onProfileChanged: () => void;
};

type Tab = "match" | "gaps" | "write" | "prep";

const TABS: [Tab, string][] = [
  ["match", "Why it matches"],
  ["gaps", "Resume gap check"],
  ["write", "Write application"],
  ["prep", "Interview topics"],
];

/** Tabs that open once the job is saved. */
const NEEDS_SAVE: Tab[] = ["write", "prep"];

const FACTORS = [
  ["skills", "Skills"],
  ["experience", "Experience"],
  ["role", "Role"],
  ["seniority", "Seniority"],
] as const;

export function JobDetailPane({
  job,
  detail,
  resumeLines,
  onSetState,
  onMarkApplied,
  weights,
  onWeights,
  aiOn,
  analyzing,
  onAnalyze,
  onCancelAnalysis,
  changeProfile,
  onProfileChanged,
}: JobDetailPaneProps) {
  const [showWeights, setShowWeights] = useState(false);
  const [tab, setTab] = useState<Tab>("match");
  const [openCheck, setOpenCheck] = useState<{ jobId: number; check: OpenCheck | null } | null>(
    null,
  );
  const stillOpen = openCheck?.jobId === job.id ? openCheck : null;
  const checkOpen = async () => {
    setOpenCheck({ jobId: job.id, check: null });
    try {
      setOpenCheck({ jobId: job.id, check: await ipc.checkStillOpen(job.id) });
    } catch (err) {
      setOpenCheck({
        jobId: job.id,
        check: { status: "unknown", note: String(err), checkedAt: "" },
      });
    }
  };
  const [first, ...also] = job.listings;
  const original = job.url ?? first?.url ?? null;
  const m = detail?.match;

  return (
    <article className="job-detail" aria-label={`${job.title} at ${job.company}`}>
      <header className="job-detail-head">
        <div className="job-detail-title">
          <div className="job-detail-meta">
            {first ? `via ${first.sourceName}` : "Added by you"} · posted {formatAge(jobDate(job))}
            {also.length > 0 && ` · also listed on ${also.map((l) => l.sourceName).join(", ")}`}
          </div>
          <h2>{job.title}</h2>
          <div className="job-detail-sub">
            {job.company} · {salaryText(job.salary)}
            {job.location && ` · ${job.location}`}
          </div>
        </div>
        <div className="score-big" title="Match score (rules only)">
          <span>{job.score}</span>
          <span className="score-big-label">match</span>
        </div>
      </header>

      <div className="eligibility-row">
        <EligibilityBadge status={job.eligibility.status} />
        <span>{job.eligibility.note}</span>
      </div>

      <div className="job-actions">
        <button type="button" className="btn btn-primary" onClick={() => onMarkApplied(job)}>
          <Icon name="send" />
          Mark applied
        </button>
        <button
          type="button"
          className="btn btn-secondary"
          aria-pressed={job.state === "saved"}
          onClick={() => onSetState(job, job.state === "saved" ? "new" : "saved")}
        >
          <Icon name="star" className={job.state === "saved" ? "icon-filled" : undefined} />
          {job.state === "saved" ? "Saved" : "Save"}
        </button>
        {original && (
          <button type="button" className="btn btn-secondary" onClick={() => openUrl(original)}>
            <Icon name="open_in_new" />
            Open original
          </button>
        )}
        {original && (
          <button
            type="button"
            className="btn btn-ghost"
            disabled={stillOpen !== null && stillOpen.check === null}
            onClick={checkOpen}
          >
            <Icon name="travel_explore" />
            Still open?
          </button>
        )}
        <span className="spacer" />
        {job.state === "dismissed" ? (
          <button type="button" className="btn btn-ghost" onClick={() => onSetState(job, "new")}>
            <Icon name="undo" />
            Restore
          </button>
        ) : (
          <button
            type="button"
            className="btn btn-ghost"
            onClick={() => onSetState(job, "dismissed")}
          >
            <Icon name="close" />
            Dismiss
          </button>
        )}
      </div>

      {stillOpen && (
        <p className="open-check" role="status" data-status={stillOpen.check?.status ?? "checking"}>
          {stillOpen.check === null
            ? "Checking the posting…"
            : `${
                stillOpen.check.status === "open"
                  ? "Looks open."
                  : stillOpen.check.status === "closed"
                    ? "Looks closed."
                    : "Can’t tell."
              } ${stillOpen.check.note}`}
        </p>
      )}

      <div
        className="tabs"
        role="tablist"
        aria-label="Job details"
        onKeyDown={(e) => {
          // Arrow keys move between tabs (ARIA tabs pattern).
          const i = TABS.findIndex(([id]) => id === tab);
          const next =
            e.key === "ArrowRight"
              ? (i + 1) % TABS.length
              : e.key === "ArrowLeft"
                ? (i + TABS.length - 1) % TABS.length
                : e.key === "Home"
                  ? 0
                  : e.key === "End"
                    ? TABS.length - 1
                    : null;
          if (next === null) return;
          e.preventDefault();
          setTab(TABS[next][0]);
          document.getElementById(`tab-${TABS[next][0]}`)?.focus();
        }}
      >
        {TABS.map(([id, label]) => (
          <button
            key={id}
            type="button"
            role="tab"
            id={`tab-${id}`}
            aria-selected={tab === id}
            aria-controls="job-tab-panel"
            tabIndex={tab === id ? 0 : -1}
            onClick={() => setTab(id)}
          >
            {label}
            {NEEDS_SAVE.includes(id) && job.state !== "saved" && <Icon name="lock" />}
          </button>
        ))}
      </div>

      <div id="job-tab-panel" role="tabpanel" aria-labelledby={`tab-${tab}`} className="tab-panel">
        {tab === "gaps" && (
          <GapCheckPanel
            jobId={job.id}
            aiOn={aiOn}
            changeProfile={changeProfile}
            onProfileChanged={onProfileChanged}
          />
        )}
        {NEEDS_SAVE.includes(tab) &&
          (job.state !== "saved" ? (
            <div className="locked">
              <Icon name="lock" />
              <p>
                {tab === "write"
                  ? "Save this job to write an application for it."
                  : "Save this job to prepare for its interview."}
              </p>
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => onSetState(job, "saved")}
              >
                <Icon name="star" />
                Save job
              </button>
            </div>
          ) : tab === "write" ? (
            <WriterPanel jobId={job.id} aiOn={aiOn} />
          ) : (
            <PrepPanel jobId={job.id} />
          ))}
        {tab === "match" &&
          (!detail || !m ? (
            <p className="muted">Loading…</p>
          ) : (
            <>
              {m.shortDescription && (
                <div className="flag" role="note">
                  <Icon name="warning" />
                  <span>
                    This description is only ~{m.wordCount} words. The score is a rough guess —
                    check the original posting.
                  </span>
                </div>
              )}

              <section className="job-section" aria-label="Match breakdown">
                <div className="factors">
                  {FACTORS.map(([key, label]) => (
                    <div key={key} className="factor">
                      <div className="factor-head">
                        <span>{label}</span>
                        <span className="mono">{m[key]}</span>
                      </div>
                      <div className="factor-bar" aria-hidden="true">
                        <span style={{ width: `${m[key]}%` }} />
                      </div>
                    </div>
                  ))}
                </div>
                <ul className="match-facts">
                  <li>
                    {m.yearsRequired === null
                      ? "Years of experience: not stated."
                      : `Asks for ${m.yearsRequired}+ years; your resume shows ${
                          m.yearsOnProfile === null
                            ? "no readable dates"
                            : `about ${m.yearsOnProfile}`
                        }.`}
                  </li>
                  <li>Seniority: {m.jobSeniority ?? "not stated"}.</li>
                </ul>
                <button
                  type="button"
                  className="btn btn-ghost"
                  aria-expanded={showWeights}
                  onClick={() => setShowWeights((v) => !v)}
                >
                  <Icon name="tune" />
                  {showWeights ? "Hide weights" : "Adjust weights"}
                </button>
                {showWeights && (
                  <div className="weights">
                    {FACTORS.map(([key, label]) => (
                      <label key={key} className="weight">
                        <span>{label}</span>
                        <input
                          type="range"
                          min={0}
                          max={50}
                          value={weights[key]}
                          onChange={(e) => onWeights({ ...weights, [key]: Number(e.target.value) })}
                        />
                        <span className="mono">×{weights[key]}</span>
                      </label>
                    ))}
                    <span className="field-hint">
                      How much each factor counts toward every job’s score.
                    </span>
                  </div>
                )}
              </section>

              <AiCheck
                aiOn={aiOn}
                analysis={detail.analysis}
                stale={detail.analysisStale}
                running={analyzing}
                onRun={onAnalyze}
                onCancel={onCancelAnalysis}
              />

              {!detail.analysis && (
                <section className="job-section">
                  <h3>Skills they mention</h3>
                  {m.matchedSkills.length === 0 && m.missingSkills.length === 0 ? (
                    <p className="muted">No skills we recognize in this posting.</p>
                  ) : (
                    <div className="skill-chips">
                      {m.matchedSkills.map((s) => (
                        <span
                          key={s.name}
                          className="tag tag-ok"
                          title={
                            s.lines.length
                              ? `From your resume: “${resumeLines[s.lines[0]] ?? ""}”`
                              : "On your profile"
                          }
                        >
                          <Icon name="check" />
                          {s.name}
                        </span>
                      ))}
                      {m.missingSkills.map((s) => (
                        <span key={s} className="tag" title="Not on your profile">
                          {s}
                        </span>
                      ))}
                    </div>
                  )}
                </section>
              )}

              <section className="job-section">
                <h3>Description</h3>
                <JobDescription text={detail.description} />
              </section>

              {job.listings.length > 0 && (
                <section className="job-section">
                  <h3>Listed on</h3>
                  <ul className="listing-links">
                    {job.listings.map((l) => (
                      <li key={l.sourceId}>
                        {l.url ? (
                          <button
                            type="button"
                            className="btn-link"
                            onClick={() => openUrl(l.url ?? "")}
                          >
                            {l.sourceName}
                          </button>
                        ) : (
                          l.sourceName
                        )}
                      </li>
                    ))}
                  </ul>
                </section>
              )}
            </>
          ))}
      </div>
    </article>
  );
}

function JobDescription({ text }: { text: string }) {
  const blocks = descriptionBlocks(text);
  if (blocks.length === 0) return <p className="job-description muted">No description.</p>;
  return (
    <div className="job-description">
      {blocks.map((b, i) => {
        // Block order is the identity; the text never reorders.
        const key = i;
        if (b.kind === "heading") return <h4 key={key}>{b.text}</h4>;
        if (b.kind === "paragraph") return <p key={key}>{b.text}</p>;
        return (
          <ul key={key}>
            {b.items.map((item, j) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: item order is the identity
              <li key={j}>{item}</li>
            ))}
          </ul>
        );
      })}
    </div>
  );
}
