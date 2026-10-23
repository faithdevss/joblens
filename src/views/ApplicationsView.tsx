import { EmptyState } from "../components/EmptyState";
import { ViewHeader } from "./ViewHeader";

export function ApplicationsView() {
  return (
    <section className="view">
      <ViewHeader title="Applications" subtitle="Track where you applied and when to follow up." />
      <EmptyState icon="checklist" title="Nothing here yet">
        Jobs you save or apply to show up here.
      </EmptyState>
    </section>
  );
}
