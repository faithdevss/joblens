import { EmptyState } from "../components/EmptyState";
import { ViewHeader } from "./ViewHeader";

export function FeedView() {
  return (
    <section className="view">
      <ViewHeader title="Job feed" subtitle="Remote jobs you can apply to, best matches first." />
      <EmptyState icon="work" title="No jobs yet">
        Add your resume and search, then turn on a job source to fill your feed.
      </EmptyState>
    </section>
  );
}
