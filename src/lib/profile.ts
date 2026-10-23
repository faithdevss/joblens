import type { Profile } from "../bindings/Profile";
import type { SearchSettings } from "../bindings/SearchSettings";
import type { Seniority } from "../bindings/Seniority";
import { dateFlag } from "./dates";
import { countryFromLocation, localTimezone } from "./places";

export function emptyProfile(): Profile {
  return {
    contact: {
      name: "",
      headline: "",
      email: "",
      phone: "",
      location: "",
      website: "",
      github: "",
      linkedin: "",
      lines: [],
    },
    summary: "",
    summaryLines: [],
    roles: [],
    skills: [],
    education: [],
    projects: [],
    languages: [],
    certifications: [],
  };
}

export function emptySearch(): SearchSettings {
  return {
    titles: [],
    mustHave: [],
    avoid: [],
    country: "",
    timezone: "",
    minSalary: null,
    currency: "USD",
    seniority: [],
  };
}

/** Id for an item added by hand. Parsed items use short counters ("r1", "a2"). */
export function newId(prefix: string): string {
  return `${prefix}-${crypto.randomUUID().slice(0, 8)}`;
}

export function profileCounts(p: Profile) {
  return {
    roles: p.roles.length,
    achievements: p.roles.reduce((n, r) => n + r.achievements.length, 0),
    skills: p.skills.length,
    flags: p.roles.filter((r) => dateFlag(r, p.roles) !== null).length,
  };
}

export function plural(n: number, word: string, many = `${word}s`): string {
  return `${n} ${n === 1 ? word : many}`;
}

export const SENIORITY: ReadonlyArray<{ value: Seniority; label: string }> = [
  { value: "junior", label: "Junior" },
  { value: "mid", label: "Mid" },
  { value: "senior", label: "Senior" },
  { value: "staff", label: "Staff" },
  { value: "lead", label: "Lead" },
];

export function guessSeniority(title: string): Seniority | null {
  const t = title.toLowerCase();
  if (/\b(staff|principal)\b/.test(t)) return "staff";
  if (/\b(lead|head|manager|director)\b/.test(t)) return "lead";
  if (/\b(senior|sr\.?)\b/.test(t)) return "senior";
  if (/\b(junior|jr\.?|intern|graduate|trainee)\b/.test(t)) return "junior";
  return null;
}

/** Fills empty search fields from the profile. Never overwrites what the user set. */
export function prefillSearch(search: SearchSettings, profile: Profile): SearchSettings {
  const next = { ...search };
  if (next.titles.length === 0) {
    const titles = [profile.contact.headline, profile.roles[0]?.title]
      .map((t) => t?.trim() ?? "")
      .filter(Boolean);
    next.titles = [...new Set(titles)];
  }
  if (next.seniority.length === 0) {
    const level = guessSeniority(next.titles[0] ?? "");
    if (level) next.seniority = [level];
  }
  if (!next.country) next.country = countryFromLocation(profile.contact.location);
  if (!next.timezone) next.timezone = localTimezone();
  return next;
}
