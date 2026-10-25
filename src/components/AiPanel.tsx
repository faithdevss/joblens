import { useCallback, useEffect, useState } from "react";
import type { AiOverview } from "../bindings/AiOverview";
import type { Provider } from "../bindings/Provider";
import type { ProviderStatus } from "../bindings/ProviderStatus";
import { focusOnMount } from "../lib/focus";
import { formatShortDate } from "../lib/format";
import { ipc } from "../lib/ipc";
import { Icon } from "./Icon";
import { useToast } from "./Toast";

const COMPANY: Record<Provider, string> = { claude: "Anthropic", codex: "OpenAI" };

function usable(p: ProviderStatus): boolean {
  return p.supported && p.installed && p.loggedIn !== false;
}

/** Settings → Privacy & AI: AI is off until the user picks a tool and agrees. */
export function AiPanel() {
  const toast = useToast();
  const [overview, setOverview] = useState<AiOverview | null>(null);
  const [checking, setChecking] = useState(false);
  const [asking, setAsking] = useState<ProviderStatus | null>(null);

  const check = useCallback(async () => {
    setChecking(true);
    try {
      setOverview(await ipc.getAiOverview());
    } catch (err) {
      toast(`Couldn't check AI tools: ${err}`, "error");
    } finally {
      setChecking(false);
    }
  }, [toast]);

  useEffect(() => {
    check();
  }, [check]);

  const choose = async (provider: Provider | null) => {
    try {
      const settings = await ipc.setAiProvider(provider, provider !== null);
      setOverview((o) => o && { ...o, settings });
      setAsking(null);
      toast(provider ? "AI is on." : "AI is off. Nothing is sent anywhere.");
    } catch (err) {
      toast(String(err), "error");
    }
  };

  const current = overview?.settings.provider ?? null;

  return (
    <div className="panel" id="settings-privacy">
      <h2>
        <Icon name="shield_lock" />
        Privacy &amp; AI
      </h2>
      <p className="panel-sub">
        Your resume, jobs and history stay on this computer. AI is optional: everything works
        without it. If you turn it on, JobLens uses an AI tool you already have installed and signed
        in to.
      </p>

      <div className="ai-options">
        <div className="ai-option" data-current={current === null || undefined}>
          <div className="ai-option-text">
            <strong>Off</strong>
            <span>Rules-only matching. Nothing leaves this computer.</span>
          </div>
          {current === null ? (
            <span className="tag">In use</span>
          ) : (
            <button type="button" className="btn btn-secondary" onClick={() => choose(null)}>
              Turn off
            </button>
          )}
        </div>

        {!overview ? (
          <p className="panel-sub">Checking which AI tools are installed…</p>
        ) : (
          overview.providers.map((p) => (
            <div
              key={p.provider}
              className="ai-option"
              data-current={current === p.provider || undefined}
            >
              <div className="ai-option-text">
                <strong>
                  {p.label}
                  {p.version && <span className="mono muted"> {p.version}</span>}
                </strong>
                <span>{p.note}</span>
              </div>
              {current === p.provider ? (
                <span className="tag tag-ok">In use</span>
              ) : (
                <button
                  type="button"
                  className="btn btn-secondary"
                  disabled={!usable(p)}
                  onClick={() => setAsking(p)}
                >
                  Use {p.label}
                </button>
              )}
            </div>
          ))
        )}
      </div>

      {asking && (
        <div
          className="consent"
          role="alertdialog"
          aria-labelledby="consent-title"
          onKeyDown={(e) => e.key === "Escape" && setAsking(null)}
        >
          <h3 id="consent-title">Send job and resume text to {COMPANY[asking.provider]}?</h3>
          <p>
            When you ask JobLens to check a job, it sends this through {asking.label} to{" "}
            {COMPANY[asking.provider]}:
          </p>
          <ul>
            <li>the job’s title, company and description;</li>
            <li>
              your resume’s headline, summary, roles, achievements, skills, education, projects,
              certifications and languages.
            </li>
          </ul>
          <p>
            Your name, email, phone, location and links are <strong>never</strong> sent. Checks only
            run when you ask, and use your {asking.label} plan.
          </p>
          <div className="consent-actions">
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => choose(asking.provider)}
            >
              I agree, turn on AI
            </button>
            <button
              type="button"
              className="btn btn-ghost"
              ref={focusOnMount}
              onClick={() => setAsking(null)}
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      <div className="setting-row">
        <div className="setting-row-text">
          <span>
            {overview?.settings.consentedAt
              ? `You agreed on ${formatShortDate(overview.settings.consentedAt)}.`
              : "Installed a tool or signed in just now?"}
          </span>
        </div>
        <button type="button" className="btn btn-ghost" onClick={check} disabled={checking}>
          <Icon name="refresh" className={checking ? "spin" : undefined} />
          Check again
        </button>
      </div>
    </div>
  );
}
