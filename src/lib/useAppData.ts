import { useCallback, useEffect, useState } from "react";
import type { Profile } from "../bindings/Profile";
import type { SavedSearch } from "../bindings/SavedSearch";
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
  const [searches, setSearches] = useState<SavedSearch[]>([]);
  const [setupStatus, setSetupStatus] = useState<SetupStatus | null>(null);
  const profileSave = useAutosave(ipc.saveProfile);
  const searchSave = useAutosave(ipc.saveSearchSettings);

  useEffect(() => {
    Promise.all([
      ipc.getProfile(),
      ipc.getSearchSettings(),
      ipc.getSetupStatus(),
      ipc.listSearches(),
    ])
      .then(([p, s, status, list]) => {
        setStored(p);
        setSearch(s);
        setSetupStatus(status);
        setSearches(list);
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

  /** Runs a backend call that changes the profile, then reloads it. */
  const changeProfile = useCallback(
    async <T>(run: () => Promise<T>): Promise<T> => {
      // A queued edit saved afterwards would undo the change.
      await profileSave.flush();
      const result = await run();
      setStored(await ipc.getProfile());
      return result;
    },
    [profileSave.flush],
  );

  /**
   * Runs a backend call that changes the list of searches, after saving pending edits
   * to the active one, then reloads the active search's settings.
   */
  const changeSearches = useCallback(
    async (run: () => Promise<SavedSearch[]>) => {
      await searchSave.flush();
      const list = await run();
      setSearches(list);
      setSearch(await ipc.getSearchSettings());
    },
    [searchSave.flush],
  );

  const switchSearch = useCallback(
    async (id: number) => {
      await searchSave.flush();
      setSearch(await ipc.setActiveSearch(id));
      setSearches((list) => list.map((s) => ({ ...s, active: s.id === id })));
    },
    [searchSave.flush],
  );

  /** The first save creates the first search, so reload the list after it. */
  const reloadSearches = useCallback(async () => {
    await searchSave.flush();
    setSearches(await ipc.listSearches());
  }, [searchSave.flush]);

  const finishSetup = useCallback(
    async (status: SetupStatus) => {
      await Promise.all([profileSave.flush(), searchSave.flush()]);
      await ipc.setSetupStatus(status);
      setSetupStatus(status);
      setSearches(await ipc.listSearches());
    },
    [profileSave.flush, searchSave.flush],
  );

  return {
    loaded,
    stored,
    profile: stored?.profile ?? emptyProfile(),
    search,
    searches,
    setupStatus,
    profileSaveStatus: profileSave.status,
    searchSaveStatus: searchSave.status,
    updateProfile,
    updateSearch,
    changeSearches,
    switchSearch,
    reloadSearches,
    importResume,
    changeProfile,
    finishSetup,
  };
}

export type AppData = ReturnType<typeof useAppData>;
export type ChangeProfile = AppData["changeProfile"];
