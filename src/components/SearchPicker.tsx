import { useEffect, useState } from "react";
import type { SavedSearch } from "../bindings/SavedSearch";
import { ipc } from "../lib/ipc";
import type { AppData } from "../lib/useAppData";
import { useToast } from "./Toast";

type Mode = { kind: "new" | "rename"; name: string } | { kind: "delete" } | null;

/** Settings → Search: pick, add, rename and delete saved searches. */
export function SearchPicker({ data }: { data: AppData }) {
  const toast = useToast();
  const [mode, setMode] = useState<Mode>(null);
  const [busy, setBusy] = useState(false);
  const { searches, reloadSearches } = data;
  const active: SavedSearch | undefined = searches.find((s) => s.active);

  useEffect(() => {
    reloadSearches().catch(() => {});
  }, [reloadSearches]);

  const run = async (call: () => Promise<SavedSearch[]>, done?: string) => {
    setBusy(true);
    try {
      await data.changeSearches(call);
      setMode(null);
      if (done) toast(done);
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setBusy(false);
    }
  };

  if (!active) return null;

  const submitName = () => {
    if (mode?.kind === "new") {
      const name = mode.name;
      void run(
        () => ipc.createSearch(name),
        `Started “${name.trim()}” from a copy of “${active.name}”.`,
      );
    } else if (mode?.kind === "rename") {
      const name = mode.name;
      void run(() => ipc.renameSearch(active.id, name));
    }
  };

  return (
    <div className="search-picker">
      <div className="search-picker-row">
        <label className="field">
          <span className="field-label">Saved search</span>
          <select
            className="input"
            value={active.id}
            disabled={busy || searches.length < 2}
            onChange={async (e) => {
              try {
                await data.switchSearch(Number(e.target.value));
              } catch (err) {
                toast(`Couldn't switch searches: ${err}`, "error");
              }
            }}
          >
            {searches.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        </label>
        <div className="search-picker-actions">
          <button
            type="button"
            className="btn btn-secondary"
            disabled={busy}
            onClick={() => setMode({ kind: "new", name: "" })}
          >
            New search
          </button>
          <button
            type="button"
            className="btn btn-ghost"
            disabled={busy}
            onClick={() => setMode({ kind: "rename", name: active.name })}
          >
            Rename
          </button>
          <button
            type="button"
            className="btn btn-ghost"
            disabled={busy || searches.length < 2}
            title={searches.length < 2 ? "You need at least one search." : undefined}
            onClick={() => setMode({ kind: "delete" })}
          >
            Delete
          </button>
        </div>
      </div>
      <label className="check-row">
        <input
          type="checkbox"
          checked={active.alerts}
          disabled={busy}
          onChange={(e) => {
            const on = e.target.checked;
            void run(() => ipc.setSearchAlerts(active.id, on));
          }}
        />
        Include this search in great-match alerts
      </label>

      {(mode?.kind === "new" || mode?.kind === "rename") && (
        <form
          className="search-name"
          onSubmit={(e) => {
            e.preventDefault();
            submitName();
          }}
          onKeyDown={(e) => e.key === "Escape" && setMode(null)}
        >
          <input
            className="input"
            aria-label={mode.kind === "new" ? "New search name" : "Search name"}
            placeholder={mode.kind === "new" ? "e.g. Staff roles in Europe" : undefined}
            value={mode.name}
            maxLength={60}
            // biome-ignore lint/a11y/noAutofocus: the field opens on request
            autoFocus
            onChange={(e) => setMode({ kind: mode.kind, name: e.target.value })}
          />
          <button type="submit" className="btn btn-primary" disabled={busy || !mode.name.trim()}>
            {mode.kind === "new" ? "Create" : "Save name"}
          </button>
          <button type="button" className="btn btn-ghost" onClick={() => setMode(null)}>
            Cancel
          </button>
        </form>
      )}
      {mode?.kind === "new" && (
        <p className="panel-sub">It starts as a copy of “{active.name}”. Change it below.</p>
      )}
      {mode?.kind === "delete" && (
        <div className="consent" role="alertdialog" aria-labelledby="search-delete-title">
          <h3 id="search-delete-title">Delete “{active.name}”?</h3>
          <p>Jobs, letters and applications stay. Only these search settings go.</p>
          <div className="consent-actions">
            <button
              type="button"
              className="btn btn-danger"
              disabled={busy}
              onClick={() => run(() => ipc.deleteSearch(active.id), `Deleted “${active.name}”.`)}
            >
              Delete search
            </button>
            <button type="button" className="btn btn-ghost" onClick={() => setMode(null)}>
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
