import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FeedUpdated } from "../bindings/FeedUpdated";

const invoke = vi.fn();
const events = vi.hoisted(() => ({ handler: null as ((u: FeedUpdated) => void) | null }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("../lib/events", () => ({
  onFeedUpdated: (h: (u: FeedUpdated) => void) => {
    events.handler = h;
    return () => {
      events.handler = null;
    };
  },
}));

import App from "../App";
import { ToastProvider } from "../components/Toast";
import { emptySearch } from "../lib/profile";
import { fakeBackend, sampleJobs } from "../test/fixtures";

function renderApp() {
  return render(
    <ToastProvider>
      <App />
    </ToastProvider>,
  );
}

async function openSettings() {
  renderApp();
  await userEvent.click(screen.getByRole("button", { name: /Settings/ }));
}

const feedLoads = () => invoke.mock.calls.filter(([cmd]) => cmd === "get_feed").length;

describe("Saved searches", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("starts a new search from a copy and switches the feed to it", async () => {
    const db = fakeBackend(invoke, { search: { ...emptySearch(), titles: ["Designer"] } });
    await openSettings();
    await userEvent.click(await screen.findByRole("button", { name: "New search" }));
    expect(screen.getByText(/starts as a copy of “My search”/)).toBeVisible();
    await userEvent.type(screen.getByLabelText("New search name"), "Staff roles{Enter}");
    expect(await screen.findByText(/Started “Staff roles”/)).toBeInTheDocument();
    expect(db.searches.map((s) => [s.name, s.active])).toEqual([
      ["My search", false],
      ["Staff roles", true],
    ]);
    expect(screen.getByLabelText("Saved search")).toHaveDisplayValue("Staff roles");

    await userEvent.click(screen.getByRole("button", { name: /Job feed/ }));
    const picker = await screen.findByRole("combobox", { name: "Saved search" });
    expect(picker).toHaveDisplayValue("Staff roles");
    const before = feedLoads();
    await userEvent.selectOptions(picker, "My search");
    await waitFor(() => expect(feedLoads()).toBeGreaterThan(before));
    expect(db.searches.find((s) => s.active)?.name).toBe("My search");
    expect(db.search.titles).toEqual(["Designer"]);
  });

  it("hides the feed picker with only one search", async () => {
    fakeBackend(invoke);
    renderApp();
    await screen.findByRole("heading", { name: "Job feed" });
    expect(screen.queryByRole("combobox", { name: "Saved search" })).not.toBeInTheDocument();
  });

  it("renames, turns alerts off and deletes after confirming", async () => {
    const db = fakeBackend(invoke, {
      searches: [
        { id: 1, name: "My search", active: true, alerts: true },
        { id: 2, name: "Europe", active: false, alerts: true },
      ],
    });
    await openSettings();
    await userEvent.click(await screen.findByRole("button", { name: "Rename" }));
    const name = screen.getByLabelText("Search name");
    await userEvent.clear(name);
    await userEvent.type(name, "Frontend{Enter}");
    await waitFor(() => expect(db.searches[0].name).toBe("Frontend"));

    await userEvent.click(screen.getByRole("checkbox", { name: /great-match alerts/ }));
    await waitFor(() => expect(db.searches[0].alerts).toBe(false));

    await userEvent.click(screen.getByRole("button", { name: "Delete" }));
    const dialog = screen.getByRole("alertdialog", { name: "Delete “Frontend”?" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete search" }));
    await waitFor(() => expect(db.searches.map((s) => s.name)).toEqual(["Europe"]));
    expect(screen.getByRole("button", { name: "Delete" })).toBeDisabled();
  });
});

describe("Updates & alerts", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("turns on alerts, sends a test notification and changes the interval", async () => {
    const db = fakeBackend(invoke);
    await openSettings();
    const alerts = await screen.findByRole("switch", { name: "Alert me about great matches" });
    expect(screen.getByLabelText("Alert at match score")).toBeDisabled();
    await userEvent.click(alerts);
    await waitFor(() => expect(db.alerts.notify).toBe(true));
    expect(db.calls).toContain("send_test_alert");
    expect(alerts).toHaveAttribute("aria-checked", "true");

    await userEvent.selectOptions(screen.getByLabelText("Alert at match score"), "90");
    await userEvent.selectOptions(screen.getByLabelText("Refresh interval"), "6");
    await waitFor(() => expect(db.alerts).toMatchObject({ minScore: 90, everyHours: 6 }));

    await userEvent.click(screen.getByRole("switch", { name: "Refresh in the background" }));
    expect(await screen.findByText(/no alerts will be sent/)).toBeVisible();
  });

  it("reloads the feed after a background refresh and mentions great matches", async () => {
    fakeBackend(invoke, { jobs: sampleJobs() });
    renderApp();
    await screen.findByRole("article", { name: /Senior Frontend Engineer/ });
    const before = feedLoads();
    act(() =>
      events.handler?.({
        alerts: [
          { jobId: 7, title: "Staff Engineer", company: "Brook", score: 92, search: "My search" },
        ],
      }),
    );
    expect(
      await screen.findByText("New great match: Staff Engineer at Brook (92%)"),
    ).toBeInTheDocument();
    await waitFor(() => expect(feedLoads()).toBeGreaterThan(before));
  });
});
