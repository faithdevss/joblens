import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const revealItemInDir = vi.fn((_path: string) => Promise.resolve());
const reloadApp = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(),
  revealItemInDir: (p: string) => revealItemInDir(p),
}));
vi.mock("../lib/reload", () => ({ reloadApp: () => reloadApp() }));

import App from "../App";
import { ToastProvider } from "../components/Toast";
import { formatDay, todayIso } from "../lib/format";
import { application, fakeBackend, sampleJobs } from "../test/fixtures";
import { appStats, dueToday } from "./ApplicationsView";

function renderApp() {
  return render(
    <ToastProvider>
      <App />
    </ToastProvider>,
  );
}

async function openApplications() {
  renderApp();
  await userEvent.click(screen.getByRole("button", { name: /Applications/ }));
}

describe("Application tracker", () => {
  beforeEach(() => {
    invoke.mockReset();
    revealItemInDir.mockClear();
  });

  it("finds follow-ups due today or overdue, for active applications only", () => {
    const apps = [
      application({ jobId: 1, followUpAt: "2026-10-25" }),
      application({ jobId: 2, followUpAt: "2026-10-20" }),
      application({ jobId: 3, followUpAt: "2026-10-26" }),
      application({ jobId: 4, followUpAt: "2026-10-20", status: "rejected" }),
      application({ jobId: 5, followUpAt: null }),
    ];
    expect(dueToday(apps, "2026-10-25").map((a) => a.jobId)).toEqual([1, 2]);
  });

  it("counts applications per week and the response rate", () => {
    const apps = [
      application({ jobId: 1, status: "saved", appliedAt: null }),
      application({ jobId: 2, appliedAt: "2026-10-24" }),
      application({ jobId: 3, appliedAt: "2026-10-15", status: "interviewing" }),
      application({ jobId: 4, appliedAt: "2026-10-01", status: "rejected" }),
      application({ jobId: 5, appliedAt: "2026-09-01" }),
    ];
    expect(appStats(apps, "2026-10-25")).toEqual({
      sent: 4,
      thisWeek: 1,
      perWeek: 0.8,
      responseRate: 50,
      interviews: 1,
    });
    expect(appStats([], "2026-10-25").responseRate).toBeNull();
  });

  it("moves applications between board columns", async () => {
    const db = fakeBackend(invoke, {
      applications: [
        application({ jobId: 1 }),
        application({ jobId: 2, status: "saved", appliedAt: null, title: "Staff Engineer" }),
      ],
    });
    await openApplications();
    await userEvent.click(await screen.findByRole("button", { name: "Board" }));
    const applied = screen.getByRole("region", { name: "Applied (1)" });
    await userEvent.selectOptions(
      within(applied).getByRole("combobox", {
        name: "Move Senior Frontend Engineer at Lumen Health",
      }),
      "interviewing",
    );
    expect(await screen.findByRole("region", { name: "Interviewing (1)" })).toBeInTheDocument();
    expect(db.applications.find((a) => a.jobId === 1)?.status).toBe("interviewing");
    expect(screen.getByRole("region", { name: "Saved (1)" })).toBeInTheDocument();
  });

  it("marks a job applied from the feed", async () => {
    const db = fakeBackend(invoke, { jobs: sampleJobs() });
    renderApp();
    const detail = await screen.findByRole("article", { name: /Senior Frontend Engineer/ });
    await userEvent.click(within(detail).getByRole("button", { name: "Mark applied" }));
    expect(await screen.findByText("Added to Applications · follow up Nov 1")).toBeInTheDocument();
    expect(db.applications).toHaveLength(1);
    expect(within(detail).getByRole("button", { name: "Saved" })).toBeInTheDocument();
  });

  it("shows the follow-up strip and logs a follow-up", async () => {
    const db = fakeBackend(invoke, {
      applications: [
        application({ followUpAt: todayIso() }),
        application({
          jobId: 2,
          title: "Backend Engineer",
          company: "Acme",
          status: "saved",
          appliedAt: null,
          followUpAt: null,
          letter: null,
          resumeVersion: null,
        }),
      ],
    });
    await openApplications();
    expect(await screen.findByText("2 tracked · 1 active · 1 follow-up due")).toBeInTheDocument();
    const strip = screen.getByRole("region", { name: "Follow up today" });
    expect(strip).toHaveTextContent("Lumen Health");
    await userEvent.click(within(strip).getByRole("button", { name: "Mark followed up" }));
    await waitFor(() =>
      expect(screen.queryByRole("region", { name: "Follow up today" })).not.toBeInTheDocument(),
    );
    expect(db.applications[0].notes).toBe("Followed up Oct 25.");
    const list = screen.getByRole("list", { name: "Applications" });
    expect(list).toHaveTextContent("cv.pdf · read Oct 17 · Cover letter · Oct 18");
    expect(list).toHaveTextContent(formatDay("2026-11-01"));
  });

  it("changes status and edits details", async () => {
    const db = fakeBackend(invoke, { applications: [application()] });
    await openApplications();
    const status = await screen.findByRole("combobox", {
      name: "Status for Senior Frontend Engineer at Lumen Health",
    });
    await userEvent.selectOptions(status, "interviewing");
    await waitFor(() => expect(db.applications[0].status).toBe("interviewing"));

    await userEvent.click(screen.getByRole("button", { name: /Senior Frontend Engineer/ }));
    await userEvent.type(screen.getByRole("textbox", { name: "Notes" }), "Panel on Friday.");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(db.applications[0].notes).toBe("Panel on Friday."));
  });

  it("exports a csv", async () => {
    const db = fakeBackend(invoke, { applications: [application()] });
    await openApplications();
    await userEvent.click(await screen.findByRole("button", { name: "Export .csv" }));
    expect(
      await screen.findByText("Saved JobLens applications 2026-10-25.csv to Downloads."),
    ).toBeInTheDocument();
    expect(db.calls).toContain("export_applications_csv");
  });
});

describe("Your data", () => {
  beforeEach(() => {
    invoke.mockReset();
    reloadApp.mockReset();
  });

  async function openData() {
    renderApp();
    await userEvent.click(screen.getByRole("button", { name: /Settings/ }));
    return screen.findByText(/Your resume is read on this computer/);
  }

  it("says what stays local and what AI sends", async () => {
    fakeBackend(invoke, { ai: { provider: "claude", consentedAt: "2026-10-24T10:00:00Z" } });
    await openData();
    expect(
      screen.getByText(/AI is on: when you ask, job text and your resume/),
    ).toBeInTheDocument();
    expect(screen.getAllByText("AI on").length).toBeGreaterThan(0);
  });

  it("backs up to one file", async () => {
    const db = fakeBackend(invoke);
    await openData();
    await userEvent.click(screen.getByRole("button", { name: "Back up" }));
    expect(
      await screen.findByText("Saved JobLens backup 2026-10-25.zip to Downloads."),
    ).toBeInTheDocument();
    expect(db.calls).toEqual(["create_backup"]);
  });

  it("deletes everything only after a second confirmation", async () => {
    const db = fakeBackend(invoke);
    await openData();
    await userEvent.click(screen.getByRole("button", { name: "Delete…" }));
    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toHaveTextContent("This can’t be undone.");
    expect(db.calls).toEqual([]);
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Delete…" }));
    await userEvent.click(screen.getByRole("button", { name: "Delete everything" }));
    await waitFor(() => expect(reloadApp).toHaveBeenCalled());
    expect(db.calls).toEqual(["delete_all_data:true"]);
  });

  it("restores a backup after confirming", async () => {
    const db = fakeBackend(invoke);
    await openData();
    const file = new File(["zip"], "JobLens backup.zip", { type: "application/zip" });
    await userEvent.upload(screen.getByLabelText("Backup file"), file);
    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toHaveTextContent("Replace everything with “JobLens backup.zip”?");
    await userEvent.click(within(dialog).getByRole("button", { name: "Restore backup" }));
    await waitFor(() => expect(reloadApp).toHaveBeenCalled());
    expect(db.calls).toEqual(["restore_backup"]);
  });
});
