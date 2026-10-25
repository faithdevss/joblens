import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Feed } from "../bindings/Feed";
import type { JobDetail } from "../bindings/JobDetail";
import type { JobState } from "../bindings/JobState";
import type { JobSummary } from "../bindings/JobSummary";
import type { SavedSearch } from "../bindings/SavedSearch";
import type { SearchSettings } from "../bindings/SearchSettings";
import type { Weights } from "../bindings/Weights";
import { EligibilityBadge } from "../components/EligibilityBadge";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { JobDetailPane } from "../components/JobDetailPane";
import { useToast } from "../components/Toast";
import { onFeedUpdated } from "../lib/events";
import {
  applyFilters,
  DEFAULT_FILTERS,
  type FeedFilters,
  formatAge,
  formatClock,
  jobDate,
  refreshSummary,
  salaryText,
  sourceOptions,
} from "../lib/feed";
import { formatDay } from "../lib/format";
import { ipc } from "../lib/ipc";
import type { ChangeProfile } from "../lib/useAppData";

type FeedViewProps = {
  search: SearchSettings;
  searches: SavedSearch[];
  onSwitchSearch: (id: number) => Promise<void>;
  resumeLines: string[];
  changeProfile: ChangeProfile;
  onOpenSources: () => void;
};

const DEFAULT_WEIGHTS: Weights = { skills: 40, experience: 25, role: 20, seniority: 15 };

const ELIGIBILITY_CHIPS = [
  ["all", "All"],
  ["eligible", "Eligible"],
  ["restricted", "Restricted"],
  ["not_stated", "Not stated"],
] as const;

export function FeedView({
  search,
  searches,
  onSwitchSearch,
  resumeLines,
  changeProfile,
  onOpenSources,
}: FeedViewProps) {
  const toast = useToast();
  const [feed, setFeed] = useState<Feed | null>(null);
  const [filters, setFilters] = useState<FeedFilters>(DEFAULT_FILTERS);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [detail, setDetail] = useState<JobDetail | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [weights, setWeights] = useState<Weights | null>(null);
  const [aiOn, setAiOn] = useState(false);
  const [analyzingId, setAnalyzingId] = useState<number | null>(null);
  const weightsTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const load = useCallback(async () => {
    try {
      setFeed(await ipc.getFeed());
    } catch (err) {
      toast(`Couldn't load the feed: ${err}`, "error");
    }
  }, [toast]);

  const refresh = useCallback(async () => {
    setRefreshing(true);
    try {
      const report = await ipc.refreshSources();
      const failed = report.runs.some((r) => r.outcome.kind === "failed");
      toast(refreshSummary(report), failed ? "error" : "info");
      await load();
    } catch (err) {
      toast(`Couldn't refresh: ${err}`, "error");
    } finally {
      setRefreshing(false);
    }
  }, [load, toast]);

  useEffect(() => {
    load();
    ipc.getWeights().then(setWeights, () => {});
    ipc.getAiSettings().then(
      (s) => setAiOn(s.provider !== null),
      () => {},
    );
  }, [load]);

  useEffect(() => () => clearTimeout(weightsTimer.current), []);

  // Scores and keyword filters come from the active search; reload after a switch.
  const activeSearch = searches.find((s) => s.active)?.id;
  const shownSearch = useRef(activeSearch);
  useEffect(() => {
    if (shownSearch.current === activeSearch) return;
    shownSearch.current = activeSearch;
    load();
  }, [activeSearch, load]);

  // Background refreshes: show the new jobs and mention great matches.
  useEffect(
    () =>
      onFeedUpdated(({ alerts }) => {
        load();
        const [top] = alerts;
        if (alerts.length === 1)
          toast(`New great match: ${top.title} at ${top.company} (${top.score}%)`);
        else if (alerts.length > 1) toast(`${alerts.length} new great matches in your feed`);
      }),
    [load, toast],
  );

  // First open: fetch right away instead of showing an empty feed.
  const neverRefreshed = feed?.neverRefreshed ?? false;
  useEffect(() => {
    if (neverRefreshed) refresh();
  }, [neverRefreshed, refresh]);

  const jobs = useMemo(
    () => (feed ? applyFilters(feed.jobs, filters, search) : []),
    [feed, filters, search],
  );
  const selected = jobs.find((j) => j.id === selectedId) ?? jobs[0] ?? null;
  const sources = useMemo(() => sourceOptions(feed?.jobs ?? []), [feed]);

  const selectedKey = selected?.id ?? null;
  // biome-ignore lint/correctness/useExhaustiveDependencies: the score depends on the active search
  useEffect(() => {
    if (selectedKey === null) return;
    let live = true;
    ipc
      .getJob(selectedKey)
      .then((d) => live && setDetail(d))
      .catch((err) => toast(`Couldn't open this job: ${err}`, "error"));
    return () => {
      live = false;
    };
  }, [selectedKey, activeSearch, toast]);

  const changeWeights = (next: Weights) => {
    setWeights(next);
    clearTimeout(weightsTimer.current);
    weightsTimer.current = setTimeout(async () => {
      try {
        await ipc.saveWeights(next);
        await load();
        if (selectedKey !== null) setDetail(await ipc.getJob(selectedKey));
      } catch (err) {
        toast(`Couldn't save weights: ${err}`, "error");
      }
    }, 400);
  };

  const analyze = async (job: JobSummary, force: boolean) => {
    setAnalyzingId(job.id);
    try {
      const d = await ipc.analyzeJob(job.id, force);
      setDetail(d);
      setFeed(
        (f) =>
          f && {
            ...f,
            jobs: f.jobs.map((j) =>
              j.id === job.id ? { ...j, score: d.job.score, aiChecked: d.job.aiChecked } : j,
            ),
          },
      );
    } catch (err) {
      const msg = String(err);
      toast(
        msg.startsWith("Cancelled") ? "Check cancelled." : msg,
        msg.startsWith("Cancelled") ? "info" : "error",
      );
    } finally {
      setAnalyzingId(null);
    }
  };

  const setJobState = async (job: JobSummary, state: JobState) => {
    try {
      await ipc.setJobState(job.id, state);
    } catch (err) {
      toast(`Couldn't update this job: ${err}`, "error");
      return;
    }
    setFeed((f) => f && { ...f, jobs: f.jobs.map((j) => (j.id === job.id ? { ...j, state } : j)) });
    if (state === "dismissed" && filters.show !== "dismissed") {
      // Move on to the next job instead of the first one.
      const i = jobs.findIndex((j) => j.id === job.id);
      setSelectedId(jobs[i + 1]?.id ?? jobs[i - 1]?.id ?? null);
    }
    toast(
      state === "saved"
        ? `Saved ${job.company}`
        : state === "dismissed"
          ? `Dismissed ${job.company}`
          : `Restored ${job.company}`,
    );
  };

  const markApplied = async (job: JobSummary) => {
    try {
      const app = await ipc.markApplied(job.id);
      setFeed(
        (f) =>
          f && { ...f, jobs: f.jobs.map((j) => (j.id === job.id ? { ...j, state: "saved" } : j)) },
      );
      toast(
        app.followUpAt
          ? `Added to Applications · follow up ${formatDay(app.followUpAt)}`
          : "Added to Applications",
      );
    } catch (err) {
      toast(`Couldn't mark this job applied: ${err}`, "error");
    }
  };

  const set = (patch: Partial<FeedFilters>) => setFilters((f) => ({ ...f, ...patch }));
  const counts = feed
    ? {
        saved: feed.jobs.filter((j) => j.state === "saved").length,
        dismissed: feed.jobs.filter((j) => j.state === "dismissed").length,
      }
    : { saved: 0, dismissed: 0 };

  return (
    <section className="feed">
      <div className="feed-list-col">
        <div className="feed-head">
          <div className="feed-title-row">
            <div>
              <h1>Job feed</h1>
              {searches.length > 1 && (
                <select
                  className="feed-search"
                  aria-label="Saved search"
                  value={activeSearch}
                  onChange={(e) =>
                    onSwitchSearch(Number(e.target.value)).catch((err) =>
                      toast(`Couldn't switch searches: ${err}`, "error"),
                    )
                  }
                >
                  {searches.map((s) => (
                    <option key={s.id} value={s.id}>
                      {s.name}
                    </option>
                  ))}
                </select>
              )}
              <p className="muted">
                {feed?.lastUpdatedAt
                  ? `Updated ${formatAge(feed.lastUpdatedAt)} at ${formatClock(feed.lastUpdatedAt)}`
                  : "Not updated yet"}
              </p>
            </div>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={refresh}
              disabled={refreshing}
            >
              <Icon name="refresh" className={refreshing ? "spin" : undefined} />
              {refreshing ? "Refreshing…" : "Refresh"}
            </button>
          </div>

          <fieldset className="toggle-group" aria-label="Can I apply?">
            {ELIGIBILITY_CHIPS.map(([value, label]) => (
              <button
                key={value}
                type="button"
                className="toggle"
                aria-pressed={filters.eligibility === value}
                onClick={() => set({ eligibility: value })}
              >
                {label}
              </button>
            ))}
          </fieldset>

          <div className="feed-filters">
            <fieldset className="segmented" aria-label="Sort">
              <button
                type="button"
                aria-pressed={filters.sort === "best"}
                onClick={() => set({ sort: "best" })}
              >
                Best match
              </button>
              <button
                type="button"
                aria-pressed={filters.sort === "newest"}
                onClick={() => set({ sort: "newest" })}
              >
                Newest
              </button>
            </fieldset>
            <select
              aria-label="Minimum match score"
              value={filters.minScore}
              onChange={(e) => set({ minScore: Number(e.target.value) })}
            >
              <option value={0}>Any score</option>
              <option value={60}>Score 60+</option>
              <option value={70}>Score 70+</option>
              <option value={80}>Score 80+</option>
            </select>
            <select
              aria-label="Posted within"
              value={filters.maxAgeDays}
              onChange={(e) => set({ maxAgeDays: Number(e.target.value) })}
            >
              <option value={0}>Any time</option>
              <option value={3}>Last 3 days</option>
              <option value={7}>Last week</option>
              <option value={30}>Last month</option>
            </select>
            <select
              aria-label="Salary"
              value={filters.salary}
              onChange={(e) => set({ salary: e.target.value as FeedFilters["salary"] })}
            >
              <option value="any">Any salary</option>
              <option value="stated">Salary stated</option>
              <option value="minimum" disabled={!search.minSalary}>
                {search.minSalary
                  ? `≥ ${search.minSalary.toLocaleString("en")} ${search.currency}`
                  : "≥ your minimum (set in Settings)"}
              </option>
            </select>
            <select
              aria-label="Source"
              value={filters.source}
              onChange={(e) => set({ source: e.target.value })}
            >
              <option value="">All sources</option>
              {sources.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
            <select
              aria-label="Show"
              value={filters.show}
              onChange={(e) => set({ show: e.target.value as FeedFilters["show"] })}
            >
              <option value="active">New and saved</option>
              <option value="saved">Saved ({counts.saved})</option>
              <option value="dismissed">Dismissed ({counts.dismissed})</option>
            </select>
          </div>
          <div className="feed-count">
            {jobs.length} shown
            {feed &&
              feed.hiddenByKeywords > 0 &&
              ` · ${feed.hiddenByKeywords} hidden by your keywords`}
          </div>
        </div>

        <div className="job-list">
          {!feed ? (
            <p className="muted feed-pad">Loading…</p>
          ) : feed.jobs.length === 0 ? (
            <EmptyState icon="work" title={refreshing ? "Looking for jobs…" : "No jobs yet"}>
              {refreshing ? (
                "Checking your job sources."
              ) : (
                <>
                  Turn on a job source or add companies to your watchlist.{" "}
                  <button type="button" className="btn-link" onClick={onOpenSources}>
                    Job sources
                  </button>
                </>
              )}
            </EmptyState>
          ) : jobs.length === 0 ? (
            <p className="muted feed-pad">
              No jobs match these filters. Loosen the score or eligibility filter.
            </p>
          ) : (
            <ul className="job-items" aria-label="Jobs">
              {jobs.map((j) => (
                <li key={j.id}>
                  <button
                    type="button"
                    className="job-row"
                    aria-current={selected?.id === j.id ? "true" : undefined}
                    onClick={() => setSelectedId(j.id)}
                  >
                    <span className="score-pill" title="Match score">
                      {j.score}
                    </span>
                    <span className="job-row-body">
                      <span className="job-row-title">{j.title}</span>
                      <span className="job-row-sub">
                        {j.company} · {salaryText(j.salary)}
                      </span>
                      <span className="job-row-meta">
                        <EligibilityBadge status={j.eligibility.status} />
                        <span>{formatAge(jobDate(j))}</span>
                        <span>{j.listings[0]?.sourceName ?? "Added by you"}</span>
                        {j.listings.length > 1 && <span>+{j.listings.length - 1}</span>}
                        {j.state === "saved" && (
                          <span className="saved-mark">
                            <Icon name="star" className="icon-filled" />
                            Saved
                          </span>
                        )}
                      </span>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>

      <div className="feed-detail-col">
        {selected ? (
          <JobDetailPane
            job={selected}
            detail={detail?.job.id === selected.id ? detail : null}
            resumeLines={resumeLines}
            onSetState={setJobState}
            onMarkApplied={markApplied}
            weights={weights ?? DEFAULT_WEIGHTS}
            onWeights={changeWeights}
            aiOn={aiOn}
            analyzing={analyzingId === selected.id}
            onAnalyze={(force) => analyze(selected, force)}
            onCancelAnalysis={() => ipc.cancelAnalysis(selected.id)}
            changeProfile={changeProfile}
            onProfileChanged={async () => {
              await load();
              if (selectedKey !== null) setDetail(await ipc.getJob(selectedKey).catch(() => null));
            }}
          />
        ) : (
          <EmptyState icon="travel_explore" title="Pick a job">
            Its match breakdown and description show here.
          </EmptyState>
        )}
      </div>
    </section>
  );
}
