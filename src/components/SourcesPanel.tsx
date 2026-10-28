import { type FormEvent, useCallback, useEffect, useState } from "react";
import type { ManualJobInput } from "../bindings/ManualJobInput";
import type { SourceStatus } from "../bindings/SourceStatus";
import { formatAge, formatClock } from "../lib/feed";
import { ipc } from "../lib/ipc";
import { plural } from "../lib/profile";
import { Icon } from "./Icon";
import { useToast } from "./Toast";

const EMPTY_MANUAL: ManualJobInput = {
  url: "",
  title: "",
  company: "",
  location: "",
  description: "",
};

function statusLine(s: SourceStatus): string {
  if (!s.enabled) return "Off";
  if (s.lastError) return s.lastError;
  if (s.lastOkAt) return `Checked ${formatAge(s.lastOkAt)} at ${formatClock(s.lastOkAt)}`;
  return "Not checked yet";
}

/** Settings → Job sources: boards, the company watchlist, and adding a job by hand. */
export function SourcesPanel() {
  const toast = useToast();
  const [sources, setSources] = useState<SourceStatus[] | null>(null);
  const [company, setCompany] = useState("");
  const [finding, setFinding] = useState(false);
  const [manual, setManual] = useState<ManualJobInput>(EMPTY_MANUAL);
  const [adding, setAdding] = useState(false);

  const load = useCallback(async () => {
    try {
      setSources(await ipc.listSources());
    } catch (err) {
      toast(`Couldn't load job sources: ${err}`, "error");
    }
  }, [toast]);

  useEffect(() => {
    load();
  }, [load]);

  const toggle = async (s: SourceStatus) => {
    try {
      await ipc.setSourceEnabled(s.id, !s.enabled);
      setSources(
        (list) => list?.map((x) => (x.id === s.id ? { ...x, enabled: !s.enabled } : x)) ?? null,
      );
    } catch (err) {
      toast(`Couldn't change ${s.name}: ${err}`, "error");
    }
  };

  const remove = async (s: SourceStatus) => {
    try {
      await ipc.removeSource(s.id);
      setSources((list) => list?.filter((x) => x.id !== s.id) ?? null);
      toast(`Removed ${s.name}`);
    } catch (err) {
      toast(`Couldn't remove ${s.name}: ${err}`, "error");
    }
  };

  const addCompany = async (e: FormEvent) => {
    e.preventDefault();
    if (!company.trim()) return;
    setFinding(true);
    try {
      const added = await ipc.addWatchlistCompany(company);
      toast(`Added ${added.name} · ${plural(added.jobCount, "remote job")}`);
      setCompany("");
      await load();
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setFinding(false);
    }
  };

  const addManual = async (e: FormEvent) => {
    e.preventDefault();
    setAdding(true);
    try {
      await ipc.addManualJob(manual);
      toast("Saved to your feed.");
      setManual(EMPTY_MANUAL);
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setAdding(false);
    }
  };

  const field = (key: keyof ManualJobInput) => ({
    value: manual[key],
    onChange: (e: { target: { value: string } }) =>
      setManual((m) => ({ ...m, [key]: e.target.value })),
  });

  const boards = sources?.filter((s) => s.kind === "board") ?? [];
  const watchlist = sources?.filter((s) => s.kind === "watchlist") ?? [];

  return (
    <div className="panel" id="settings-sources">
      <h2>
        <Icon name="rss_feed" />
        Job sources
      </h2>
      <p className="panel-sub">
        Where jobs come from. A broken source never stops the others. Each job links back to where
        it was listed.
      </p>

      {!sources ? (
        <p className="panel-sub">Loading…</p>
      ) : (
        <div className="source-grid">
          {[...boards, ...watchlist].map((s) => (
            <div
              key={s.id}
              className="source-card"
              data-failed={(s.enabled && !!s.lastError) || undefined}
            >
              <div className="source-card-head">
                <span className="source-card-name">
                  <Icon
                    name={
                      s.enabled && s.lastError
                        ? "error"
                        : s.kind === "watchlist"
                          ? "visibility"
                          : "rss_feed"
                    }
                  />
                  <span>
                    {s.name}
                    {s.detail && <span className="source-card-detail">{s.detail}</span>}
                  </span>
                </span>
                <button
                  type="button"
                  role="switch"
                  className="switch"
                  aria-checked={s.enabled}
                  aria-label={`${s.name} on`}
                  onClick={() => toggle(s)}
                >
                  <span />
                </button>
              </div>
              <div className="source-card-foot">
                <span className="source-card-status">{statusLine(s)}</span>
                <span className="mono">{plural(s.jobCount, "job")}</span>
              </div>
              {s.kind === "watchlist" && (
                <button type="button" className="btn-link source-remove" onClick={() => remove(s)}>
                  Remove
                </button>
              )}
            </div>
          ))}
        </div>
      )}

      <div className="source-forms">
        <form className="source-form" onSubmit={addCompany}>
          <h3>Company watchlist</h3>
          <p className="field-hint">
            Add a company that hires on Greenhouse, Lever, Ashby, Workable, Recruitee or
            SmartRecruiters. We check its board for remote openings.
          </p>
          <div className="inline-form">
            <input
              aria-label="Company name or careers page link"
              placeholder="Company name or careers page link"
              value={company}
              onChange={(e) => setCompany(e.target.value)}
              disabled={finding}
            />
            <button
              type="submit"
              className="btn btn-secondary"
              disabled={finding || !company.trim()}
            >
              <Icon
                name={finding ? "progress_activity" : "add"}
                className={finding ? "spin" : undefined}
              />
              {finding ? "Looking…" : "Add"}
            </button>
          </div>
        </form>

        <form className="source-form" onSubmit={addManual}>
          <h3>Save a job you found elsewhere</h3>
          <p className="field-hint">
            Seen a job on LinkedIn or a company site? Save it here so JobLens can match and track
            it. Only you see it. Greenhouse, Lever, Ashby, Workable, Recruitee and SmartRecruiters
            links are read for you; for anything else, paste the description.
          </p>
          <input aria-label="Job link" placeholder="Job link (optional)" {...field("url")} />
          <div className="form-row">
            <input aria-label="Job title" placeholder="Job title" {...field("title")} />
            <input aria-label="Company" placeholder="Company" {...field("company")} />
          </div>
          <input
            aria-label="Location"
            placeholder="Location as the posting says, e.g. “Remote, EU only”"
            {...field("location")}
          />
          <textarea
            aria-label="Job description"
            rows={4}
            placeholder="Paste the full job description"
            {...field("description")}
          />
          <button
            type="submit"
            className="btn btn-secondary align-self-start"
            disabled={adding || (!manual.url.trim() && !manual.description.trim())}
          >
            <Icon
              name={adding ? "progress_activity" : "bookmark_add"}
              className={adding ? "spin" : undefined}
            />
            {adding ? "Saving…" : "Save to my feed"}
          </button>
        </form>
      </div>
    </div>
  );
}
