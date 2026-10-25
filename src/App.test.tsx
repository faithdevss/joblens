import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

import App from "./App";
import { ToastProvider } from "./components/Toast";
import { fakeBackend, importedProfile } from "./test/fixtures";

function renderApp() {
  return render(
    <ToastProvider>
      <App />
    </ToastProvider>,
  );
}

function resumeFile(name = "sam-rivera.pdf") {
  return new File(["%PDF-1.5 fake"], name, { type: "application/pdf" });
}

describe("App shell", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("opens on the job feed", async () => {
    fakeBackend(invoke);
    renderApp();
    expect(await screen.findByRole("heading", { name: "Job feed" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Job feed/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(invoke).toHaveBeenCalledWith("app_info");
  });

  it("navigates between views", async () => {
    fakeBackend(invoke);
    renderApp();
    await userEvent.click(screen.getByRole("button", { name: /Applications/ }));
    expect(screen.getByRole("heading", { name: "Applications" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Resume/ }));
    expect(screen.getByRole("heading", { name: "Resume" })).toBeInTheDocument();
  });

  it("shows where data is stored in settings", async () => {
    fakeBackend(invoke);
    renderApp();
    await userEvent.click(screen.getByRole("button", { name: /Settings/ }));
    expect(await screen.findByText("/data")).toBeInTheDocument();
    expect(screen.getByText("2.0 KB")).toBeInTheDocument();
  });

  it("shows an error toast when data can't be opened", async () => {
    invoke.mockRejectedValue("database error: locked");
    renderApp();
    const toast = await screen.findByRole("status");
    expect(toast).toHaveTextContent("database error: locked");
  });
});

describe("Quick setup", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("opens on first run and can be skipped", async () => {
    const db = fakeBackend(invoke, { setup: "pending" });
    renderApp();
    expect(
      await screen.findByRole("heading", { name: "Start with your resume" }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Skip for now/ }));
    expect(await screen.findByRole("heading", { name: "Job feed" })).toBeInTheDocument();
    expect(db.setup).toBe("skipped");
  });

  it("goes from resume to a saved search", async () => {
    const db = fakeBackend(invoke, { setup: "pending" });
    renderApp();
    await screen.findByRole("heading", { name: "Start with your resume" });
    expect(screen.getByRole("button", { name: /Continue/ })).toBeDisabled();

    await userEvent.upload(screen.getByTestId("resume-file"), resumeFile());
    expect(await screen.findByText("sam-rivera.pdf")).toBeInTheDocument();
    expect(screen.getByText("2 roles")).toBeInTheDocument();
    expect(screen.getByText("1 thing to check")).toBeInTheDocument();
    const call = invoke.mock.calls.find(([cmd]) => cmd === "import_resume");
    expect(call?.[1]).toBeInstanceOf(Uint8Array);
    expect(call?.[2]).toEqual({ headers: { "x-file-name": "sam-rivera.pdf" } });

    await userEvent.click(screen.getByRole("button", { name: /Continue/ }));
    expect(screen.getByRole("heading", { name: "Check what we found" })).toBeInTheDocument();
    expect(screen.getByDisplayValue("Brook")).toBeInTheDocument();
    // Year-only dates next to month dates get flagged; the user can confirm them.
    const flag = screen.getByRole("note");
    expect(flag).toHaveTextContent("different format");
    await userEvent.click(within(flag).getByRole("button", { name: "Looks right" }));
    expect(screen.queryByRole("note")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /Continue/ }));
    expect(screen.getByRole("heading", { name: "What are you looking for?" })).toBeInTheDocument();
    // Prefilled from the profile.
    expect(screen.getByRole("button", { name: "Remove Senior Frontend Engineer" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Senior" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByLabelText("Your country")).toHaveValue("PT");

    await userEvent.type(screen.getByLabelText("Must-have keywords"), "React{Enter}");
    await userEvent.click(screen.getByRole("button", { name: /Start searching/ }));

    expect(await screen.findByRole("heading", { name: "Job feed" })).toBeInTheDocument();
    expect(db.setup).toBe("done");
    expect(db.search.mustHave).toEqual(["React"]);
    expect(db.search.country).toBe("PT");
    expect(db.profile?.profile.roles[1].datesConfirmed).toBe(true);
  });

  it("explains why a resume couldn't be read", async () => {
    fakeBackend(invoke, {
      setup: "pending",
      importResult: new Error("This PDF has almost no text we can read. It looks scanned"),
    });
    renderApp();
    await screen.findByRole("heading", { name: "Start with your resume" });
    await userEvent.upload(screen.getByTestId("resume-file"), resumeFile("scan.pdf"));
    expect(await screen.findByRole("alert")).toHaveTextContent("looks scanned");
    expect(screen.getByRole("button", { name: /Continue/ })).toBeDisabled();
  });

  it("can be filled in by hand without a resume", async () => {
    const db = fakeBackend(invoke, { setup: "pending" });
    renderApp();
    await userEvent.click(
      await screen.findByRole("button", { name: /Fill in your profile by hand/ }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Add role" }));
    await userEvent.type(screen.getByLabelText("Role"), "Designer");
    await userEvent.click(screen.getByRole("button", { name: /Continue/ }));
    await userEvent.click(screen.getByRole("button", { name: /Start searching/ }));
    await screen.findByRole("heading", { name: "Job feed" });
    expect(db.profile?.profile.roles[0].title).toBe("Designer");
    expect(db.search.titles).toEqual(["Designer"]);
  });

  it("can be run again from settings", async () => {
    fakeBackend(invoke, { profile: importedProfile() });
    renderApp();
    await userEvent.click(screen.getByRole("button", { name: /Settings/ }));
    await userEvent.click(await screen.findByRole("button", { name: "Run again" }));
    expect(screen.getByRole("heading", { name: "Start with your resume" })).toBeInTheDocument();
    expect(screen.getByText("sam-rivera.pdf")).toBeInTheDocument();
  });
});

describe("Resume view", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("autosaves edits", async () => {
    const db = fakeBackend(invoke, { profile: importedProfile() });
    renderApp();
    await userEvent.click(screen.getByRole("button", { name: /Resume/ }));
    const name = await screen.findByLabelText("Full name");
    await userEvent.clear(name);
    await userEvent.type(name, "Sam R.");
    await waitFor(() => expect(db.profile?.profile.contact.name).toBe("Sam R."));
    expect(screen.getByText("Saved")).toBeInTheDocument();
  });

  it("adds and removes skills and achievements", async () => {
    const db = fakeBackend(invoke, { profile: importedProfile() });
    renderApp();
    await userEvent.click(screen.getByRole("button", { name: /Resume/ }));
    await userEvent.type(await screen.findByLabelText("Skills"), "Rust, Go{Enter}");
    await userEvent.click(screen.getByRole("button", { name: "Remove React" }));
    await userEvent.click(screen.getByRole("button", { name: "Remove achievement 1 at Brook" }));
    await waitFor(() =>
      expect(db.profile?.profile.skills.map((s) => s.name)).toEqual(["TypeScript", "Rust", "Go"]),
    );
    // Kept skills keep their source line for later evidence.
    expect(db.profile?.profile.skills[0]).toEqual({ id: "s6", name: "TypeScript", lines: [8] });
    expect(db.profile?.profile.roles[0].achievements).toHaveLength(0);
  });

  it("asks before replacing an imported resume", async () => {
    fakeBackend(invoke, { profile: importedProfile() });
    renderApp();
    await userEvent.click(screen.getByRole("button", { name: /Resume/ }));
    await userEvent.click(await screen.findByRole("button", { name: /Replace/ }));
    expect(screen.getByText(/replaces your profile/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByText(/replaces your profile/)).not.toBeInTheDocument();
  });
});
