// Typed wrappers around Tauri commands. Every Rust command gets one function here,
// using types generated into src/bindings by `cargo test` (ts-rs).
import { invoke } from "@tauri-apps/api/core";
import type { AiOverview } from "../bindings/AiOverview";
import type { AiSettings } from "../bindings/AiSettings";
import type { AlertSettings } from "../bindings/AlertSettings";
import type { AppInfo } from "../bindings/AppInfo";
import type { Application } from "../bindings/Application";
import type { ApplicationEdit } from "../bindings/ApplicationEdit";
import type { DataOverview } from "../bindings/DataOverview";
import type { Draft } from "../bindings/Draft";
import type { DraftKind } from "../bindings/DraftKind";
import type { DraftLength } from "../bindings/DraftLength";
import type { EditAction } from "../bindings/EditAction";
import type { ExportFormat } from "../bindings/ExportFormat";
import type { ExportResult } from "../bindings/ExportResult";
import type { Feed } from "../bindings/Feed";
import type { GapCheck } from "../bindings/GapCheck";
import type { InterviewPrep } from "../bindings/InterviewPrep";
import type { JobDetail } from "../bindings/JobDetail";
import type { JobState } from "../bindings/JobState";
import type { ManualJobInput } from "../bindings/ManualJobInput";
import type { OpenCheck } from "../bindings/OpenCheck";
import type { Profile } from "../bindings/Profile";
import type { Provider } from "../bindings/Provider";
import type { RefreshReport } from "../bindings/RefreshReport";
import type { SavedSearch } from "../bindings/SavedSearch";
import type { SearchSettings } from "../bindings/SearchSettings";
import type { SetupStatus } from "../bindings/SetupStatus";
import type { SourceStatus } from "../bindings/SourceStatus";
import type { StoredProfile } from "../bindings/StoredProfile";
import type { Weights } from "../bindings/Weights";
import type { WriterState } from "../bindings/WriterState";

export const ipc = {
  appInfo: () => invoke<AppInfo>("app_info"),
  getProfile: () => invoke<StoredProfile | null>("get_profile"),
  saveProfile: (profile: Profile) => invoke<void>("save_profile", { profile }),
  /** Sends the file as raw bytes; parsing happens offline in Rust. */
  importResume: async (file: File) =>
    invoke<StoredProfile>("import_resume", new Uint8Array(await file.arrayBuffer()), {
      headers: { "x-file-name": encodeURIComponent(file.name) },
    }),
  getSearchSettings: () => invoke<SearchSettings>("get_search_settings"),
  saveSearchSettings: (settings: SearchSettings) =>
    invoke<void>("save_search_settings", { settings }),
  getSetupStatus: () => invoke<SetupStatus>("get_setup_status"),
  setSetupStatus: (status: SetupStatus) => invoke<void>("set_setup_status", { status }),
  getFeed: () => invoke<Feed>("get_feed"),
  getJob: (id: number) => invoke<JobDetail>("get_job", { id }),
  setJobState: (id: number, jobState: JobState) => invoke<void>("set_job_state", { id, jobState }),
  listSources: () => invoke<SourceStatus[]>("list_sources"),
  setSourceEnabled: (id: string, enabled: boolean) =>
    invoke<void>("set_source_enabled", { id, enabled }),
  /** Fetches enabled sources, or only the given ones. Sources fetched recently are skipped. */
  refreshSources: (only: string[] | null = null) =>
    invoke<RefreshReport>("refresh_sources", { only }),
  addWatchlistCompany: (input: string) => invoke<SourceStatus>("add_watchlist_company", { input }),
  removeSource: (id: string) => invoke<void>("remove_source", { id }),
  /** Returns the new job's id. */
  addManualJob: (input: ManualJobInput) => invoke<number>("add_manual_job", { input }),
  getWeights: () => invoke<Weights>("get_weights"),
  saveWeights: (weights: Weights) => invoke<void>("save_weights", { weights }),
  getAiSettings: () => invoke<AiSettings>("get_ai_settings"),
  /** Also checks which AI tools are installed; takes a moment. */
  getAiOverview: () => invoke<AiOverview>("get_ai_overview"),
  setAiProvider: (provider: Provider | null, consent: boolean) =>
    invoke<AiSettings>("set_ai_provider", { provider, consent }),
  /** Uses the cached analysis unless `force`. */
  analyzeJob: (id: number, force = false) => invoke<JobDetail>("analyze_job", { id, force }),
  cancelAnalysis: (id: number) => invoke<void>("cancel_analysis", { id }),
  getGapCheck: (id: number) => invoke<GapCheck>("get_gap_check", { id }),
  /** Accepting changes the profile; undo puts it back. */
  decideEdit: (id: number, editId: string, action: EditAction) =>
    invoke<GapCheck>("decide_edit", { id, editId, action }),
  /** AI rewordings of the user's own resume lines. */
  suggestEdits: (id: number) => invoke<GapCheck>("suggest_edits", { id }),
  getWriter: (id: number, kind: DraftKind) => invoke<WriterState>("get_writer", { id, kind }),
  /** Replaces the working draft. */
  writeDraft: (
    id: number,
    kind: DraftKind,
    length: DraftLength,
    highlights: string[],
    useAi: boolean,
  ) => invoke<Draft>("write_draft", { id, kind, length, highlights, useAi }),
  rewriteParagraph: (id: number, kind: DraftKind, index: number, useAi: boolean) =>
    invoke<Draft>("rewrite_paragraph", { id, kind, index, useAi }),
  setParagraphVersion: (id: number, kind: DraftKind, index: number, version: number) =>
    invoke<Draft>("set_paragraph_version", { id, kind, index, version }),
  editParagraph: (id: number, kind: DraftKind, index: number, text: string) =>
    invoke<Draft>("edit_paragraph", { id, kind, index, text }),
  resolveSentence: (
    id: number,
    kind: DraftKind,
    paragraph: number,
    sentence: number,
    keep: boolean,
  ) => invoke<Draft>("resolve_sentence", { id, kind, paragraph, sentence, keep }),
  updateDraftText: (
    id: number,
    kind: DraftKind,
    subject: string,
    greeting: string,
    signOff: string,
  ) => invoke<Draft>("update_draft_text", { id, kind, subject, greeting, signOff }),
  /** Refused while sentences still need checking. Files go to Downloads. */
  exportDraft: (id: number, kind: DraftKind, format: ExportFormat) =>
    invoke<ExportResult>("export_draft", { id, kind, format }),
  cancelWriting: (id: number) => invoke<void>("cancel_writing", { id }),
  listApplications: () => invoke<Application[]>("list_applications"),
  updateApplication: (id: number, edit: ApplicationEdit) =>
    invoke<Application>("update_application", { id, edit }),
  /** Saves the job if needed and records what was sent. */
  markApplied: (id: number) => invoke<Application>("mark_applied", { id }),
  markFollowedUp: (id: number) => invoke<Application>("mark_followed_up", { id }),
  /** Returns the saved file's path (in Downloads). */
  exportApplicationsCsv: () => invoke<string>("export_applications_csv"),
  getDataOverview: () => invoke<DataOverview>("get_data_overview"),
  /** Returns the backup's path (in Downloads). */
  createBackup: () => invoke<string>("create_backup"),
  /** Replaces all data with the backup. Reload the app afterwards. */
  restoreBackup: async (file: File) =>
    invoke<void>("restore_backup", new Uint8Array(await file.arrayBuffer())),
  /** Deletes everything. Reload the app afterwards. */
  deleteAllData: () => invoke<void>("delete_all_data", { confirm: true }),
  getInterviewPrep: (id: number) => invoke<InterviewPrep>("get_interview_prep", { id }),
  setTopicPrepared: (id: number, key: string, prepared: boolean) =>
    invoke<void>("set_topic_prepared", { id, key, prepared }),
  /** Fetches the original posting. */
  checkStillOpen: (id: number) => invoke<OpenCheck>("check_still_open", { id }),
  listSearches: () => invoke<SavedSearch[]>("list_searches"),
  /** Copies the active search under a new name and switches to it. */
  createSearch: (name: string) => invoke<SavedSearch[]>("create_search", { name }),
  renameSearch: (id: number, name: string) => invoke<SavedSearch[]>("rename_search", { id, name }),
  deleteSearch: (id: number) => invoke<SavedSearch[]>("delete_search", { id }),
  setActiveSearch: (id: number) => invoke<SearchSettings>("set_active_search", { id }),
  setSearchAlerts: (id: number, on: boolean) =>
    invoke<SavedSearch[]>("set_search_alerts", { id, on }),
  getAlertSettings: () => invoke<AlertSettings>("get_alert_settings"),
  saveAlertSettings: (settings: AlertSettings) =>
    invoke<AlertSettings>("save_alert_settings", { settings }),
  sendTestAlert: () => invoke<void>("send_test_alert"),
};
