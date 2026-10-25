import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import type { Application } from "../bindings/Application";
import type { AppStatus } from "../bindings/AppStatus";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { useToast } from "../components/Toast";
import { formatDay, todayIso } from "../lib/format";
import { ipc } from "../lib/ipc";
import { ViewHeader } from "./ViewHeader";

const STATUSES: [AppStatus, string][] = [
  ["saved", "Saved"],
  ["applied", "Applied"],
  ["interviewing", "Interviewing"],
  ["offer", "Offer"],
  ["rejected", "Rejected"],
  ["closed", "Closed"],
];

const ACTIVE: AppStatus[] = ["applied", "interviewing", "offer"];

/** Follow-ups due today or overdue, for applications still in progress. */
export function dueToday(apps: Application[], today = todayIso()): Application[] {
  return apps.filter(
    (a) => ACTIVE.includes(a.status) && a.followUpAt !== null && a.followUpAt <= today,
  );
}

const DAY_MS = 86_400_000;

function daysBetween(from: string, to: string): number {
  return Math.round((Date.parse(to) - Date.parse(from)) / DAY_MS);
}

export type AppStats = {
  sent: number;
  thisWeek: number;
  /** Average over the last four weeks. */
  perWeek: number;
  /** Share of sent applications that got any answer (interview, offer or rejection). */
  responseRate: number | null;
  interviews: number;
};

export function appStats(apps: Application[], today = todayIso()): AppStats {
  const sent = apps.filter((a) => a.status !== "saved");
  const age = (a: Application) => (a.appliedAt ? daysBetween(a.appliedAt, today) : null);
  const within = (days: number) =>
    sent.filter((a) => {
      const d = age(a);
      return d !== null && d >= 0 && d < days;
    }).length;
  const answered = sent.filter((a) =>
    ["interviewing", "offer", "rejected"].includes(a.status),
  ).length;
  return {
    sent: sent.length,
    thisWeek: within(7),
    perWeek: Math.round((within(28) / 4) * 10) / 10,
    responseRate: sent.length ? Math.round((answered / sent.length) * 100) : null,
    interviews: sent.filter((a) => a.status === "interviewing" || a.status === "offer").length,
  };
}

const COLUMNS: [string, AppStatus[]][] = [
  ["Saved", ["saved"]],
  ["Applied", ["applied"]],
  ["Interviewing", ["interviewing"]],
  ["Offer", ["offer"]],
  ["Closed", ["closed", "rejected"]],
];

async function setStatus(app: Application, status: AppStatus): Promise<Application> {
  return ipc.updateApplication(app.jobId, {
    status,
    appliedAt: app.appliedAt,
    followUpAt: app.followUpAt,
    link: app.link,
    notes: app.notes,
  });
}

function Board({
  apps,
  today,
  onChange,
}: {
  apps: Application[];
  today: string;
  onChange: (a: Application) => void;
}) {
  const toast = useToast();
  return (
    <div className="board">
      {COLUMNS.map(([label, statuses]) => {
        const items = apps.filter((a) => statuses.includes(a.status));
        return (
          <section key={label} className="board-col" aria-label={`${label} (${items.length})`}>
            <h2>
              {label} <span className="mono">{items.length}</span>
            </h2>
            {items.length === 0 ? (
              <p className="muted">None</p>
            ) : (
              <ul>
                {items.map((a) => (
                  <li key={a.jobId} className="board-card" data-status={a.status}>
                    <strong>{a.title}</strong>
                    <span className="muted">{a.company}</span>
                    {a.appliedAt && (
                      <span className="app-cell-label">Applied {formatDay(a.appliedAt)}</span>
                    )}
                    {a.followUpAt && (
                      <span
                        className="app-cell-label"
                        data-overdue={ACTIVE.includes(a.status) && a.followUpAt <= today}
                      >
                        Follow up {a.followUpAt === today ? "today" : formatDay(a.followUpAt)}
                      </span>
                    )}
                    <select
                      className="input app-status"
                      aria-label={`Move ${a.title} at ${a.company}`}
                      value={a.status}
                      onChange={async (e) => {
                        try {
                          onChange(await setStatus(a, e.target.value as AppStatus));
                        } catch (err) {
                          toast(String(err), "error");
                        }
                      }}
                    >
                      {STATUSES.map(([s, l]) => (
                        <option key={s} value={s}>
                          {l}
                        </option>
                      ))}
                    </select>
                  </li>
                ))}
              </ul>
            )}
          </section>
        );
      })}
    </div>
  );
}

function ApplicationRow({
  app,
  today,
  onChange,
}: {
  app: Application;
  today: string;
  onChange: (a: Application) => void;
}) {
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState({
    appliedAt: app.appliedAt ?? "",
    followUpAt: app.followUpAt ?? "",
    link: app.link,
    notes: app.notes,
  });

  const save = async (status: AppStatus, fields = draft) => {
    try {
      const next = await ipc.updateApplication(app.jobId, {
        status,
        appliedAt: fields.appliedAt || null,
        followUpAt: fields.followUpAt || null,
        link: fields.link,
        notes: fields.notes,
      });
      onChange(next);
      setDraft({
        appliedAt: next.appliedAt ?? "",
        followUpAt: next.followUpAt ?? "",
        link: next.link,
        notes: next.notes,
      });
      return true;
    } catch (err) {
      toast(String(err), "error");
      return false;
    }
  };

  const overdue = ACTIVE.includes(app.status) && app.followUpAt !== null && app.followUpAt <= today;
  return (
    <li className="app-row" data-status={app.status}>
      <div className="app-row-main">
        <button
          type="button"
          className="app-row-title"
          aria-expanded={open}
          onClick={() => setOpen((v) => !v)}
        >
          <Icon name={open ? "expand_more" : "chevron_right"} />
          <span>
            <strong>{app.title}</strong>
            <span className="muted"> · {app.company}</span>
          </span>
        </button>
        <select
          className="input app-status"
          aria-label={`Status for ${app.title} at ${app.company}`}
          value={app.status}
          onChange={(e) => save(e.target.value as AppStatus)}
        >
          {STATUSES.map(([s, label]) => (
            <option key={s} value={s}>
              {label}
            </option>
          ))}
        </select>
        <span className="app-cell">
          <span className="app-cell-label">Applied</span>
          {app.appliedAt ? formatDay(app.appliedAt) : "—"}
        </span>
        <span className="app-cell">
          <span className="app-cell-label">Sent</span>
          {[app.resumeVersion, app.letter].filter(Boolean).join(" · ") || "—"}
        </span>
        <span className="app-cell" data-overdue={overdue}>
          <span className="app-cell-label">Follow up</span>
          {app.followUpAt ? (app.followUpAt === today ? "Today" : formatDay(app.followUpAt)) : "—"}
        </span>
      </div>
      {!open && app.notes && <p className="app-notes-preview">{app.notes}</p>}
      {open && (
        <form
          className="app-edit"
          onSubmit={async (e) => {
            e.preventDefault();
            if (await save(app.status)) toast("Saved.");
          }}
        >
          <label className="field">
            <span>Applied on</span>
            <input
              className="input"
              type="date"
              value={draft.appliedAt}
              onChange={(e) => setDraft({ ...draft, appliedAt: e.target.value })}
            />
          </label>
          <label className="field">
            <span>Follow up on</span>
            <input
              className="input"
              type="date"
              value={draft.followUpAt}
              onChange={(e) => setDraft({ ...draft, followUpAt: e.target.value })}
            />
          </label>
          <label className="field app-edit-wide">
            <span>Where you applied</span>
            <input
              className="input"
              placeholder="Link to the application portal, if not the posting"
              value={draft.link}
              onChange={(e) => setDraft({ ...draft, link: e.target.value })}
            />
          </label>
          <label className="field app-edit-wide">
            <span>Notes</span>
            <textarea
              className="input"
              rows={3}
              value={draft.notes}
              onChange={(e) => setDraft({ ...draft, notes: e.target.value })}
            />
          </label>
          <div className="app-edit-actions">
            <button type="submit" className="btn btn-secondary">
              Save
            </button>
            {app.url && (
              <button
                type="button"
                className="btn btn-ghost"
                onClick={() => openUrl(app.url ?? "")}
              >
                <Icon name="open_in_new" />
                Posting
              </button>
            )}
            {draft.link.startsWith("http") && (
              <button type="button" className="btn btn-ghost" onClick={() => openUrl(draft.link)}>
                <Icon name="open_in_new" />
                Where you applied
              </button>
            )}
          </div>
        </form>
      )}
    </li>
  );
}

function Stats({ stats }: { stats: AppStats }) {
  const tiles: [string, string][] = [
    ["Applied", String(stats.sent)],
    ["This week", String(stats.thisWeek)],
    ["Per week", String(stats.perWeek)],
    ["Response rate", stats.responseRate === null ? "—" : `${stats.responseRate}%`],
    ["Interviews", String(stats.interviews)],
  ];
  return (
    <dl className="stats" aria-label="Stats">
      {tiles.map(([label, value]) => (
        <div key={label} className="stat">
          <dt>{label}</dt>
          <dd>{value}</dd>
        </div>
      ))}
    </dl>
  );
}

export function ApplicationsView() {
  const toast = useToast();
  const [apps, setApps] = useState<Application[] | null>(null);
  const [layout, setLayout] = useState<"list" | "board">("list");
  const today = todayIso();

  useEffect(() => {
    ipc
      .listApplications()
      .then(setApps)
      .catch((err) => toast(`Couldn't load your applications: ${err}`, "error"));
  }, [toast]);

  const replace = (a: Application) =>
    setApps((list) => list?.map((x) => (x.jobId === a.jobId ? a : x)) ?? null);

  const exportCsv = async () => {
    try {
      const path = await ipc.exportApplicationsCsv();
      toast(`Saved ${path.split(/[\\/]/).pop()} to Downloads.`);
      void revealItemInDir(path).catch(() => {});
    } catch (err) {
      toast(`Couldn't export: ${err}`, "error");
    }
  };

  const followedUp = async (a: Application) => {
    try {
      const next = await ipc.markFollowedUp(a.jobId);
      replace(next);
      toast(`Noted. Next follow-up ${next.followUpAt ? formatDay(next.followUpAt) : "not set"}.`);
    } catch (err) {
      toast(String(err), "error");
    }
  };

  const due = apps ? dueToday(apps, today) : [];
  const active = apps ? apps.filter((a) => ACTIVE.includes(a.status)).length : 0;

  return (
    <section className="view">
      <ViewHeader
        title="Applications"
        subtitle={
          apps?.length
            ? `${apps.length} tracked · ${active} active · ${due.length} follow-up${due.length === 1 ? "" : "s"} due`
            : "Track where you applied and when to follow up."
        }
        aside={
          apps?.length ? (
            <button type="button" className="btn btn-secondary" onClick={exportCsv}>
              <Icon name="download" />
              Export .csv
            </button>
          ) : undefined
        }
      />
      {!apps ? (
        <p className="muted feed-pad">Loading…</p>
      ) : apps.length === 0 ? (
        <EmptyState icon="checklist" title="Nothing here yet">
          Jobs you save or apply to show up here.
        </EmptyState>
      ) : (
        <div className="view-body">
          {due.length > 0 && (
            <section className="due-strip" aria-label="Follow up today">
              <h2>
                <Icon name="notifications_active" />
                Follow up today
              </h2>
              <ul>
                {due.map((a) => (
                  <li key={a.jobId}>
                    <span>
                      <strong>{a.company}</strong> · {a.title}
                      <span className="muted">
                        {a.appliedAt ? ` · applied ${formatDay(a.appliedAt)}` : ""}
                      </span>
                    </span>
                    <button
                      type="button"
                      className="btn btn-secondary"
                      onClick={() => followedUp(a)}
                    >
                      Mark followed up
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          )}
          <Stats stats={appStats(apps, today)} />
          <fieldset className="segmented app-layout" aria-label="Layout">
            {(
              [
                ["list", "List"],
                ["board", "Board"],
              ] as const
            ).map(([l, label]) => (
              <button
                key={l}
                type="button"
                aria-pressed={layout === l}
                onClick={() => setLayout(l)}
              >
                {label}
              </button>
            ))}
          </fieldset>
          {layout === "board" ? (
            <Board apps={apps} today={today} onChange={replace} />
          ) : (
            <ul className="app-list" aria-label="Applications">
              {apps.map((a) => (
                <ApplicationRow key={a.jobId} app={a} today={today} onChange={replace} />
              ))}
            </ul>
          )}
        </div>
      )}
    </section>
  );
}
