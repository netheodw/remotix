import { describe, expect, it } from "vitest";
import { formatSessionDuration } from "./sessionHistory";

describe("formatSessionDuration", () => {
  const start = "2026-11-07T10:00:00Z";

  it("formats seconds, minutes and hours", () => {
    expect(formatSessionDuration(start, "2026-11-07T10:00:38Z")).toBe("38s");
    expect(formatSessionDuration(start, "2026-11-07T10:04:12Z")).toBe("4m 12s");
    expect(formatSessionDuration(start, "2026-11-07T11:05:00Z")).toBe("1h 05m");
  });

  it("is null when the end is unknown or unparsable", () => {
    expect(formatSessionDuration(start, null)).toBeNull();
    expect(formatSessionDuration("garbage", "2026-11-07T10:00:38Z")).toBeNull();
  });
});
