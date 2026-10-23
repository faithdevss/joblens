import type { AppInfo } from "../bindings/AppInfo";
import { Icon } from "../components/Icon";
import { SaveIndicator } from "../components/SaveIndicator";
import { SearchForm } from "../components/SearchForm";
import { formatBytes, formatShortDate } from "../lib/format";
import { plural, profileCounts } from "../lib/profile";
import type { AppData } from "../lib/useAppData";
import { ViewHeader } from "./ViewHeader";

type SettingsViewProps = {
  info: AppInfo | null;
  data: AppData;
  onEditProfile: () => void;
  onRunSetup: () => void;
};

export function SettingsView({ info, data, onEditProfile, onRunSetup }: SettingsViewProps) {
  const resume = data.stored?.resume;
  const counts = profileCounts(data.profile);

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
          <SearchForm value={data.search} onChange={data.updateSearch} />
        </div>

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

        <div className="panel">
          <h2>
            <Icon name="hard_drive" />
            Your data
          </h2>
          <p className="panel-sub">Your resume, jobs, letters and history stay on this computer.</p>
          {info ? (
            <dl className="kv">
              <dt>Stored at</dt>
              <dd>{info.dbPath}</dd>
              <dt>Size</dt>
              <dd>{formatBytes(info.dbSizeBytes)}</dd>
              <dt>Schema</dt>
              <dd>v{info.schemaVersion}</dd>
              <dt>App version</dt>
              <dd>{info.version}</dd>
            </dl>
          ) : (
            <p className="panel-sub">Loading…</p>
          )}
        </div>
      </div>
    </section>
  );
}
