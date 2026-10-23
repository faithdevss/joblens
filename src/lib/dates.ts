// Checks on free-text role dates ("Mar 2021 – Present") for the "Looks right" flag.
import type { Role } from "../bindings/Role";

type Point = { year: number; month: number | null };
type Range = { start: Point; end: Point | "present" };

const MONTHS = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

function parsePoint(raw: string): Point | "present" | null {
  const s = raw.trim().toLowerCase().replace(/\.$/, "");
  if (/^(present|current|now|today)$/.test(s)) return "present";
  let m = s.match(/^([a-z]{3,9})\.?\s*'?(\d{2}|\d{4})$/);
  if (m) {
    const month = MONTHS.indexOf(m[1].slice(0, 3));
    if (month < 0) return null;
    const year = m[2].length === 2 ? 2000 + Number(m[2]) : Number(m[2]);
    return { year, month: month + 1 };
  }
  m = s.match(/^(\d{1,2})\/(\d{4})$/) ?? s.match(/^(\d{4})-(\d{2})$/);
  if (m) {
    const [year, month] = m[1].length === 4 ? [m[1], m[2]] : [m[2], m[1]];
    if (Number(month) < 1 || Number(month) > 12) return null;
    return { year: Number(year), month: Number(month) };
  }
  if (/^\d{4}$/.test(s)) return { year: Number(s), month: null };
  return null;
}

export function parseRange(text: string): Range | null {
  // A bare hyphen splits "2018-2021" but not the single point "2019-05".
  const parts = text
    .trim()
    .split(/\s*[–—]\s*|\s+-\s+|\s+(?:to|until)\s+|(?<=\d{4})-(?!\d{2}\b)/i)
    .filter(Boolean);
  if (parts.length !== 2) return null;
  const start = parsePoint(parts[0]);
  const end = parsePoint(parts[1]);
  if (!start || start === "present" || !end) return null;
  return { start, end };
}

const asNumber = (p: Point) => p.year * 12 + (p.month ?? 1);

/** A short warning for the role's dates, or null when they look fine or were confirmed. */
export function dateFlag(role: Role, allRoles: Role[], today = new Date()): string | null {
  if (role.datesConfirmed) return null;
  if (!role.dates.trim()) return "No dates yet. Add when you started and finished.";
  const range = parseRange(role.dates);
  if (!range) return "We couldn't read these dates. Try a format like “Mar 2021 – Present”.";
  const now = { year: today.getFullYear(), month: today.getMonth() + 1 };
  if (asNumber(range.start) > asNumber(now)) return "Start date is in the future.";
  if (range.end !== "present" && asNumber(range.end) < asNumber(range.start)) {
    return "End date is before the start date.";
  }
  if (range.start.month === null) {
    const withMonths = allRoles.find((r) => r.id !== role.id && parseRange(r.dates)?.start.month);
    if (withMonths) {
      return `Dates use a different format than your other roles (“${role.dates.trim()}” vs “${withMonths.dates.trim()}”). Add months if you know them.`;
    }
  }
  return null;
}
