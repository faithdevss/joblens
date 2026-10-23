import { useCallback, useEffect, useState } from "react";
import type { Profile } from "../bindings/Profile";
import type { SearchSettings } from "../bindings/SearchSettings";
import type { SetupStatus } from "../bindings/SetupStatus";
import type { StoredProfile } from "../bindings/StoredProfile";
import { useToast } from "../components/Toast";
import { ipc } from "./ipc";
import { emptyProfile, emptySearch } from "./profile";
import { useAutosave } from "./useAutosave";

/** Profile, search settings and setup status, loaded once and autosaved on change. */
export function useAppData() {
  const toast = useToast();
  const [loaded, setLoaded] = useState(false);
  const [stored, setStored] = useState<StoredProfile | null>(null);
  const [search, setSearch] = useState<SearchSettings>(emptySearch);
  const [setupStatus, setSetupStatus] = useState<SetupStatus | null>(null);
  const profileSave = useAutosave(ipc.saveProfile);
  const searchSave = useAutosave(ipc.saveSearchSettings);

  useEffect(() => {
    Promise.all([ipc.getProfile(), ipc.getSearchSettings(), ipc.getSetupStatus()])
      .then(([p, s, status]) => {
        setStored(p);
        setSearch(s);
        setSetupStatus(status);
        setLoaded(true);
      })
      .catch((err) => toast(`Couldn't load your profile: ${err}`, "error"));
  }, [toast]);

  useEffect(() => {
    if (profileSave.status === "error" || searchSave.status === "error") {
      toast("Couldn't save your changes. They'll be retried on your next edit.", "error");
    }
  }, [profileSave.status, searchSave.status, toast]);

  const updateProfile = useCallback(
    (profile: Profile) => {
      setStored((s) => ({ profile, resume: s?.resume ?? null, resumeLines: s?.resumeLines ?? [] }));
      profileSave.schedule(profile);
    },
    [profileSave.schedule],
  );

  const updateSearch = useCallback(
    (settings: SearchSettings) => {
      setSearch(settings);
      searchSave.schedule(settings);
    },
    [searchSave.schedule],
  );

  const importResume = useCallback(
    async (file: File) => {
      // The import replaces the profile; a queued edit must not overwrite it afterwards.
      profileSave.cancel();
      const next = await ipc.importResume(file);
      setStored(next);
      return next;
    },
    [profileSave.cancel],
  );

  const finishSetup = useCallback(
    async (status: SetupStatus) => {
      await Promise.all([profileSave.flush(), searchSave.flush()]);
      await ipc.setSetupStatus(status);
      setSetupStatus(status);
    },
    [profileSave.flush, searchSave.flush],
  );

  return {
    loaded,
    stored,
    profile: stored?.profile ?? emptyProfile(),
    search,
    setupStatus,
    profileSaveStatus: profileSave.status,
    searchSaveStatus: searchSave.status,
    updateProfile,
    updateSearch,
    importResume,
    finishSetup,
  };
}

export type AppData = ReturnType<typeof useAppData>;
