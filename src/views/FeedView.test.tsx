import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const openUrl = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: (url: string) => openUrl(url) }));

import App from "../App";
import { ToastProvider } from "../components/Toast";
import { fakeBackend, job, sampleJobs } from "../test/fixtures";

function renderApp() {
  return render(
    <ToastProvider>
      <App />
    </ToastProvider>,
  );
}

describe("Job feed", () => {
  beforeEach(() => {
    invoke.mockReset();
    openUrl.mockReset();
  });

  it("lists jobs best first and shows the top one", async () => {
    fakeBackend(invoke, { jobs: sampleJobs() });
    renderApp();
    const list = await screen.findByRole("list", { name: "Jobs" });
    const rows = within(list).getAllByRole("button");
    expect(rows[0]).toHaveTextContent("Senior Frontend Engineer");
    expect(rows[1]).toHaveTextContent("Backend Engineer");
    expect(rows[1]).toHaveTextContent("Restricted");

    const detail = await screen.findByRole("article", {
      name: /Senior Frontend Engineer at Lumen Health/,
    });
    expect(within(detail).getByText("Open worldwide (“Worldwide”).")).toBeInTheDocument();
    expect(await within(detail).findByText(/only ~90 words/)).toBeInTheDocument();
    expect(within(detail).getByText("GraphQL")).toBeInTheDocument();
  });

  it("shows long lists 50 at a time", async () => {
    const jobs = Array.from({ length: 120 }, (_, i) =>
      job({ id: i + 1, title: `Role ${i + 1}`, score: 90 - (i % 50) }),
    );
    fakeBackend(invoke, { jobs });
    renderApp();
    const list = await screen.findByRole("list", { name: "Jobs" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(51);
    await userEvent.click(within(list).getByRole("button", { name: /Show 50 more · 70 left/ }));
    await userEvent.click(within(list).getByRole("button", { name: /Show 20 more · 20 left/ }));
    expect(within(list).getAllByRole("listitem")).toHaveLength(120);
    expect(within(list).queryByRole("button", { name: /more/ })).not.toBeInTheDocument();
  });

  it("shows where else a job is listed and opens the original", async () => {
    fakeBackend(invoke, { jobs: sampleJobs() });
    renderApp();
    await userEvent.click(await screen.findByRole("button", { name: /Backend Engineer/ }));
    const detail = await screen.findByRole("article", { name: /Backend Engineer at Acme/ });
    expect(within(detail).getByText(/also listed on Remote OK/)).toBeInTheDocument();
    expect(within(detail).getByText(/salary not stated/)).toBeInTheDocument();
    await userEvent.click(within(detail).getByRole("button", { name: "Open original" }));
    expect(openUrl).toHaveBeenCalledWith("https://weworkremotely.com/2");
  });

  it("filters by eligibility", async () => {
    fakeBackend(invoke, { jobs: sampleJobs() });
    renderApp();
    await screen.findByRole("list", { name: "Jobs" });
    await userEvent.click(screen.getByRole("button", { name: "Eligible" }));
    expect(within(screen.getByRole("list", { name: "Jobs" })).getAllByRole("button")).toHaveLength(
      1,
    );
    await userEvent.click(screen.getByRole("button", { name: "Not stated" }));
    expect(screen.getByText(/No jobs match these filters/)).toBeInTheDocument();
  });

  it("saves and dismisses jobs", async () => {
    const db = fakeBackend(invoke, { jobs: sampleJobs() });
    renderApp();
    const detail = await screen.findByRole("article", { name: /Senior Frontend Engineer/ });
    await userEvent.click(within(detail).getByRole("button", { name: "Save" }));
    expect(db.jobs[0].state).toBe("saved");
    expect(await within(detail).findByRole("button", { name: "Saved" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );

    await userEvent.click(within(detail).getByRole("button", { name: "Dismiss" }));
    expect(db.jobs[0].state).toBe("dismissed");
    expect(await screen.findByRole("article", { name: /Backend Engineer/ })).toBeInTheDocument();
    expect(within(screen.getByRole("list", { name: "Jobs" })).getAllByRole("button")).toHaveLength(
      1,
    );
  });

  it("refreshes and reports failures", async () => {
    fakeBackend(invoke, {
      jobs: sampleJobs(),
      refreshReport: {
        runs: [
          { sourceId: "remotive", name: "Remotive", outcome: { kind: "ok", fetched: 5, added: 2 } },
          {
            sourceId: "remoteok",
            name: "Remote OK",
            outcome: { kind: "failed", error: "Timed out" },
          },
        ],
      },
    });
    renderApp();
    await userEvent.click(await screen.findByRole("button", { name: "Refresh" }));
    expect(await screen.findByRole("status")).toHaveTextContent(
      "1 source updated · 2 new jobs · Remote OK failed",
    );
    expect(invoke).toHaveBeenCalledWith("refresh_sources", { only: null });
  });

  it("fetches on first open", async () => {
    fakeBackend(invoke, { lastUpdatedAt: null });
    renderApp();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("refresh_sources", { only: null }));
  });
});

describe("Job sources", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  async function openSettings() {
    renderApp();
    await userEvent.click(screen.getByRole("button", { name: /Settings/ }));
    return screen.findByRole("switch", { name: "Remotive on" });
  }

  it("turns a source off", async () => {
    const db = fakeBackend(invoke);
    const toggle = await openSettings();
    expect(toggle).toHaveAttribute("aria-checked", "true");
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-checked", "false");
    expect(db.sources[0].enabled).toBe(false);
  });

  it("shows why a source failed", async () => {
    const sources = (await import("../test/fixtures")).boardSources();
    sources[1].lastError = "The site asked us to slow down (429). Try again later.";
    fakeBackend(invoke, { sources });
    await openSettings();
    expect(screen.getByText(/asked us to slow down/)).toBeInTheDocument();
  });

  it("adds and removes a watchlist company", async () => {
    const db = fakeBackend(invoke);
    await openSettings();
    await userEvent.type(screen.getByRole("textbox", { name: /Company name/ }), "GitLab");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Added GitLab · 12 remote jobs");
    expect(await screen.findByText("Greenhouse · gitlab")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(screen.queryByText("Greenhouse · gitlab")).not.toBeInTheDocument());
    expect(db.sources.map((s) => s.id)).not.toContain("greenhouse:gitlab");
  });

  it("explains when a company board can't be found", async () => {
    fakeBackend(invoke, {
      watchlistResult: new Error("Couldn’t find a Greenhouse, Lever or Ashby board for “Nope”."),
    });
    await openSettings();
    await userEvent.type(screen.getByRole("textbox", { name: /Company name/ }), "Nope");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Couldn’t find a Greenhouse");
  });

  it("saves a job found elsewhere", async () => {
    const db = fakeBackend(invoke);
    await openSettings();
    await userEvent.type(screen.getByRole("textbox", { name: "Job title" }), "Designer");
    await userEvent.type(screen.getByRole("textbox", { name: "Company" }), "Acme");
    await userEvent.type(
      screen.getByRole("textbox", { name: "Job description" }),
      "Design things.",
    );
    await userEvent.click(screen.getByRole("button", { name: "Save to my feed" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Saved to your feed.");
    expect(db.manualJobs).toEqual([
      { url: "", title: "Designer", company: "Acme", location: "", description: "Design things." },
    ]);
  });
});
