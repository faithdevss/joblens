import { useEffect, useState } from "react";
import type { Coverage } from "../bindings/Coverage";
import type { EditAction } from "../bindings/EditAction";
import type { GapCheck } from "../bindings/GapCheck";
import type { SuggestedEdit } from "../bindings/SuggestedEdit";
import { ipc } from "../lib/ipc";
import type { ChangeProfile } from "../lib/useAppData";
import { Icon } from "./Icon";
import { useToast } from "./Toast";

type GapCheckPanelProps = {
  jobId: number;
  aiOn: boolean;
  changeProfile: ChangeProfile;
  /** The profile changed, so scores elsewhere may have too. */
  onProfileChanged: () => void;
};

const COVERAGE: Record<Coverage, { mark: string; label: string }> = {
  covered: { mark: "✓", label: "Covered" },
  partly: { mark: "~", label: "Worded differently" },
  missing: { mark: "", label: "Not covered" },
};

function EditCard({
  edit,
  busy,
  onDecide,
}: {
  edit: SuggestedEdit;
  busy: boolean;
  onDecide: (action: EditAction) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(edit.after);
  const decided = edit.decision !== null;
  return (
    <li className="edit-card" data-decision={edit.decision ?? "pending"}>
      <div className="edit-why">
        {edit.why}
        <span className="muted"> · {edit.place}</span>
        {edit.source === "ai" && <span className="muted"> · suggested by AI</span>}
      </div>
      {edit.before && <div className="edit-before">{edit.before}</div>}
      {editing ? (
        <textarea
          className="input"
          aria-label="Edit suggestion"
          rows={2}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
      ) : (
        <div className="edit-after">{decided && edit.finalText ? edit.finalText : text}</div>
      )}
      {edit.warning && !decided && (
        <div className="edit-warning">
          <Icon name="warning" />
          {edit.warning}
        </div>
      )}
      <div className="edit-actions">
        {decided ? (
          <>
            <strong>
              {edit.decision === "accepted" ? "Accepted — added to your profile" : "Rejected"}
            </strong>
            <button
              type="button"
              className="btn btn-ghost"
              disabled={busy}
              onClick={() => onDecide({ type: "undo" })}
            >
              <Icon name="undo" />
              Undo
            </button>
          </>
        ) : (
          <>
            <button
              type="button"
              className="btn btn-secondary"
              disabled={busy || !text.trim()}
              onClick={() => {
                setEditing(false);
                onDecide({ type: "accept", text });
              }}
            >
              <Icon name="check" />
              Accept
            </button>
            <button
              type="button"
              className="btn btn-ghost"
              disabled={busy}
              onClick={() => setEditing((v) => !v)}
            >
              {editing ? "Done" : "Edit"}
            </button>
            <button
              type="button"
              className="btn btn-ghost"
              disabled={busy}
              onClick={() => onDecide({ type: "reject" })}
            >
              Reject
            </button>
          </>
        )}
      </div>
    </li>
  );
}

/** "Resume gap check": requirements covered, suggested edits, basic checks, plain-text view. */
export function GapCheckPanel({
  jobId,
  aiOn,
  changeProfile,
  onProfileChanged,
}: GapCheckPanelProps) {
  const toast = useToast();
  const [gap, setGap] = useState<GapCheck | null>(null);
  const [busy, setBusy] = useState(false);
  const [suggesting, setSuggesting] = useState(false);

  useEffect(() => {
    let live = true;
    setGap(null);
    ipc
      .getGapCheck(jobId)
      .then((g) => live && setGap(g))
      .catch((err) => toast(`Couldn't run the gap check: ${err}`, "error"));
    return () => {
      live = false;
    };
  }, [jobId, toast]);

  if (!gap) return <p className="muted">Checking your resume…</p>;

  const decide = async (edit: SuggestedEdit, action: EditAction) => {
    // Accepting, and undoing an accepted edit, change the profile.
    const touchesProfile =
      action.type === "accept" || (action.type === "undo" && edit.decision === "accepted");
    setBusy(true);
    try {
      const call = () => ipc.decideEdit(jobId, edit.id, action);
      setGap(touchesProfile ? await changeProfile(call) : await call());
      if (touchesProfile) onProfileChanged();
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setBusy(false);
    }
  };

  const suggest = async () => {
    setSuggesting(true);
    try {
      setGap(await ipc.suggestEdits(jobId));
    } catch (err) {
      const msg = String(err);
      toast(
        msg.startsWith("Cancelled") ? "Cancelled." : msg,
        msg.startsWith("Cancelled") ? "info" : "error",
      );
    } finally {
      setSuggesting(false);
    }
  };

  const covered = gap.requirements.filter((r) => r.coverage === "covered").length;
  const accepted = gap.edits.some((e) => e.decision === "accepted");

  return (
    <div className="gap-check">
      <section className="job-section" aria-label="Requirements">
        <div className="section-title-row">
          <h3>Requirements</h3>
          {gap.requirements.length > 0 && (
            <span className="field-hint">
              <strong>
                {covered} of {gap.requirements.length}
              </strong>{" "}
              covered by your resume
            </span>
          )}
        </div>
        {gap.aiStale && (
          <p className="field-hint">
            Your profile changed since the AI check. Run it again on “Why it matches” to update this
            list.
          </p>
        )}
        {gap.requirements.length === 0 ? (
          <p className="muted">No requirements we can recognise in this posting.</p>
        ) : (
          <ul className="gap-reqs">
            {gap.requirements.map((r) => (
              <li key={r.text} data-coverage={r.coverage}>
                <span className="gap-mark" aria-hidden="true">
                  {COVERAGE[r.coverage].mark}
                </span>
                <span className="gap-req-text">
                  {r.text}
                  {r.evidence[0] && <span className="req-evidence">“{r.evidence[0].text}”</span>}
                  {r.note && <span className="req-note">{r.note}</span>}
                </span>
                <span className="gap-status">{COVERAGE[r.coverage].label}</span>
              </li>
            ))}
          </ul>
        )}
        {!gap.fromAi && (
          <p className="field-hint">
            {aiOn
              ? "Skills we recognise in the posting. Use “Check with AI” on “Why it matches” for the full list."
              : "Skills we recognise in the posting. Turn on AI in Settings for the full list."}
          </p>
        )}
      </section>

      <section className="job-section" aria-label="Suggested edits">
        <div className="section-title-row">
          <h3>
            Suggested edits <span className="h3-aside">· only from your real experience</span>
          </h3>
          {aiOn && !suggesting && (
            <button type="button" className="btn btn-ghost" onClick={suggest}>
              <Icon name="auto_awesome" />
              {gap.aiSuggested ? "Suggest again" : "Suggest rewording with AI"}
            </button>
          )}
        </div>
        {suggesting && (
          <div className="ai-running" aria-busy="true">
            <Icon name="progress_activity" className="spin" />
            <span>Looking for lines to reword… usually 10–20 seconds.</span>
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => ipc.cancelWriting(jobId)}
            >
              Cancel
            </button>
          </div>
        )}
        {gap.edits.length === 0 ? (
          <p className="muted">
            {aiOn
              ? "Nothing to suggest from the rules. Ask the AI for rewording ideas."
              : "Nothing to suggest right now."}
          </p>
        ) : (
          <ul className="edit-list">
            {gap.edits.map((e) => (
              <EditCard key={e.id} edit={e} busy={busy} onDecide={(a) => decide(e, a)} />
            ))}
          </ul>
        )}
        {accepted && (
          <p className="field-hint">
            Accepted edits update your profile in JobLens. Copy them into your resume file too
            before you send it.
          </p>
        )}
      </section>

      <div className="gap-columns">
        <section className="job-section" aria-label="Basic checks">
          <h3>Basic checks</h3>
          <ul className="basic-checks">
            {gap.checks.map((c) => (
              <li key={c.id} data-ok={c.ok}>
                <span className="check-mark" aria-hidden="true">
                  {c.ok ? "✓" : "!"}
                </span>
                <span>
                  {c.label}
                  {c.note && <span className="req-note">{c.note}</span>}
                </span>
              </li>
            ))}
          </ul>
          <p className="field-hint">No overall “ATS score” — only what’s covered and what isn’t.</p>
        </section>
        <section className="job-section" aria-label="As hiring software reads it">
          <h3>As hiring software reads it</h3>
          {gap.resumeLines.length ? (
            <pre className="plain-resume">{gap.resumeLines.join("\n")}</pre>
          ) : (
            <p className="muted">Upload your resume to see the text hiring software gets.</p>
          )}
        </section>
      </div>
    </div>
  );
}
