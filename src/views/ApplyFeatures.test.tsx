import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const revealItemInDir = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(),
  revealItemInDir: (p: string) => revealItemInDir(p),
}));

import App from "../App";
import { ToastProvider } from "../components/Toast";
import { fakeBackend, importedProfile, job, sampleJobs } from "../test/fixtures";

function renderApp() {
  return render(
    <ToastProvider>
      <App />
    </ToastProvider>,
  );
}

async function openTab(name: string) {
  renderApp();
  const detail = await screen.findByRole("article", { name: /Senior Frontend Engineer/ });
  await userEvent.click(within(detail).getByRole("tab", { name }));
  return detail;
}

describe("Resume gap check", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("lists what's covered, basic checks and the plain-text resume, with no score", async () => {
    fakeBackend(invoke, { jobs: sampleJobs(), profile: importedProfile() });
    const detail = await openTab("Resume gap check");
    const reqs = await within(detail).findByRole("region", { name: "Requirements" });
    expect(reqs).toHaveTextContent("1 of 2 covered by your resume");
    expect(within(reqs).getByText("Not covered")).toBeInTheDocument();
    const checks = within(detail).getByRole("region", { name: "Basic checks" });
    expect(checks).toHaveTextContent("Dates use different formats");
    expect(checks).toHaveTextContent("No overall “ATS score”");
    expect(
      within(detail).getByRole("region", { name: "As hiring software reads it" }),
    ).toHaveTextContent("Senior Frontend Engineer, Brook");
  });

  it("accepts an edit after saving pending profile edits, and can undo it", async () => {
    const db = fakeBackend(invoke, { jobs: sampleJobs(), profile: importedProfile() });
    const detail = await openTab("Resume gap check");
    const edits = await within(detail).findByRole("region", { name: "Suggested edits" });
    await userEvent.click(within(edits).getByRole("button", { name: "Accept" }));
    expect(await within(edits).findByText("Accepted — added to your profile")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("decide_edit", {
      id: 1,
      editId: "skill:typescript",
      action: { type: "accept", text: "TypeScript" },
    });
    // The profile is reloaded after the change.
    expect(invoke.mock.calls.filter(([c]) => c === "get_profile").length).toBeGreaterThan(1);
    expect(within(edits).getByText(/Copy them into your resume file too/)).toBeInTheDocument();

    await userEvent.click(within(edits).getByRole("button", { name: "Undo" }));
    expect(await within(edits).findByRole("button", { name: "Accept" })).toBeInTheDocument();
    expect(db.calls).toEqual(["decide_edit:accept", "decide_edit:undo"]);
  });

  it("lets the user edit a suggestion before accepting", async () => {
    fakeBackend(invoke, { jobs: sampleJobs(), profile: importedProfile() });
    const detail = await openTab("Resume gap check");
    const edits = await within(detail).findByRole("region", { name: "Suggested edits" });
    await userEvent.click(within(edits).getByRole("button", { name: "Edit" }));
    const box = within(edits).getByRole("textbox", { name: "Edit suggestion" });
    await userEvent.clear(box);
    await userEvent.type(box, "TypeScript (strict)");
    await userEvent.click(within(edits).getByRole("button", { name: "Accept" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("decide_edit", {
        id: 1,
        editId: "skill:typescript",
        action: { type: "accept", text: "TypeScript (strict)" },
      }),
    );
    expect(await within(edits).findByText("TypeScript (strict)")).toBeInTheDocument();
  });

  it("offers AI rewording only when AI is on", async () => {
    fakeBackend(invoke, { jobs: sampleJobs() });
    const detail = await openTab("Resume gap check");
    await within(detail).findByRole("region", { name: "Suggested edits" });
    expect(
      within(detail).queryByRole("button", { name: "Suggest rewording with AI" }),
    ).not.toBeInTheDocument();
  });
});

describe("Application writer", () => {
  beforeEach(() => {
    invoke.mockReset();
    revealItemInDir.mockReset();
  });

  it("is locked until the job is saved", async () => {
    fakeBackend(invoke, { jobs: sampleJobs() });
    const detail = await openTab("Write application");
    expect(within(detail).getByText("Save this job to write an application for it.")).toBeVisible();
    await userEvent.click(within(detail).getByRole("button", { name: "Save job" }));
    expect(await within(detail).findByRole("button", { name: "Write draft" })).toBeInTheDocument();
  });

  it("blocks export until unbacked sentences are kept or removed", async () => {
    const db = fakeBackend(invoke, { jobs: [job({ state: "saved" })] });
    const detail = await openTab("Write application");
    // The best-fitting experience is picked to start with.
    const chip = await within(detail).findByRole("button", {
      name: "Led the TypeScript migration. (Senior Frontend Engineer · Brook)",
    });
    expect(chip).toHaveAttribute("aria-pressed", "true");

    await userEvent.click(within(detail).getByRole("button", { name: "Write draft" }));
    expect(invoke).toHaveBeenCalledWith("write_draft", {
      id: 1,
      kind: "cover_letter",
      length: "standard",
      highlights: ["a2"],
      useAi: false,
    });
    const status = await within(detail).findByText(/sentence isn’t backed by your resume/);
    expect(status).toBeInTheDocument();
    expect(within(detail).getByRole("textbox", { name: "Greeting" })).toHaveValue(
      "Dear [hiring manager name],",
    );
    expect(within(detail).getByRole("button", { name: "Download PDF" })).toBeDisabled();
    expect(within(detail).getByText("Resolve highlighted sentences first.")).toBeInTheDocument();

    await userEvent.click(within(detail).getByRole("button", { name: "It’s true — keep" }));
    expect(
      await within(detail).findByText(
        /every claim is backed by your resume. Fill in the 1 marked blank/,
      ),
    ).toBeInTheDocument();
    const pdf = within(detail).getByRole("button", { name: "Download PDF" });
    expect(pdf).toBeEnabled();
    await userEvent.click(pdf);
    expect(
      await screen.findByText("Saved Cover letter - Lumen Health.pdf to Downloads."),
    ).toBeInTheDocument();
    await userEvent.click(within(detail).getByRole("button", { name: "Show in folder" }));
    expect(revealItemInDir).toHaveBeenCalledWith(
      "/Users/sam/Downloads/Cover letter - Lumen Health.pdf",
    );
    expect(db.calls).toContain("export_draft:pdf");
  });

  it("removes a flagged sentence", async () => {
    fakeBackend(invoke, { jobs: [job({ state: "saved" })] });
    const detail = await openTab("Write application");
    await userEvent.click(await within(detail).findByRole("button", { name: "Write draft" }));
    await userEvent.click(await within(detail).findByRole("button", { name: "Remove" }));
    await waitFor(() =>
      expect(within(detail).queryByText("I mentored ten engineers.")).not.toBeInTheDocument(),
    );
    expect(within(detail).getByRole("button", { name: "Copy" })).toBeEnabled();
  });

  it("writes with AI when it's on", async () => {
    fakeBackend(invoke, {
      jobs: [job({ state: "saved" })],
      ai: { provider: "claude", consentedAt: "2026-10-24T10:00:00Z" },
    });
    const detail = await openTab("Write application");
    await userEvent.click(await within(detail).findByRole("button", { name: "Write with AI" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("write_draft", expect.objectContaining({ useAi: true })),
    );
  });
});

describe("Job detail tabs", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("move with the arrow keys", async () => {
    fakeBackend(invoke, { jobs: sampleJobs() });
    renderApp();
    const detail = await screen.findByRole("article", { name: /Senior Frontend Engineer/ });
    const first = within(detail).getByRole("tab", { name: "Why it matches" });
    first.focus();
    await userEvent.keyboard("{ArrowRight}");
    const gaps = within(detail).getByRole("tab", { name: "Resume gap check" });
    expect(gaps).toHaveAttribute("aria-selected", "true");
    expect(gaps).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(within(detail).getByRole("tab", { name: /Interview topics/ })).toHaveFocus();
    await userEvent.keyboard("{ArrowRight}");
    expect(first).toHaveFocus();
  });
});

describe("Interview topics", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("is locked until the job is saved", async () => {
    fakeBackend(invoke, { jobs: sampleJobs() });
    const detail = await openTab("Interview topics");
    expect(within(detail).getByText("Save this job to prepare for its interview.")).toBeVisible();
  });

  it("lists stories, gaps and questions, and remembers what's prepared", async () => {
    const db = fakeBackend(invoke, { jobs: [job({ state: "saved" })] });
    const detail = await openTab("Interview topics");
    expect(
      await within(detail).findByText(/Your story: “Led the TypeScript migration.”/),
    ).toBeVisible();
    expect(within(detail).getByText("Not on your resume")).toBeVisible();
    expect(
      within(detail).getByText("What does success look like in the first 90 days?"),
    ).toBeVisible();
    await userEvent.click(within(detail).getByRole("checkbox", { name: "Prepared: React" }));
    await waitFor(() => expect(db.calls).toContain("set_topic_prepared:req:react:true"));
    expect(within(detail).getByRole("checkbox", { name: "Prepared: React" })).toBeChecked();
  });
});

describe("Still open check", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("says whether the posting still loads", async () => {
    fakeBackend(invoke, {
      jobs: sampleJobs(),
      openCheck: {
        status: "closed",
        note: "The page says it's no longer accepting applications.",
        checkedAt: "2026-10-25T10:00:00Z",
      },
    });
    renderApp();
    const detail = await screen.findByRole("article", { name: /Senior Frontend Engineer/ });
    await userEvent.click(within(detail).getByRole("button", { name: /Still open\?/ }));
    expect(await within(detail).findByText(/Looks closed\./)).toHaveAttribute(
      "data-status",
      "closed",
    );
  });
});
