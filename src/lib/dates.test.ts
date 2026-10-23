import { describe, expect, it } from "vitest";
import type { Role } from "../bindings/Role";
import { dateFlag, parseRange } from "./dates";

const role = (id: string, dates: string, datesConfirmed = false): Role => ({
  id,
  title: "Engineer",
  company: "Acme",
  dates,
  datesConfirmed,
  achievements: [],
  lines: [],
});

const today = new Date(2026, 9, 23);

describe("parseRange", () => {
  it("reads common formats", () => {
    expect(parseRange("Mar 2021 – Present")).toEqual({
      start: { year: 2021, month: 3 },
      end: "present",
    });
    expect(parseRange("06/2017 to 12/2019")).toEqual({
      start: { year: 2017, month: 6 },
      end: { year: 2019, month: 12 },
    });
    expect(parseRange("2018 - 2021")).toEqual({
      start: { year: 2018, month: null },
      end: { year: 2021, month: null },
    });
    expect(parseRange("September 2019 — Jan 2020")?.start).toEqual({ year: 2019, month: 9 });
  });

  it("rejects text it can't read", () => {
    expect(parseRange("a while ago")).toBeNull();
    expect(parseRange("2021")).toBeNull();
    expect(parseRange("13/2020 - 2021")).toBeNull();
  });
});

describe("dateFlag", () => {
  it("accepts good dates", () => {
    const r = role("a", "Mar 2021 – Present");
    expect(dateFlag(r, [r], today)).toBeNull();
  });

  it("flags missing, unreadable, reversed and future dates", () => {
    expect(dateFlag(role("a", ""), [], today)).toMatch(/No dates/);
    expect(dateFlag(role("a", "ages ago"), [], today)).toMatch(/couldn't read/);
    expect(dateFlag(role("a", "2021 - 2019"), [], today)).toMatch(/before the start/);
    expect(dateFlag(role("a", "Jan 2027 - Present"), [], today)).toMatch(/future/);
  });

  it("flags year-only dates when other roles have months", () => {
    const a = role("a", "Mar 2021 – Present");
    const b = role("b", "2018 – 2021");
    expect(dateFlag(b, [a, b], today)).toBe(
      "Dates use a different format than your other roles (“2018 – 2021” vs “Mar 2021 – Present”). Add months if you know them.",
    );
    expect(dateFlag(b, [b], today)).toBeNull();
  });

  it("stays quiet once the user confirms", () => {
    expect(dateFlag(role("a", "ages ago", true), [], today)).toBeNull();
  });
});
