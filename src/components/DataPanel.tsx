import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useEffect, useRef, useState } from "react";
import type { AiSettings } from "../bindings/AiSettings";
import type { AppInfo } from "../bindings/AppInfo";
import type { DataOverview } from "../bindings/DataOverview";
import { focusOnMount } from "../lib/focus";
import { formatBytes } from "../lib/format";
import { ipc } from "../lib/ipc";
import { plural } from "../lib/profile";
import { reloadApp } from "../lib/reload";
import { Icon } from "./Icon";
import { useToast } from "./Toast";

type Confirm = { kind: "restore"; file: File } | { kind: "delete" } | null;

/** Settings → Your data: where it lives, what leaves the computer, backup, restore, delete. */
export function DataPanel({ info }: { info: AppInfo | null }) {
  const toast = useToast();
  const [overview, setOverview] = useState<DataOverview | null>(null);
  const [ai, setAi] = useState<AiSettings | null>(null);
  const [confirm, setConfirm] = useState<Confirm>(null);
  const [busy, setBusy] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);

  useEffect(() => {
    ipc.getDataOverview().then(setOverview, () => {});
    ipc.getAiSettings().then(setAi, () => {});
  }, []);

  const backup = async () => {
    setBusy(true);
    try {
      const path = await ipc.createBackup();
      toast(`Saved ${path.split(/[\\/]/).pop()} to Downloads.`);
      void revealItemInDir(path).catch(() => {});
    } catch (err) {
      toast(`Couldn't back up: ${err}`, "error");
    } finally {
      setBusy(false);
    }
  };

  const replaceData = async () => {
    if (!confirm) return;
    setBusy(true);
    try {
      if (confirm.kind === "restore") await ipc.restoreBackup(confirm.file);
      else await ipc.deleteAllData();
      reloadApp();
    } catch (err) {
      toast(String(err), "error");
      setBusy(false);
      setConfirm(null);
    }
  };

  return (
    <div className="panel" id="settings-privacy">
      <h2>
        <Icon name="hard_drive" />
        Your data
      </h2>
      <p className="panel-sub">
        No account, no cloud. Your resume, jobs, letters and history stay on this computer.
      </p>
      {overview && info ? (
        <dl className="kv">
          <dt>Stored at</dt>
          <dd>{overview.dataDir}</dd>
          <dt>Size</dt>
          <dd>{formatBytes(overview.totalBytes)}</dd>
          <dt>Contents</dt>
          <dd>
            {plural(overview.jobs, "job")} · {plural(overview.applications, "application")} ·{" "}
            {plural(overview.drafts, "draft")}
            {overview.resumeFile ? ` · ${overview.resumeFile}` : ""}
          </dd>
          <dt>App version</dt>
          <dd>
            {info.version} · data v{info.schemaVersion}
          </dd>
        </dl>
      ) : (
        <p className="panel-sub">Loading…</p>
      )}

      <ul className="privacy-facts">
        <li>
          <Icon name="lock" />
          Your resume is read on this computer. The file is never uploaded.
        </li>
        <li>
          <Icon name="travel_explore" />
          Job sources are fetched from their public feeds. Nothing about you is sent to them.
        </li>
        <li>
          <Icon name={ai?.provider ? "auto_awesome" : "block"} />
          {ai?.provider
            ? "AI is on: when you ask, job text and your resume (without name or contact details) go to Claude Code."
            : "AI is off: nothing leaves this computer for analysis."}
        </li>
        <li>
          <Icon name="send" />
          JobLens never applies for you. You send every application yourself.
        </li>
      </ul>

      <div className="setting-row">
        <div className="setting-row-text">
          <strong>Back up</strong>
          <span>One file with everything, to move to another computer or keep safe.</span>
        </div>
        <button type="button" className="btn btn-secondary" disabled={busy} onClick={backup}>
          <Icon name="download" />
          Back up
        </button>
      </div>
      <div className="setting-row">
        <div className="setting-row-text">
          <strong>Restore</strong>
          <span>Replace everything here with a JobLens backup file.</span>
        </div>
        <input
          ref={fileInput}
          type="file"
          accept=".zip"
          hidden
          aria-label="Backup file"
          onChange={(e) => {
            const file = e.target.files?.[0];
            e.target.value = "";
            if (file) setConfirm({ kind: "restore", file });
          }}
        />
        <button
          type="button"
          className="btn btn-secondary"
          disabled={busy}
          onClick={() => fileInput.current?.click()}
        >
          <Icon name="upload" />
          Restore…
        </button>
      </div>
      <div className="setting-row">
        <div className="setting-row-text">
          <strong>Delete all data</strong>
          <span>Profile, resume file, jobs, letters and applications. Exported files stay.</span>
        </div>
        <button
          type="button"
          className="btn btn-danger"
          disabled={busy}
          onClick={() => setConfirm({ kind: "delete" })}
        >
          <Icon name="delete" />
          Delete…
        </button>
      </div>

      {confirm && (
        <div
          className="consent"
          role="alertdialog"
          aria-labelledby="data-confirm-title"
          onKeyDown={(e) => e.key === "Escape" && setConfirm(null)}
        >
          <h3 id="data-confirm-title">
            {confirm.kind === "delete"
              ? "Delete everything JobLens has stored?"
              : `Replace everything with “${confirm.file.name}”?`}
          </h3>
          <p>
            {confirm.kind === "delete"
              ? "This can’t be undone. Back up first if you might want it later."
              : "Your current data is replaced. A copy of it is kept in the data folder until you delete all data."}
          </p>
          <div className="consent-actions">
            <button
              type="button"
              className={confirm.kind === "delete" ? "btn btn-danger" : "btn btn-primary"}
              disabled={busy}
              onClick={replaceData}
            >
              {confirm.kind === "delete" ? "Delete everything" : "Restore backup"}
            </button>
            <button
              type="button"
              className="btn btn-ghost"
              ref={focusOnMount}
              onClick={() => setConfirm(null)}
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
