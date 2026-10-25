import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Fragment, useEffect, useState } from "react";
import type { Draft } from "../bindings/Draft";
import type { DraftKind } from "../bindings/DraftKind";
import type { DraftLength } from "../bindings/DraftLength";
import type { ExportFormat } from "../bindings/ExportFormat";
import type { Sentence } from "../bindings/Sentence";
import type { WriterState } from "../bindings/WriterState";
import { ipc } from "../lib/ipc";
import { Icon } from "./Icon";
import { useToast } from "./Toast";

type WriterPanelProps = {
  jobId: number;
  aiOn: boolean;
};

const MAX_HIGHLIGHTS = 3;

/** Counts sentences still waiting for "It's true — keep" or "Remove". */
export function openFlags(d: Draft): number {
  return d.paragraphs
    .flatMap((p) => p.versions[p.current]?.sentences ?? [])
    .filter((s) => s.backing === "unbacked").length;
}

/** Text with [blanks] marked so they stand out. */
function WithBlanks({ text }: { text: string }) {
  const parts = text.split(/(\[[^\]]+\])/);
  return (
    <>
      {parts.map((part, i) =>
        /^\[[^\]]+\]$/.test(part) ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: parts of a fixed string
          <mark key={i} className="blank">
            {part}
          </mark>
        ) : (
          // biome-ignore lint/suspicious/noArrayIndexKey: parts of a fixed string
          <Fragment key={i}>{part}</Fragment>
        ),
      )}
    </>
  );
}

function sentenceTitle(s: Sentence): string | undefined {
  if (s.backing === "backed" && s.evidence.length) {
    return `From your resume: ${s.evidence.map((e) => `“${e.text}”`).join(" · ")}`;
  }
  if (s.backing === "confirmed") return "You confirmed this is true.";
  return undefined;
}

function short(text: string): string {
  return text.length > 60 ? `${text.slice(0, 58)}…` : text;
}

/** "Write application": a cover letter or email, honesty-checked sentence by sentence. */
export function WriterPanel({ jobId, aiOn }: WriterPanelProps) {
  const toast = useToast();
  const [kind, setKind] = useState<DraftKind>("cover_letter");
  const [state, setState] = useState<WriterState | null>(null);
  const [length, setLength] = useState<DraftLength>("standard");
  const [highlights, setHighlights] = useState<string[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [editing, setEditing] = useState<{ index: number; text: string } | null>(null);
  const [savedPath, setSavedPath] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    setState(null);
    setEditing(null);
    setSavedPath(null);
    ipc
      .getWriter(jobId, kind)
      .then((s) => {
        if (!live) return;
        setState(s);
        setLength(s.draft?.length ?? (kind === "email" ? "short" : "standard"));
        setHighlights(s.draft?.highlights ?? s.suggested.slice(0, MAX_HIGHLIGHTS));
      })
      .catch((err) => toast(`Couldn't open the writer: ${err}`, "error"));
    return () => {
      live = false;
    };
  }, [jobId, kind, toast]);

  if (!state) return <p className="muted">Loading…</p>;
  const draft = state.draft;
  const setDraft = (d: Draft) => setState((s) => s && { ...s, draft: d });

  /** Runs a draft change, showing errors as toasts. */
  const run = async (label: string, call: () => Promise<Draft>) => {
    setBusy(label);
    try {
      setDraft(await call());
      setSavedPath(null);
    } catch (err) {
      const msg = String(err);
      toast(
        msg.startsWith("Cancelled") ? "Cancelled." : msg,
        msg.startsWith("Cancelled") ? "info" : "error",
      );
    } finally {
      setBusy(null);
    }
  };

  const write = (useAi: boolean) =>
    run(useAi ? "ai" : "write", () => ipc.writeDraft(jobId, kind, length, highlights, useAi));

  const toggle = (id: string) =>
    setHighlights((h) =>
      h.includes(id) ? h.filter((x) => x !== id) : h.length < MAX_HIGHLIGHTS ? [...h, id] : h,
    );

  const saveText = (patch: Partial<Pick<Draft, "subject" | "greeting" | "signOff">>) => {
    if (!draft) return;
    const next = { ...draft, ...patch };
    if (
      next.subject === draft.subject &&
      next.greeting === draft.greeting &&
      next.signOff === draft.signOff
    ) {
      return;
    }
    void run("text", () =>
      ipc.updateDraftText(jobId, kind, next.subject, next.greeting, next.signOff),
    );
  };

  const exportAs = async (format: ExportFormat) => {
    setBusy(`export-${format}`);
    try {
      const r = await ipc.exportDraft(jobId, kind, format);
      if (format === "text") {
        await navigator.clipboard.writeText(r.text);
        toast("Copied to the clipboard.");
      } else {
        setSavedPath(r.path);
        toast(`Saved ${r.path?.split(/[\\/]/).pop() ?? "the file"} to Downloads.`);
      }
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setBusy(null);
    }
  };

  const open = draft ? openFlags(draft) : 0;
  const blanks = draft
    ? ([
        draft.subject,
        draft.greeting,
        draft.signOff,
        ...draft.paragraphs.flatMap((p) =>
          (p.versions[p.current]?.sentences ?? []).map((s) => s.text),
        ),
      ]
        .join(" ")
        .match(/\[[^\]]+\]/g)?.length ?? 0)
    : 0;
  const working = busy === "ai" || busy?.startsWith("rewrite-ai");

  return (
    <div className="writer">
      <div className="writer-controls">
        <fieldset className="segmented" aria-label="What to write">
          {(
            [
              ["cover_letter", "Cover letter"],
              ["email", "Email"],
            ] as const
          ).map(([k, label]) => (
            <button key={k} type="button" aria-pressed={kind === k} onClick={() => setKind(k)}>
              {label}
            </button>
          ))}
        </fieldset>
        <fieldset className="segmented" aria-label="Length">
          {(
            [
              ["short", "Short"],
              ["standard", "Standard"],
            ] as const
          ).map(([l, label]) => (
            <button key={l} type="button" aria-pressed={length === l} onClick={() => setLength(l)}>
              {label}
            </button>
          ))}
        </fieldset>
      </div>

      <section className="job-section" aria-label="Highlights">
        <h3>
          Highlight 2–3 experiences <span className="h3-aside">· {highlights.length} chosen</span>
        </h3>
        {state.options.length === 0 ? (
          <p className="muted">
            Add achievements or projects on the Resume page to highlight them.
          </p>
        ) : (
          <div className="highlight-chips">
            {state.options.map((o) => (
              <button
                key={o.id}
                type="button"
                className="highlight-chip"
                aria-pressed={highlights.includes(o.id)}
                aria-label={`${o.label} (${o.detail})`}
                title={o.detail}
                onClick={() => toggle(o.id)}
              >
                {short(o.label)}
              </button>
            ))}
          </div>
        )}
        <div className="writer-actions">
          <button
            type="button"
            className="btn btn-secondary"
            disabled={busy !== null}
            onClick={() => write(false)}
          >
            <Icon name="edit_note" />
            {draft ? "Write again" : "Write draft"}
          </button>
          {aiOn && (
            <button
              type="button"
              className="btn btn-secondary"
              disabled={busy !== null}
              onClick={() => write(true)}
            >
              <Icon name="auto_awesome" />
              Write with AI
            </button>
          )}
          {draft && <span className="field-hint">Writing again replaces the current draft.</span>}
        </div>
        {working && (
          <div className="ai-running" aria-busy="true">
            <Icon name="progress_activity" className="spin" />
            <span>Writing from your resume… usually 20–40 seconds.</span>
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => ipc.cancelWriting(jobId)}
            >
              Cancel
            </button>
          </div>
        )}
      </section>

      {!draft ? (
        <p className="muted">
          Pick what to highlight, then write a draft. Every sentence about you is checked against
          your resume.
        </p>
      ) : (
        <>
          <div className="honesty" data-open={open > 0} role="status">
            <Icon name="verified_user" />
            <span>
              <strong>Honesty check:</strong>{" "}
              {open > 0
                ? `${open} sentence${open === 1 ? " isn’t" : "s aren’t"} backed by your resume. Keep or remove ${open === 1 ? "it" : "them"} before using the ${kind === "email" ? "email" : "letter"}.`
                : blanks > 0
                  ? `every claim is backed by your resume. Fill in the ${blanks} marked blank${blanks === 1 ? "" : "s"} before sending.`
                  : "every claim is backed by your resume."}
            </span>
          </div>

          <article
            className="letter"
            aria-label={kind === "email" ? "Email draft" : "Cover letter draft"}
          >
            {kind === "email" && (
              <label className="letter-line">
                <span className="muted">Subject:</span>
                <input
                  className="input"
                  aria-label="Subject"
                  defaultValue={draft.subject}
                  key={`s-${draft.updatedAt}`}
                  onBlur={(e) => saveText({ subject: e.target.value })}
                />
              </label>
            )}
            <input
              className="input letter-greeting"
              aria-label="Greeting"
              defaultValue={draft.greeting}
              key={`g-${draft.updatedAt}`}
              onBlur={(e) => saveText({ greeting: e.target.value })}
            />

            {draft.paragraphs.map((p, pi) => {
              const v = p.versions[p.current] ?? p.versions[0];
              const key = `${pi}-${p.versions.length}`;
              if (editing?.index === pi) {
                return (
                  <div key={key} className="letter-para">
                    <textarea
                      className="input"
                      aria-label={`Edit paragraph ${pi + 1}`}
                      rows={5}
                      value={editing.text}
                      onChange={(e) => setEditing({ index: pi, text: e.target.value })}
                    />
                    <div className="para-tools">
                      <button
                        type="button"
                        className="btn btn-secondary"
                        disabled={busy !== null}
                        onClick={async () => {
                          await run("edit", () => ipc.editParagraph(jobId, kind, pi, editing.text));
                          setEditing(null);
                        }}
                      >
                        Save paragraph
                      </button>
                      <button
                        type="button"
                        className="btn btn-ghost"
                        onClick={() => setEditing(null)}
                      >
                        Cancel
                      </button>
                      <span className="field-hint">
                        New sentences are checked against your resume.
                      </span>
                    </div>
                  </div>
                );
              }
              return (
                <div key={key} className="letter-para">
                  <p>
                    {v.sentences.map((s, si) => (
                      // biome-ignore lint/suspicious/noArrayIndexKey: sentence order is the identity
                      <Fragment key={si}>
                        <span
                          className="sentence"
                          data-backing={s.backing}
                          title={sentenceTitle(s)}
                        >
                          <WithBlanks text={s.text} />
                        </span>{" "}
                      </Fragment>
                    ))}
                  </p>
                  {v.sentences.map((s, si) =>
                    s.backing === "unbacked" ? (
                      <div key={s.text} className="flag-row">
                        <span>
                          {s.note.startsWith("Nothing")
                            ? `Nothing in your resume backs “${short(s.text)}”.`
                            : `“${short(s.text)}”: ${s.note}`}
                        </span>
                        <button
                          type="button"
                          className="btn btn-ghost"
                          disabled={busy !== null}
                          onClick={() =>
                            run("resolve", () => ipc.resolveSentence(jobId, kind, pi, si, true))
                          }
                        >
                          It’s true — keep
                        </button>
                        <button
                          type="button"
                          className="btn btn-ghost"
                          disabled={busy !== null}
                          onClick={() =>
                            run("resolve", () => ipc.resolveSentence(jobId, kind, pi, si, false))
                          }
                        >
                          Remove
                        </button>
                      </div>
                    ) : null,
                  )}
                  <div className="para-tools">
                    <button
                      type="button"
                      className="btn btn-ghost"
                      disabled={busy !== null}
                      onClick={() =>
                        run(aiOn ? "rewrite-ai" : "rewrite", () =>
                          ipc.rewriteParagraph(jobId, kind, pi, aiOn),
                        )
                      }
                    >
                      <Icon name="autorenew" />
                      Rewrite paragraph
                    </button>
                    <button
                      type="button"
                      className="btn btn-ghost"
                      disabled={busy !== null}
                      onClick={() =>
                        setEditing({ index: pi, text: v.sentences.map((s) => s.text).join(" ") })
                      }
                    >
                      <Icon name="edit" />
                      Edit
                    </button>
                    {p.versions.length > 1 && (
                      <span className="draft-nav">
                        <button
                          type="button"
                          className="btn btn-ghost"
                          aria-label="Earlier draft"
                          disabled={busy !== null || p.current === 0}
                          onClick={() =>
                            run("version", () =>
                              ipc.setParagraphVersion(jobId, kind, pi, p.current - 1),
                            )
                          }
                        >
                          <Icon name="chevron_left" />
                        </button>
                        <span className="mono">
                          draft {p.current + 1} of {p.versions.length}
                        </span>
                        <button
                          type="button"
                          className="btn btn-ghost"
                          aria-label="Later draft"
                          disabled={busy !== null || p.current >= p.versions.length - 1}
                          onClick={() =>
                            run("version", () =>
                              ipc.setParagraphVersion(jobId, kind, pi, p.current + 1),
                            )
                          }
                        >
                          <Icon name="chevron_right" />
                        </button>
                      </span>
                    )}
                  </div>
                </div>
              );
            })}

            <textarea
              className="input letter-signoff"
              aria-label="Sign-off"
              rows={2}
              defaultValue={draft.signOff}
              key={`o-${draft.updatedAt}`}
              onBlur={(e) => saveText({ signOff: e.target.value })}
            />
          </article>

          <div className="writer-export">
            <button
              type="button"
              className="btn btn-secondary"
              disabled={open > 0 || busy !== null}
              onClick={() => exportAs("text")}
            >
              <Icon name="content_copy" />
              Copy
            </button>
            <button
              type="button"
              className="btn btn-secondary"
              disabled={open > 0 || busy !== null}
              onClick={() => exportAs("docx")}
            >
              <Icon name="description" />
              Download .docx
            </button>
            <button
              type="button"
              className="btn btn-secondary"
              disabled={open > 0 || busy !== null}
              onClick={() => exportAs("pdf")}
            >
              <Icon name="picture_as_pdf" />
              Download PDF
            </button>
            {open > 0 && <span className="field-hint">Resolve highlighted sentences first.</span>}
            {savedPath && (
              <button
                type="button"
                className="btn btn-link"
                onClick={() => revealItemInDir(savedPath)}
              >
                Show in folder
              </button>
            )}
          </div>
        </>
      )}
    </div>
  );
}
