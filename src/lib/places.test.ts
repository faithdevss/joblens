import { describe, expect, it } from "vitest";
import { COUNTRIES, countryFromLocation, localTimezone, TIMEZONES } from "./places";

describe("places", () => {
  it("lists countries by name", () => {
    expect(COUNTRIES.find((c) => c.code === "PT")?.name).toBe("Portugal");
    expect(COUNTRIES.length).toBeGreaterThan(240);
  });

  it("finds the country in a resume location", () => {
    expect(countryFromLocation("Lisbon, Portugal")).toBe("PT");
    expect(countryFromLocation("Austin, TX, USA")).toBe("US");
    expect(countryFromLocation("Remote")).toBe("");
  });

  it("formats the local offset as one of the options", () => {
    const tz = localTimezone(new Date());
    expect(tz).toMatch(/^UTC[+-]\d{2}:\d{2}$/);
    expect(TIMEZONES.find((t) => t.value === "UTC+05:30")?.label).toBe("UTC+05:30");
    expect(TIMEZONES.find((t) => t.value === "UTC-05:00")?.label).toBe("UTC−05:00");
  });
});
