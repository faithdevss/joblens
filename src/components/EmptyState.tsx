import type { ReactNode } from "react";
import { Icon } from "./Icon";

type EmptyStateProps = {
  icon: string;
  title: string;
  children?: ReactNode;
};

export function EmptyState({ icon, title, children }: EmptyStateProps) {
  return (
    <div className="empty-state">
      <Icon name={icon} />
      <div className="empty-state-title">{title}</div>
      {children && <div className="empty-state-body">{children}</div>}
    </div>
  );
}
