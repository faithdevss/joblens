import { useEffect, useState } from "react";
import type { AiSettings } from "../bindings/AiSettings";
import { ipc } from "../lib/ipc";
import { NAV_ITEMS, type ViewId } from "../lib/nav";
import { Icon } from "./Icon";

type SidebarProps = {
  view: ViewId;
  version: string;
  onNavigate: (view: ViewId) => void;
  onQuickSetup: () => void;
};

export function Sidebar({ view, version, onNavigate, onQuickSetup }: SidebarProps) {
  const [ai, setAi] = useState<AiSettings | null>(null);

  // AI can be switched in Settings; check again whenever the view changes.
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-read on navigation
  useEffect(() => {
    ipc.getAiSettings().then(setAi, () => {});
  }, [view]);
  const aiLabel =
    ai?.provider === "claude" ? "Claude Code" : ai?.provider === "codex" ? "Codex" : null;

  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="brand-name">
          <Icon name="travel_explore" />
          JobLens
        </div>
        <div className="brand-meta">local · v{version}</div>
      </div>

      <nav className="nav" aria-label="Main">
        {NAV_ITEMS.map((item) => (
          <button
            key={item.id}
            type="button"
            className="nav-item"
            aria-current={view === item.id ? "page" : undefined}
            onClick={() => onNavigate(item.id)}
          >
            <span className="nav-item-label">
              <Icon name={item.icon} />
              {item.label}
            </span>
          </button>
        ))}
      </nav>

      <button type="button" className="setup-button" onClick={onQuickSetup}>
        <Icon name="restart_alt" />
        Quick setup
      </button>

      <div className="privacy-card">
        <button type="button" className="privacy-row" onClick={() => onNavigate("settings")}>
          <Icon name="lock" />
          <span className="privacy-row-text">
            <span className="privacy-row-title">On this device</span>
            <span className="privacy-row-sub">No account · no cloud</span>
          </span>
        </button>
        <div className="privacy-row">
          <span className="status-dot" data-on={aiLabel !== null} />
          <span className="privacy-row-text">
            <span className="privacy-row-title">{aiLabel ? "AI on" : "AI off"}</span>
            <span className="privacy-row-sub mono">{aiLabel ?? "Rules only"}</span>
          </span>
        </div>
      </div>
    </aside>
  );
}
