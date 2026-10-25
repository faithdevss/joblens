import { useEffect, useState } from "react";
import type { AlertSettings } from "../bindings/AlertSettings";
import { ipc } from "../lib/ipc";
import { Icon } from "./Icon";
import { useToast } from "./Toast";

const HOURS = [1, 3, 6, 12, 24];
const SCORES = [70, 80, 90];

/** Settings → Updates & alerts: background refresh and great-match notifications. */
export function AlertsPanel() {
  const toast = useToast();
  const [settings, setSettings] = useState<AlertSettings | null>(null);

  useEffect(() => {
    ipc.getAlertSettings().then(setSettings, () => {});
  }, []);

  const save = async (patch: Partial<AlertSettings>) => {
    if (!settings) return;
    const next = { ...settings, ...patch };
    setSettings(next);
    try {
      setSettings(await ipc.saveAlertSettings(next));
      if (patch.notify) await ipc.sendTestAlert();
    } catch (err) {
      toast(String(err), "error");
      setSettings(settings);
    }
  };

  return (
    <div className="panel" id="settings-alerts">
      <h2>
        <Icon name="notifications" />
        Updates & alerts
      </h2>
      <p className="panel-sub">
        Runs only while JobLens is open. Sources are still fetched no more often than they allow.
      </p>
      {settings ? (
        <>
          <div className="setting-row">
            <div className="setting-row-text">
              <strong>Refresh in the background</strong>
              <span className="setting-inline">
                Check sources every
                <select
                  className="input input-inline"
                  aria-label="Refresh interval"
                  value={settings.everyHours}
                  disabled={!settings.autoRefresh}
                  onChange={(e) => save({ everyHours: Number(e.target.value) })}
                >
                  {HOURS.map((h) => (
                    <option key={h} value={h}>
                      {h === 1 ? "hour" : `${h} hours`}
                    </option>
                  ))}
                </select>
              </span>
            </div>
            <button
              type="button"
              role="switch"
              className="switch"
              aria-checked={settings.autoRefresh}
              aria-label="Refresh in the background"
              onClick={() => save({ autoRefresh: !settings.autoRefresh })}
            >
              <span />
            </button>
          </div>
          <div className="setting-row">
            <div className="setting-row-text">
              <strong>Alert me about great matches</strong>
              <span className="setting-inline">
                A desktop notification when a new job scores
                <select
                  className="input input-inline"
                  aria-label="Alert at match score"
                  value={settings.minScore}
                  disabled={!settings.notify}
                  onChange={(e) => save({ minScore: Number(e.target.value) })}
                >
                  {SCORES.map((s) => (
                    <option key={s} value={s}>
                      {s}%
                    </option>
                  ))}
                </select>
                or more. Jobs you can’t apply for are skipped.
              </span>
              {settings.notify && !settings.autoRefresh && (
                <span className="setting-warn">
                  Background refresh is off, so no alerts will be sent.
                </span>
              )}
            </div>
            <button
              type="button"
              role="switch"
              className="switch"
              aria-checked={settings.notify}
              aria-label="Alert me about great matches"
              onClick={() => save({ notify: !settings.notify })}
            >
              <span />
            </button>
          </div>
          {settings.notify && (
            <button
              type="button"
              className="btn-link"
              onClick={() => ipc.sendTestAlert().catch((err) => toast(String(err), "error"))}
            >
              Send a test alert
            </button>
          )}
        </>
      ) : (
        <p className="panel-sub">Loading…</p>
      )}
    </div>
  );
}
