import { useEffect, useState } from "react";
import type { InterviewPrep } from "../bindings/InterviewPrep";
import type { PrepTopic } from "../bindings/PrepTopic";
import { ipc } from "../lib/ipc";
import { useToast } from "./Toast";

const TAG: Record<PrepTopic["kind"], string> = {
  story: "",
  worded: "Worded differently",
  gap: "Not on your resume",
};

function TopicRow({ t, onToggle }: { t: PrepTopic; onToggle: () => void }) {
  return (
    <li className="prep-topic" data-kind={t.kind}>
      <input
        type="checkbox"
        checked={t.prepared}
        onChange={onToggle}
        aria-label={`Prepared: ${t.topic}`}
      />
      <div className="prep-body">
        <span className="prep-title">
          {t.topic}
          {TAG[t.kind] && <span className="prep-tag">{TAG[t.kind]}</span>}
        </span>
        {t.kind === "story" ? (
          <>
            <span className="muted">They may ask: “{t.prompt.replace(/^Tell me/, "Tell me")}”</span>
            {t.evidence[0] && (
              <span className="req-evidence">Your story: “{t.evidence[0].text}”</span>
            )}
          </>
        ) : (
          <span className="muted">{t.prompt}</span>
        )}
      </div>
    </li>
  );
}

/** "Interview topics": stories you have, gaps to expect, questions to ask. */
export function PrepPanel({ jobId }: { jobId: number }) {
  const toast = useToast();
  const [prep, setPrep] = useState<InterviewPrep | null>(null);

  useEffect(() => {
    let live = true;
    setPrep(null);
    ipc
      .getInterviewPrep(jobId)
      .then((p) => live && setPrep(p))
      .catch((err) => toast(`Couldn't load interview topics: ${err}`, "error"));
    return () => {
      live = false;
    };
  }, [jobId, toast]);

  if (!prep) return <p className="muted">Loading…</p>;

  const toggle = async (t: PrepTopic) => {
    const prepared = !t.prepared;
    setPrep(
      (p) => p && { ...p, topics: p.topics.map((x) => (x.key === t.key ? { ...x, prepared } : x)) },
    );
    try {
      await ipc.setTopicPrepared(jobId, t.key, prepared);
    } catch (err) {
      toast(String(err), "error");
    }
  };

  const stories = prep.topics.filter((t) => t.kind === "story");
  const gaps = prep.topics.filter((t) => t.kind !== "story");
  const done = prep.topics.filter((t) => t.prepared).length;

  return (
    <div className="gap-check">
      <p className="field-hint">
        Built from this posting and your resume. Nothing here is invented; each story points to a
        real line.{" "}
        <strong>
          {done} of {prep.topics.length}
        </strong>{" "}
        prepared.
      </p>
      <section className="job-section" aria-label="Likely topics">
        <h3>
          Likely topics <span className="h3-aside">· you have a story</span>
        </h3>
        {stories.length ? (
          <ul className="prep-list">
            {stories.map((t) => (
              <TopicRow key={t.key} t={t} onToggle={() => toggle(t)} />
            ))}
          </ul>
        ) : (
          <p className="muted">No matching requirements yet. Run the AI check for a fuller list.</p>
        )}
      </section>
      <section className="job-section" aria-label="Expect to be probed">
        <h3>
          Expect to be probed <span className="h3-aside">· gaps</span>
        </h3>
        {gaps.length ? (
          <ul className="prep-list">
            {gaps.map((t) => (
              <TopicRow key={t.key} t={t} onToggle={() => toggle(t)} />
            ))}
          </ul>
        ) : (
          <p className="muted">No gaps found.</p>
        )}
      </section>
      <section className="job-section" aria-label="Questions to ask them">
        <h3>Questions to ask them</h3>
        <ul className="ask-list">
          {prep.questions.map((q) => (
            <li key={q}>{q}</li>
          ))}
        </ul>
      </section>
    </div>
  );
}
