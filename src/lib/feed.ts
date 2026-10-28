// Feed filters, sorting and display text. The score and eligibility come from Rust;
// everything here only narrows and orders what the feed already returned.
import type { EligibilityStatus } from "../bindings/EligibilityStatus";
import type { JobSummary } from "../bindings/JobSummary";
import type { RefreshReport } from "../bindings/RefreshReport";
import type { Salary } from "../bindings/Salary";
import type { SearchSettings } from "../bindings/SearchSettings";

export type FeedSort = "best" | "newest";
export type FeedShow = "active" | "saved" | "dismissed";
export type SalaryFilter = "any" | "stated" | "minimum";

export type FeedFilters = {
  show: FeedShow;
  sort: FeedSort;
  eligibility: EligibilityStatus | "all";
  minScore: number;
  /** Days; 0 = any time. */
  maxAgeDays: number;
  salary: SalaryFilter;
  /** Source id, or "" for all. */
  source: string;
};

export const DEFAULT_FILTERS: FeedFilters = {
  show: "active",
  sort: "best",
  eligibility: "all",
  minScore: 0,
  maxAgeDays: 0,
  salary: "any",
  source: "",
};

export const ELIGIBILITY_LABELS: Record<EligibilityStatus, { label: string; icon: string }> = {
  eligible: { label: "Eligible", icon: "check_circle" },
  restricted: { label: "Restricted", icon: "block" },
  not_stated: { label: "Not stated", icon: "help" },
};

const DAY_MS = 86_400_000;

/** When the job was posted, or when we first saw it if the source doesn't say. */
export function jobDate(job: JobSummary): string {
  return job.postedAt ?? job.firstSeenAt;
}

export function ageDays(iso: string, now = new Date()): number | null {
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return null;
  return Math.max(0, Math.floor((now.getTime() - t) / DAY_MS));
}

/** "today", "yesterday", "3 days ago", "Sep 14". */
export function formatAge(iso: string, now = new Date()): string {
  const days = ageDays(iso, now);
  if (days === null) return "date not stated";
  if (days === 0) return "today";
  if (days === 1) return "yesterday";
  if (days < 30) return `${days} days ago`;
  return new Date(iso).toLocaleDateString("en", { month: "short", day: "numeric" });
}

/** Yearly amount, when the salary can be compared at all. */
function yearlyMax(s: Salary): number | null {
  const top = s.max ?? s.min;
  if (top === null) return null;
  if (s.period === "year") return top;
  if (s.period === "month") return top * 12;
  return null;
}

/** False only when the posting's salary is known and below the minimum. Unknown stays in. */
export function meetsMinimum(job: JobSummary, search: SearchSettings): boolean {
  const s = job.salary;
  if (!search.minSalary || !s) return true;
  if (!s.currency || !search.currency || s.currency !== search.currency) return true;
  const yearly = yearlyMax(s);
  return yearly === null || yearly >= search.minSalary;
}

export function salaryText(s: Salary | null): string {
  if (!s?.text) return "salary not stated";
  const per = s.period === "hour" ? " / hour" : s.period === "month" ? " / month" : "";
  return /hour|month|year|annual|\/\s*(hr|mo|yr)/i.test(s.text) ? s.text : `${s.text}${per}`;
}

export function applyFilters(
  jobs: JobSummary[],
  f: FeedFilters,
  search: SearchSettings,
  now = new Date(),
): JobSummary[] {
  const list = jobs.filter((j) => {
    if (f.show === "active" && j.state === "dismissed") return false;
    if (f.show === "saved" && j.state !== "saved") return false;
    if (f.show === "dismissed" && j.state !== "dismissed") return false;
    if (f.eligibility !== "all" && j.eligibility.status !== f.eligibility) return false;
    if (j.score < f.minScore) return false;
    if (f.maxAgeDays > 0) {
      const days = ageDays(jobDate(j), now);
      if (days !== null && days > f.maxAgeDays) return false;
    }
    if (f.salary === "stated" && !j.salary) return false;
    if (f.salary === "minimum" && !meetsMinimum(j, search)) return false;
    if (f.source && !j.listings.some((l) => l.sourceId === f.source)) return false;
    return true;
  });
  const newest = (a: JobSummary, b: JobSummary) => jobDate(b).localeCompare(jobDate(a));
  return list.sort(f.sort === "best" ? (a, b) => b.score - a.score || newest(a, b) : newest);
}

/** Sources that list at least one job, for the source filter. */
export function sourceOptions(jobs: JobSummary[]): Array<{ id: string; name: string }> {
  const seen = new Map<string, string>();
  for (const j of jobs) for (const l of j.listings) seen.set(l.sourceId, l.sourceName);
  return [...seen].map(([id, name]) => ({ id, name })).sort((a, b) => a.name.localeCompare(b.name));
}

/** "14:05". */
export function formatClock(iso: string): string {
  return new Date(iso).toLocaleTimeString("en", { hour: "2-digit", minute: "2-digit" });
}

/** One line for the toast after a refresh. */
export function refreshSummary(report: RefreshReport): string {
  const ok = report.runs.filter((r) => r.outcome.kind === "ok");
  const failed = report.runs.filter((r) => r.outcome.kind === "failed");
  const skipped = report.runs.filter((r) => r.outcome.kind === "skipped");
  const added = ok.reduce((n, r) => n + (r.outcome.kind === "ok" ? r.outcome.added : 0), 0);
  const parts: string[] = [];
  if (ok.length) {
    parts.push(
      `${ok.length} source${ok.length === 1 ? "" : "s"} updated · ${added} new job${added === 1 ? "" : "s"}`,
    );
  }
  if (failed.length) parts.push(`${failed.map((r) => r.name).join(", ")} failed`);
  if (!ok.length && !failed.length && skipped.length) {
    const next = skipped
      .map((r) => (r.outcome.kind === "skipped" ? r.outcome.nextAt : ""))
      .sort()[0];
    parts.push(`Already up to date. Next check from ${formatClock(next)}.`);
  }
  return parts.join(" · ") || "No sources are turned on.";
}

export type DescriptionBlock =
  | { kind: "heading"; text: string }
  | { kind: "paragraph"; text: string }
  | { kind: "list"; items: string[] };

const BULLET = /^[•·▪●◦‣*\-–]\s*/;

/**
 * Splits plain-text job descriptions into headings, paragraphs and lists. Postings
 * arrive as flattened HTML, so short unpunctuated lines are treated as headings.
 */
export function descriptionBlocks(text: string): DescriptionBlock[] {
  const lines = text
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
  const blocks: DescriptionBlock[] = [];
  for (let i = 0; i < lines.length; i++) {
    let line = lines[i];
    // A bullet left alone on its line belongs to the text after it.
    if (BULLET.test(line) && line.replace(BULLET, "") === "" && i + 1 < lines.length) {
      line = `• ${lines[++i]}`;
    }
    if (BULLET.test(line) && line.replace(BULLET, "") !== "") {
      const item = line.replace(BULLET, "");
      const last = blocks[blocks.length - 1];
      if (last?.kind === "list") last.items.push(item);
      else blocks.push({ kind: "list", items: [item] });
    } else if (isHeading(line) && i + 1 < lines.length) {
      blocks.push({ kind: "heading", text: line.replace(/:$/, "") });
    } else {
      blocks.push({ kind: "paragraph", text: line });
    }
  }
  return blocks;
}

function isHeading(line: string): boolean {
  return (
    line.length <= 60 &&
    line.split(/\s+/).length <= 8 &&
    (/:$/.test(line) || !/[.!?,;]$/.test(line))
  );
}
