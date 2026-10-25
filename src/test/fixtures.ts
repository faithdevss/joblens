import type { AiSettings } from "../bindings/AiSettings";
import type { AlertSettings } from "../bindings/AlertSettings";
import type { AppInfo } from "../bindings/AppInfo";
import type { Application } from "../bindings/Application";
import type { Draft } from "../bindings/Draft";
import type { GapCheck } from "../bindings/GapCheck";
import type { InterviewPrep } from "../bindings/InterviewPrep";
import type { JobAnalysis } from "../bindings/JobAnalysis";
import type { JobSummary } from "../bindings/JobSummary";
import type { OpenCheck } from "../bindings/OpenCheck";
import type { RefreshReport } from "../bindings/RefreshReport";
import type { SavedSearch } from "../bindings/SavedSearch";
import type { SearchSettings } from "../bindings/SearchSettings";
import type { SetupStatus } from "../bindings/SetupStatus";
import type { SourceStatus } from "../bindings/SourceStatus";
import type { StoredProfile } from "../bindings/StoredProfile";
import type { Weights } from "../bindings/Weights";
import type { WriterState } from "../bindings/WriterState";
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

export function job(overrides: Partial<JobSummary> = {}): JobSummary {
  return {
    id: 1,
    title: "Senior Frontend Engineer",
    company: "Lumen Health",
    url: "https://remotive.com/remote-jobs/1",
    location: "Worldwide",
    salary: { min: 60000, max: 80000, currency: "EUR", period: "year", text: "€60k – €80k" },
    postedAt: "2026-10-22T10:00:00Z",
    firstSeenAt: "2026-10-23T10:00:00Z",
    state: "new",
    eligibility: { status: "eligible", note: "Open worldwide (“Worldwide”)." },
    score: 82,
    aiChecked: false,
    listings: [
      { sourceId: "remotive", sourceName: "Remotive", url: "https://remotive.com/remote-jobs/1" },
    ],
    ...overrides,
  };
}

export function sampleJobs(): JobSummary[] {
  return [
    job(),
    job({
      id: 2,
      title: "Backend Engineer",
      company: "Acme",
      url: "https://weworkremotely.com/2",
      salary: null,
      score: 55,
      postedAt: "2026-10-23T09:00:00Z",
      eligibility: { status: "restricted", note: "“US only” You can’t apply from Portugal." },
      listings: [
        { sourceId: "wwr", sourceName: "We Work Remotely", url: "https://weworkremotely.com/2" },
        { sourceId: "remoteok", sourceName: "Remote OK", url: "https://remoteok.com/2" },
      ],
    }),
  ];
}

export function boardSources(): SourceStatus[] {
  const base = {
    kind: "board" as const,
    enabled: true,
    lastRunAt: null,
    lastOkAt: null,
    lastError: null,
    jobCount: 0,
  };
  return [
    { ...base, id: "remotive", name: "Remotive", detail: "remotive.com" },
    { ...base, id: "remoteok", name: "Remote OK", detail: "remoteok.com" },
    { ...base, id: "wwr", name: "We Work Remotely", detail: "weworkremotely.com" },
  ];
}

type Backend = {
  profile: StoredProfile | null;
  search: SearchSettings;
  setup: SetupStatus;
  importResult: StoredProfile | Error;
  jobs: JobSummary[];
  sources: SourceStatus[];
  lastUpdatedAt: string | null;
  refreshReport: RefreshReport;
  /** Error message for add_watchlist_company, or the company to add. */
  watchlistResult: string | Error;
  manualJobs: unknown[];
  weights: Weights;
  ai: AiSettings;
  /** Result of the next analyze_job call. */
  analysis: JobAnalysis | Error;
  analyzed: Record<number, JobAnalysis>;
  gap: GapCheck;
  writer: WriterState;
  /** Returned by the next write_draft call. */
  nextDraft: Draft;
  /** Profile saves, in order, to check that edits are flushed before profile changes. */
  calls: string[];
  applications: Application[];
  prep: InterviewPrep;
  openCheck: OpenCheck;
  searches: SavedSearch[];
  /** Settings of searches other than the active one, by id. */
  otherSearches: Record<number, SearchSettings>;
  alerts: AlertSettings;
};

export function samplePrep(): InterviewPrep {
  return {
    topics: [
      {
        key: "req:react",
        topic: "React",
        kind: "story",
        prompt: "Tell me about a time you used React.",
        evidence: [{ id: "a2", text: "Led the TypeScript migration.", lines: [6] }],
        prepared: false,
      },
      {
        key: "req:graphql",
        topic: "GraphQL",
        kind: "gap",
        prompt: "Be ready to say how you'd pick it up.",
        evidence: [],
        prepared: false,
      },
    ],
    questions: ["What does success look like in the first 90 days?"],
  };
}

export function application(overrides: Partial<Application> = {}): Application {
  return {
    jobId: 1,
    title: "Senior Frontend Engineer",
    company: "Lumen Health",
    url: "https://remotive.com/remote-jobs/1",
    status: "applied",
    appliedAt: "2026-10-18",
    resumeVersion: "cv.pdf · read Oct 17",
    letter: "Cover letter · Oct 18",
    link: "",
    notes: "",
    followUpAt: "2026-10-25",
    updatedAt: "2026-10-18T10:00:00Z",
    ...overrides,
  };
}

export function sampleGap(): GapCheck {
  return {
    requirements: [
      {
        text: "React",
        required: false,
        coverage: "covered",
        evidence: [{ id: "skill:React", text: "React, TypeScript", lines: [9] }],
        note: "",
      },
      { text: "GraphQL", required: false, coverage: "missing", evidence: [], note: "" },
    ],
    fromAi: false,
    aiStale: false,
    resumeLines: ["Sam Rivera", "EXPERIENCE", "Senior Frontend Engineer, Brook"],
    checks: [
      { id: "contact", ok: true, label: "Email and phone found", note: "" },
      {
        id: "dates",
        ok: false,
        label: "Dates use different formats",
        note: "“2018–2021” vs “Mar 2021 – Present”. Pick one format.",
      },
    ],
    edits: [
      {
        id: "skill:typescript",
        source: "rules",
        why: "The job asks for TypeScript, and your resume mentions it, but it isn’t in your skills list.",
        place: "Skills",
        before: "Mentioned in: “Led the TypeScript migration”",
        after: "TypeScript",
        warning: "",
        decision: null,
        finalText: "",
      },
    ],
    aiSuggested: false,
  };
}

export function sampleDraft(): Draft {
  const plain = (text: string) => ({ text, backing: "plain" as const, evidence: [], note: "" });
  return {
    kind: "cover_letter",
    length: "standard",
    highlights: ["a2"],
    subject: "",
    greeting: "Dear [hiring manager name],",
    paragraphs: [
      {
        role: "intro",
        current: 0,
        versions: [
          {
            source: "rules",
            sentences: [
              plain("I’m applying for the Senior Frontend Engineer role at Lumen Health."),
            ],
          },
        ],
      },
      {
        role: "highlights",
        current: 0,
        versions: [
          {
            source: "ai",
            sentences: [
              {
                text: "At Brook, I led the TypeScript migration.",
                backing: "backed",
                evidence: [{ id: "a2", text: "Led the TypeScript migration.", lines: [4] }],
                note: "",
              },
              {
                text: "I mentored ten engineers.",
                backing: "unbacked",
                evidence: [],
                note: "Nothing in your resume backs this.",
              },
            ],
          },
        ],
      },
    ],
    signOff: "Best,\nSam Rivera",
    source: "ai",
    updatedAt: "2026-10-25T10:00:00Z",
  };
}

export function sampleAnalysis(): JobAnalysis {
  return {
    provider: "claude",
    model: "haiku",
    createdAt: "2026-10-24T10:00:00Z",
    summary: "Strong frontend fit; GraphQL is the main gap.",
    yearsRequired: 5,
    seniority: "senior",
    requirements: [
      {
        text: "React",
        kind: "skill",
        required: true,
        status: "matched",
        evidence: [{ id: "a2", text: "Led the TypeScript migration.", lines: [4] }],
        note: "",
      },
      {
        text: "Design systems",
        kind: "skill",
        required: false,
        status: "worded_differently",
        evidence: [{ id: "a4", text: "Built the component library.", lines: [6] }],
        note: "Call the component library a design system.",
      },
      { text: "GraphQL", kind: "skill", required: true, status: "missing", evidence: [], note: "" },
    ],
  };
}

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
    jobs: [],
    sources: boardSources(),
    lastUpdatedAt: "2026-10-24T09:40:00Z",
    refreshReport: { runs: [] },
    watchlistResult: "GitLab",
    manualJobs: [],
    weights: { skills: 40, experience: 25, role: 20, seniority: 15 },
    ai: { provider: null, consentedAt: null },
    analysis: sampleAnalysis(),
    analyzed: {},
    gap: sampleGap(),
    writer: {
      draft: null,
      options: [
        {
          id: "a2",
          label: "Led the TypeScript migration.",
          detail: "Senior Frontend Engineer · Brook",
        },
        {
          id: "a4",
          label: "Built the component library.",
          detail: "Senior Frontend Engineer · Brook",
        },
      ],
      suggested: ["a2"],
    },
    nextDraft: sampleDraft(),
    calls: [],
    applications: [],
    prep: samplePrep(),
    searches: [{ id: 1, name: "My search", active: true, alerts: true }],
    otherSearches: {},
    alerts: { autoRefresh: true, everyHours: 3, notify: false, minScore: 80, since: null },
    openCheck: {
      status: "open",
      note: "The posting still loads.",
      checkedAt: "2026-10-25T10:00:00Z",
    },
    ...overrides,
  };
  const draftNow = () => {
    if (!db.writer.draft) throw "Write a draft first.";
    return db.writer.draft;
  };
  const fakeInvoke = async (cmd: string, args?: unknown): Promise<unknown> => {
    const a = args as Record<string, never>;
    switch (cmd) {
      case "app_info":
        return info;
      case "get_profile":
        return db.profile;
      case "save_profile":
        db.calls.push("save_profile");
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
      case "get_feed":
        return {
          jobs: db.jobs,
          hiddenByKeywords: 0,
          lastUpdatedAt: db.lastUpdatedAt,
          neverRefreshed: db.lastUpdatedAt === null,
        };
      case "get_job": {
        const j = db.jobs.find((x) => x.id === a.id);
        if (!j) throw "That job is no longer in your feed.";
        return {
          job: j,
          description: `Description of ${j.title}.`,
          tags: [],
          analysis: db.analyzed[j.id] ?? null,
          analysisStale: false,
          match: {
            score: j.score,
            skills: 80,
            experience: 70,
            role: 90,
            seniority: 60,
            matchedSkills: [{ name: "React", lines: [] }],
            missingSkills: ["GraphQL"],
            yearsRequired: 5,
            yearsOnProfile: 7,
            jobSeniority: "Senior",
            wordCount: 90,
            shortDescription: true,
          },
        };
      }
      case "set_job_state":
        db.jobs = db.jobs.map((j) => (j.id === a.id ? { ...j, state: a.jobState } : j));
        return null;
      case "refresh_sources":
        db.lastUpdatedAt = "2026-10-24T10:00:00Z";
        return db.refreshReport;
      case "list_sources":
        return db.sources;
      case "set_source_enabled":
        db.sources = db.sources.map((s) => (s.id === a.id ? { ...s, enabled: a.enabled } : s));
        return null;
      case "add_watchlist_company": {
        if (db.watchlistResult instanceof Error) throw db.watchlistResult.message;
        const added: SourceStatus = {
          id: `greenhouse:${db.watchlistResult.toLowerCase()}`,
          kind: "watchlist",
          name: db.watchlistResult,
          enabled: true,
          lastRunAt: "2026-10-24T10:00:00Z",
          lastOkAt: "2026-10-24T10:00:00Z",
          lastError: null,
          jobCount: 12,
          detail: `Greenhouse · ${db.watchlistResult.toLowerCase()}`,
        };
        db.sources = [...db.sources, added];
        return added;
      }
      case "remove_source":
        db.sources = db.sources.filter((s) => s.id !== a.id);
        return null;
      case "add_manual_job":
        db.manualJobs.push(a.input);
        return 99;
      case "get_weights":
        return db.weights;
      case "save_weights":
        db.weights = a.weights;
        return null;
      case "get_ai_settings":
        return db.ai;
      case "get_ai_overview":
        return {
          settings: db.ai,
          providers: [
            {
              provider: "claude",
              label: "Claude Code",
              installed: true,
              path: "/usr/local/bin/claude",
              version: "2.1.309 (Claude Code)",
              loggedIn: true,
              supported: true,
              note: "Ready.",
            },
            {
              provider: "codex",
              label: "Codex",
              installed: false,
              path: null,
              version: null,
              loggedIn: null,
              supported: false,
              note: "Codex support is coming once we’ve tested it.",
            },
          ],
        };
      case "set_ai_provider":
        db.ai = a.provider
          ? { provider: a.provider, consentedAt: "2026-10-24T10:00:00Z" }
          : { provider: null, consentedAt: null };
        return db.ai;
      case "analyze_job": {
        if (db.analysis instanceof Error) throw db.analysis.message;
        db.analyzed[a.id] = db.analysis;
        const j = db.jobs.find((x) => x.id === a.id);
        if (j) Object.assign(j, { score: 77, aiChecked: true });
        return (await fakeInvoke("get_job", { id: a.id })) as unknown;
      }
      case "cancel_analysis":
      case "cancel_writing":
        return null;
      case "get_gap_check":
        return db.gap;
      case "decide_edit": {
        db.calls.push(`decide_edit:${(a.action as { type: string }).type}`);
        const action = a.action as { type: string; text?: string };
        db.gap = {
          ...db.gap,
          edits: db.gap.edits.map((e) =>
            e.id !== a.editId
              ? e
              : action.type === "undo"
                ? { ...e, decision: null, finalText: "" }
                : {
                    ...e,
                    decision: action.type === "accept" ? "accepted" : "rejected",
                    finalText: action.text ?? "",
                  },
          ),
        };
        return db.gap;
      }
      case "get_writer":
        return db.writer;
      case "write_draft":
        db.calls.push(`write_draft:${a.useAi}`);
        db.writer = { ...db.writer, draft: db.nextDraft };
        return db.nextDraft;
      case "resolve_sentence": {
        const d = structuredClone(draftNow());
        const v = d.paragraphs[a.paragraph].versions[d.paragraphs[a.paragraph].current];
        if (a.keep) v.sentences[a.sentence].backing = "confirmed";
        else v.sentences.splice(a.sentence, 1);
        db.writer = { ...db.writer, draft: d };
        return d;
      }
      case "update_draft_text": {
        const d = { ...draftNow(), subject: a.subject, greeting: a.greeting, signOff: a.signOff };
        db.writer = { ...db.writer, draft: d };
        return d;
      }
      case "list_applications":
        return db.applications;
      case "update_application": {
        const e = a.edit as Partial<Application>;
        db.applications = db.applications.map((x) => (x.jobId === a.id ? { ...x, ...e } : x));
        return db.applications.find((x) => x.jobId === a.id);
      }
      case "mark_applied": {
        db.jobs = db.jobs.map((j) => (j.id === a.id ? { ...j, state: "saved" } : j));
        const app = application({ jobId: a.id, followUpAt: "2026-11-01" });
        db.applications = [app, ...db.applications];
        return app;
      }
      case "mark_followed_up": {
        db.applications = db.applications.map((x) =>
          x.jobId === a.id ? { ...x, followUpAt: "2026-11-01", notes: "Followed up Oct 25." } : x,
        );
        return db.applications.find((x) => x.jobId === a.id);
      }
      case "export_applications_csv":
        db.calls.push("export_applications_csv");
        return "/Users/sam/Downloads/JobLens applications 2026-10-25.csv";
      case "get_data_overview":
        return {
          dataDir: "/data",
          totalBytes: 2048,
          dbBytes: 1024,
          resumeFile: "cv.pdf",
          jobs: db.jobs.length,
          applications: db.applications.length,
          drafts: 0,
        };
      case "create_backup":
        db.calls.push("create_backup");
        return "/Users/sam/Downloads/JobLens backup 2026-10-25.zip";
      case "restore_backup":
        db.calls.push("restore_backup");
        return null;
      case "delete_all_data":
        db.calls.push(`delete_all_data:${a.confirm}`);
        return null;
      case "export_draft":
        db.calls.push(`export_draft:${a.format}`);
        return {
          text: "Dear [hiring manager name],",
          path:
            a.format === "text"
              ? null
              : `/Users/sam/Downloads/Cover letter - Lumen Health.${a.format}`,
        };
      case "get_interview_prep":
        return db.prep;
      case "set_topic_prepared":
        db.calls.push(`set_topic_prepared:${a.key}:${a.prepared}`);
        db.prep = {
          ...db.prep,
          topics: db.prep.topics.map((t) =>
            t.key === a.key ? { ...t, prepared: a.prepared as boolean } : t,
          ),
        };
        return null;
      case "check_still_open":
        return db.openCheck;
      case "list_searches":
        return db.searches;
      case "create_search": {
        const id = Math.max(0, ...db.searches.map((s) => s.id)) + 1;
        const prev = db.searches.find((s) => s.active);
        if (prev) db.otherSearches[prev.id] = db.search;
        db.searches = [
          ...db.searches.map((s) => ({ ...s, active: false })),
          { id, name: (a.name as string).trim(), active: true, alerts: true },
        ];
        db.search = { ...db.search };
        return db.searches;
      }
      case "rename_search":
        db.searches = db.searches.map((s) => (s.id === a.id ? { ...s, name: a.name } : s));
        return db.searches;
      case "delete_search": {
        if (db.searches.length < 2) throw "You need at least one search.";
        const wasActive = db.searches.find((s) => s.id === a.id)?.active;
        db.searches = db.searches.filter((s) => s.id !== a.id);
        if (wasActive) {
          db.searches[0] = { ...db.searches[0], active: true };
          db.search = db.otherSearches[db.searches[0].id] ?? db.search;
        }
        return db.searches;
      }
      case "set_active_search": {
        const prev = db.searches.find((s) => s.active);
        if (prev) db.otherSearches[prev.id] = db.search;
        db.searches = db.searches.map((s) => ({ ...s, active: s.id === a.id }));
        db.search = db.otherSearches[a.id] ?? db.search;
        return db.search;
      }
      case "set_search_alerts":
        db.searches = db.searches.map((s) => (s.id === a.id ? { ...s, alerts: a.on } : s));
        return db.searches;
      case "get_alert_settings":
        return db.alerts;
      case "save_alert_settings": {
        const next = a.settings as AlertSettings;
        db.alerts = {
          ...next,
          since: next.notify ? (db.alerts.since ?? "2026-10-25T10:00:00Z") : null,
        };
        return db.alerts;
      }
      case "send_test_alert":
        db.calls.push("send_test_alert");
        return null;
      default:
        throw new Error(`unexpected command ${cmd}`);
    }
  };
  invoke.mockImplementation(fakeInvoke);
  return db;
}
