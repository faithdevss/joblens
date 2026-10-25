import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

import App from "../App";
import { ToastProvider } from "../components/Toast";
import { fakeBackend, sampleJobs } from "../test/fixtures";

function renderApp() {
  return render(
    <ToastProvider>
      <App />
    </ToastProvider>,
  );
}

async function openPrivacy() {
  renderApp();
  await userEvent.click(screen.getByRole("button", { name: /Settings/ }));
  return screen.findByRole("button", { name: "Use Claude Code" });
}

describe("Privacy & AI", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("is off until the user agrees to what gets sent", async () => {
    const db = fakeBackend(invoke);
    const use = await openPrivacy();
    expect(screen.getByRole("button", { name: "Use Codex" })).toBeDisabled();
    await userEvent.click(use);
    const consent = screen.getByRole("alertdialog");
    expect(consent).toHaveTextContent("Send job and resume text to Anthropic?");
    expect(consent).toHaveTextContent(
      "Your name, email, phone, location and links are never sent.",
    );
    expect(db.ai.provider).toBeNull();

    await userEvent.click(within(consent).getByRole("button", { name: "I agree, turn on AI" }));
    expect(invoke).toHaveBeenCalledWith("set_ai_provider", { provider: "claude", consent: true });
    expect(db.ai.provider).toBe("claude");
    expect(await screen.findByRole("button", { name: "Turn off" })).toBeInTheDocument();
  });

  it("can be cancelled and turned off again", async () => {
    const db = fakeBackend(invoke, {
      ai: { provider: "claude", consentedAt: "2026-10-24T10:00:00Z" },
    });
    renderApp();
    await userEvent.click(screen.getByRole("button", { name: /Settings/ }));
    await userEvent.click(await screen.findByRole("button", { name: "Turn off" }));
    expect(db.ai.provider).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Use Claude Code" }));
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(db.ai.provider).toBeNull();
  });
});

describe("AI check in the feed", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("explains how to turn it on when AI is off", async () => {
    fakeBackend(invoke, { jobs: sampleJobs() });
    renderApp();
    expect(await screen.findByText(/Turn on AI in Settings/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Check with AI" })).not.toBeInTheDocument();
  });

  it("shows requirements with evidence and gaps", async () => {
    fakeBackend(invoke, {
      jobs: sampleJobs(),
      ai: { provider: "claude", consentedAt: "2026-10-24T10:00:00Z" },
    });
    renderApp();
    await userEvent.click(await screen.findByRole("button", { name: "Check with AI" }));
    const asks = await screen.findByRole("region", { name: "What they ask for" });
    expect(within(asks).getByText("“Led the TypeScript migration.”")).toBeInTheDocument();
    expect(within(asks).getByText(/resume line 5/)).toBeInTheDocument();
    const gaps = screen.getByRole("region", { name: "Gaps" });
    expect(within(gaps).getByText("GraphQL")).toBeInTheDocument();
    expect(within(gaps).getByText("Design systems")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("analyze_job", { id: 1, force: false });
    // The list score updates and is marked as AI-checked.
    const row = within(screen.getByRole("list", { name: "Jobs" })).getAllByRole("button")[0];
    expect(row).toHaveTextContent("77");
  });

  it("reports why a check failed", async () => {
    fakeBackend(invoke, {
      jobs: sampleJobs(),
      ai: { provider: "claude", consentedAt: "x" },
      analysis: new Error("Claude Code isn’t signed in. Run `claude` in a terminal to sign in."),
    });
    renderApp();
    await userEvent.click(await screen.findByRole("button", { name: "Check with AI" }));
    expect(await screen.findByRole("status")).toHaveTextContent("isn’t signed in");
    expect(screen.getByRole("button", { name: "Check with AI" })).toBeInTheDocument();
  });

  it("saves adjusted weights", async () => {
    const db = fakeBackend(invoke, { jobs: sampleJobs() });
    renderApp();
    await userEvent.click(await screen.findByRole("button", { name: "Adjust weights" }));
    fireEvent.change(screen.getByRole("slider", { name: /Skills/ }), { target: { value: "50" } });
    await waitFor(() => expect(db.weights.skills).toBe(50));
    expect(screen.getByText("×50")).toBeInTheDocument();
  });
});
