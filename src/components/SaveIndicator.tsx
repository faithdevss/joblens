import type { SaveStatus } from "../lib/useAutosave";
import { Icon } from "./Icon";

const LABELS: Record<SaveStatus, { icon: string; text: string }> = {
  idle: { icon: "cloud_done", text: "Saved automatically" },
  saving: { icon: "progress_activity", text: "Saving…" },
  saved: { icon: "cloud_done", text: "Saved" },
  error: { icon: "error", text: "Couldn't save" },
};

export function SaveIndicator({ status }: { status: SaveStatus }) {
  const { icon, text } = LABELS[status];
  return (
    <span className="save-indicator" data-status={status} aria-live="polite">
      <Icon name={icon} />
      {text}
    </span>
  );
}
