import { describe, expect, it } from "vitest";
import { emptyProfile, emptySearch, guessSeniority, prefillSearch, profileCounts } from "./profile";

describe("profile helpers", () => {
  it("guesses seniority from a title", () => {
    expect(guessSeniority("Senior Frontend Engineer")).toBe("senior");
    expect(guessSeniority("Staff Engineer")).toBe("staff");
    expect(guessSeniority("Engineering Manager")).toBe("lead");
    expect(guessSeniority("Frontend Engineer")).toBeNull();
  });

  it("prefills an empty search from the profile", () => {
    const p = emptyProfile();
    p.contact.headline = "Senior Frontend Engineer";
    p.contact.location = "Lisbon, Portugal";
    const s = prefillSearch(emptySearch(), p);
    expect(s.titles).toEqual(["Senior Frontend Engineer"]);
    expect(s.seniority).toEqual(["senior"]);
    expect(s.country).toBe("PT");
    expect(s.timezone).toMatch(/^UTC/);
  });

  it("never overwrites search fields the user set", () => {
    const p = emptyProfile();
    p.contact.headline = "Senior Frontend Engineer";
    p.contact.location = "Lisbon, Portugal";
    const s = prefillSearch(
      { ...emptySearch(), titles: ["Designer"], country: "BR", timezone: "UTC-03:00" },
      p,
    );
    expect(s.titles).toEqual(["Designer"]);
    expect(s.country).toBe("BR");
    expect(s.timezone).toBe("UTC-03:00");
  });

  it("counts roles, achievements and date flags", () => {
    const p = emptyProfile();
    p.roles = [
      {
        id: "r1",
        title: "Engineer",
        company: "Acme",
        dates: "",
        datesConfirmed: false,
        lines: [],
        achievements: [{ id: "a1", text: "Did a thing", lines: [] }],
      },
    ];
    expect(profileCounts(p)).toEqual({ roles: 1, achievements: 1, skills: 0, flags: 1 });
  });
});
