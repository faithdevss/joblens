import type { ReactNode } from "react";

type ViewHeaderProps = {
  title: string;
  subtitle?: string;
  aside?: ReactNode;
};

export function ViewHeader({ title, subtitle, aside }: ViewHeaderProps) {
  return (
    <header className="view-header">
      <div>
        <h1>{title}</h1>
        {subtitle && <p>{subtitle}</p>}
      </div>
      {aside}
    </header>
  );
}
