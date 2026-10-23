// Typed wrappers around Tauri commands. Every Rust command gets one function here,
// using types generated into src/bindings by `cargo test` (ts-rs).
import { invoke } from "@tauri-apps/api/core";
import type { AppInfo } from "../bindings/AppInfo";
import type { Profile } from "../bindings/Profile";
import type { SearchSettings } from "../bindings/SearchSettings";
import type { SetupStatus } from "../bindings/SetupStatus";
import type { StoredProfile } from "../bindings/StoredProfile";

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
};
