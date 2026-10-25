import type { EligibilityStatus } from "../bindings/EligibilityStatus";
import { ELIGIBILITY_LABELS } from "../lib/feed";
import { Icon } from "./Icon";

/** The "Can I apply?" badge. */
export function EligibilityBadge({ status }: { status: EligibilityStatus }) {
  const { label, icon } = ELIGIBILITY_LABELS[status];
  return (
    <span className="elig-badge" data-status={status}>
      <Icon name={icon} />
      {label}
    </span>
  );
}
