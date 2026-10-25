import { useEffect } from "react";
import type { AppInfo } from "../bindings/AppInfo";
import { AiPanel } from "../components/AiPanel";
import { AlertsPanel } from "../components/AlertsPanel";
import { DataPanel } from "../components/DataPanel";
import { Icon } from "../components/Icon";
import { SaveIndicator } from "../components/SaveIndicator";
import { SearchForm } from "../components/SearchForm";
import { SearchPicker } from "../components/SearchPicker";
import { SourcesPanel } from "../components/SourcesPanel";
import { formatShortDate } from "../lib/format";
import { plural, profileCounts } from "../lib/profile";
import type { AppData } from "../lib/useAppData";
import { ViewHeader } from "./ViewHeader";

type SettingsViewProps = {
  info: AppInfo | null;
  data: AppData;
  onEditProfile: () => void;
  onRunSetup: () => void;
  /** Section to scroll to when the view opens. */
  focus?: "sources" | "privacy" | null;
};

export function SettingsView({ info, data, onEditProfile, onRunSetup, focus }: SettingsViewProps) {
  const resume = data.stored?.resume;
  const counts = profileCounts(data.profile);

  useEffect(() => {
    if (focus) document.getElementById(`settings-${focus}`)?.scrollIntoView?.({ block: "start" });
  }, [focus]);

  return (
    <section className="view">
      <ViewHeader
        title="Settings"
        subtitle="Search, sources, privacy and AI. Changes apply right away."
        aside={<SaveIndicator status={data.searchSaveStatus} />}
      />
      <div className="view-body">
        <div className="panel">
          <h2>
            <Icon name="search" />
            Search
          </h2>
          <p className="panel-sub">What the job feed looks for.</p>
          <SearchPicker data={data} />
          <SearchForm value={data.search} onChange={data.updateSearch} />
        </div>

        <SourcesPanel />

        <AlertsPanel />

        <AiPanel />

        <div className="panel">
          <h2>
            <Icon name="description" />
            Profile
          </h2>
          <div className="setting-row">
            <div className="setting-row-text">
              <strong>{resume?.fileName ?? "No resume uploaded"}</strong>
              <span>
                {plural(counts.roles, "role")} · {plural(counts.achievements, "achievement")}
                {resume ? ` · read ${formatShortDate(resume.importedAt)}` : ""}
              </span>
            </div>
            <button type="button" className="btn btn-secondary" onClick={onEditProfile}>
              Edit profile
              <Icon name="chevron_right" />
            </button>
          </div>
          <div className="setting-row">
            <div className="setting-row-text">
              <strong>Quick setup</strong>
              <span>Upload a new resume and redo your search in a few minutes.</span>
            </div>
            <button type="button" className="btn btn-secondary" onClick={onRunSetup}>
              Run again
            </button>
          </div>
        </div>

        <DataPanel info={info} />
      </div>
    </section>
  );
}
