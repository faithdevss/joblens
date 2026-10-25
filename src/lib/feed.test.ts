import { describe, expect, it } from "vitest";
import { job, sampleJobs } from "../test/fixtures";
import {
  applyFilters,
  DEFAULT_FILTERS,
  formatAge,
  meetsMinimum,
  refreshSummary,
  salaryText,
  sourceOptions,
} from "./feed";
import { emptySearch } from "./profile";

const now = new Date("2026-10-24T12:00:00Z");

describe("feed filters", () => {
  it("sorts by score, or by date", () => {
    const jobs = sampleJobs();
    expect(applyFilters(jobs, DEFAULT_FILTERS, emptySearch(), now).map((j) => j.id)).toEqual([
      1, 2,
    ]);
    expect(
      applyFilters(jobs, { ...DEFAULT_FILTERS, sort: "newest" }, emptySearch(), now).map(
        (j) => j.id,
      ),
    ).toEqual([2, 1]);
  });

  it("filters by eligibility, score, salary and source", () => {
    const jobs = sampleJobs();
    const run = (patch: Partial<typeof DEFAULT_FILTERS>) =>
      applyFilters(jobs, { ...DEFAULT_FILTERS, ...patch }, emptySearch(), now).map((j) => j.id);
    expect(run({ eligibility: "restricted" })).toEqual([2]);
    expect(run({ minScore: 60 })).toEqual([1]);
    expect(run({ salary: "stated" })).toEqual([1]);
    expect(run({ source: "remoteok" })).toEqual([2]);
  });

  it("keeps dismissed jobs out unless asked for", () => {
    const jobs = [job({ id: 1, state: "dismissed" }), job({ id: 2, state: "saved" })];
    const run = (show: typeof DEFAULT_FILTERS.show) =>
      applyFilters(jobs, { ...DEFAULT_FILTERS, show }, emptySearch(), now).map((j) => j.id);
    expect(run("active")).toEqual([2]);
    expect(run("saved")).toEqual([2]);
    expect(run("dismissed")).toEqual([1]);
  });

  it("filters by recency using the posted date, or first seen", () => {
    const old = job({ id: 3, postedAt: null, firstSeenAt: "2026-09-01T00:00:00Z" });
    const out = applyFilters(
      [job(), old],
      { ...DEFAULT_FILTERS, maxAgeDays: 7 },
      emptySearch(),
      now,
    );
    expect(out.map((j) => j.id)).toEqual([1]);
  });

  it("only hides salaries known to be under the minimum", () => {
    const search = { ...emptySearch(), minSalary: 70000, currency: "EUR" };
    expect(meetsMinimum(job(), search)).toBe(true);
    expect(
      meetsMinimum(
        job({ salary: { min: 40000, max: 50000, currency: "EUR", period: "year", text: "" } }),
        search,
      ),
    ).toBe(false);
    expect(
      meetsMinimum(
        job({ salary: { min: 5000, max: 6000, currency: "EUR", period: "month", text: "" } }),
        search,
      ),
    ).toBe(true);
    // Other currencies and unknown periods can't be compared, so they stay.
    expect(
      meetsMinimum(
        job({ salary: { min: 40000, max: 50000, currency: "USD", period: "year", text: "" } }),
        search,
      ),
    ).toBe(true);
    expect(meetsMinimum(job({ salary: null }), search)).toBe(true);
  });
});

describe("feed text", () => {
  it("says how old a job is", () => {
    expect(formatAge("2026-10-24T08:00:00Z", now)).toBe("today");
    expect(formatAge("2026-10-23T08:00:00Z", now)).toBe("yesterday");
    expect(formatAge("2026-10-20T08:00:00Z", now)).toBe("4 days ago");
    expect(formatAge("nope", now)).toBe("date not stated");
  });

  it("shows salary as stated, never guessed", () => {
    expect(salaryText(null)).toBe("salary not stated");
    expect(salaryText({ min: 50, max: 75, currency: "USD", period: "hour", text: "$50-$75" })).toBe(
      "$50-$75 / hour",
    );
    expect(
      salaryText({ min: 50, max: 75, currency: "USD", period: "hour", text: "$50-$75 /hour" }),
    ).toBe("$50-$75 /hour");
  });

  it("lists sources found in the feed", () => {
    expect(sourceOptions(sampleJobs()).map((s) => s.name)).toEqual([
      "Remote OK",
      "Remotive",
      "We Work Remotely",
    ]);
  });

  it("summarizes a refresh", () => {
    expect(
      refreshSummary({
        runs: [
          { sourceId: "a", name: "Remotive", outcome: { kind: "ok", fetched: 10, added: 3 } },
          { sourceId: "b", name: "WWR", outcome: { kind: "ok", fetched: 10, added: 1 } },
          { sourceId: "c", name: "Remote OK", outcome: { kind: "failed", error: "Timed out" } },
        ],
      }),
    ).toBe("2 sources updated · 4 new jobs · Remote OK failed");
    expect(
      refreshSummary({
        runs: [
          {
            sourceId: "a",
            name: "Remotive",
            outcome: { kind: "skipped", nextAt: "2026-10-24T15:00:00Z" },
          },
        ],
      }),
    ).toMatch(/^Already up to date\. Next check from /);
    expect(refreshSummary({ runs: [] })).toBe("No sources are turned on.");
  });
});
