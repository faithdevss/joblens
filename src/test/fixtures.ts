import type { AppInfo } from "../bindings/AppInfo";
import type { SearchSettings } from "../bindings/SearchSettings";
import type { SetupStatus } from "../bindings/SetupStatus";
import type { StoredProfile } from "../bindings/StoredProfile";
import { emptyProfile, emptySearch } from "../lib/profile";

export const info: AppInfo = {
  version: "0.1.0",
  dataDir: "/data",
  dbPath: "/data/joblens.db",
  schemaVersion: 2,
  dbSizeBytes: 4096,
};

export function importedProfile(): StoredProfile {
  const profile = emptyProfile();
  profile.contact.name = "Sam Rivera";
  profile.contact.headline = "Senior Frontend Engineer";
  profile.contact.location = "Lisbon, Portugal";
  profile.roles = [
    {
      id: "r1",
      title: "Senior Frontend Engineer",
      company: "Brook",
      dates: "Mar 2021 – Present",
      datesConfirmed: false,
      lines: [3],
      achievements: [{ id: "a2", text: "Led the TypeScript migration.", lines: [4] }],
    },
    {
      id: "r3",
      title: "Frontend Engineer",
      company: "Halcyon",
      dates: "2018 – 2021",
      datesConfirmed: false,
      lines: [5],
      achievements: [{ id: "a4", text: "Built the component library.", lines: [6] }],
    },
  ];
  profile.skills = [
    { id: "s5", name: "React", lines: [8] },
    { id: "s6", name: "TypeScript", lines: [8] },
  ];
  return {
    profile,
    resume: {
      fileName: "sam-rivera.pdf",
      sizeBytes: 188416,
      pages: 2,
      importedAt: "2026-10-23T10:00:00Z",
    },
    resumeLines: [],
  };
}

type Backend = {
  profile: StoredProfile | null;
  search: SearchSettings;
  setup: SetupStatus;
  importResult: StoredProfile | Error;
};

/** Routes mocked `invoke` calls to an in-memory backend. Returns the backend to inspect. */
export function fakeBackend(
  invoke: { mockImplementation: (fn: (cmd: string, args?: unknown) => Promise<unknown>) => void },
  overrides: Partial<Backend> = {},
): Backend {
  const db: Backend = {
    profile: null,
    search: emptySearch(),
    setup: "done",
    importResult: importedProfile(),
    ...overrides,
  };
  invoke.mockImplementation(async (cmd: string, args?: unknown) => {
    const a = args as Record<string, never>;
    switch (cmd) {
      case "app_info":
        return info;
      case "get_profile":
        return db.profile;
      case "save_profile":
        db.profile = {
          profile: a.profile,
          resume: db.profile?.resume ?? null,
          resumeLines: db.profile?.resumeLines ?? [],
        };
        return null;
      case "import_resume":
        if (db.importResult instanceof Error) throw db.importResult.message;
        db.profile = db.importResult;
        return db.profile;
      case "get_search_settings":
        return db.search;
      case "save_search_settings":
        db.search = a.settings;
        return null;
      case "get_setup_status":
        return db.setup;
      case "set_setup_status":
        db.setup = a.status;
        return null;
      default:
        throw new Error(`unexpected command ${cmd}`);
    }
  });
  return db;
}
