import { describe, expect, it } from "vitest";
import { formatBytes } from "./format";

describe("formatBytes", () => {
  it("keeps small values in bytes", () => {
    expect(formatBytes(512)).toBe("512 B");
  });

  it("scales to KB and MB", () => {
    expect(formatBytes(2048)).toBe("2.0 KB");
    expect(formatBytes(4.2 * 1024 * 1024)).toBe("4.2 MB");
  });
});
