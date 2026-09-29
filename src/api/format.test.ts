import { describe, expect, it } from "vitest";
import { formatBytes, formatEta, formatModified, formatSpeed } from "./format";

describe("formatBytes", () => {
  it("keeps whole bytes without a decimal", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
  });

  it("steps up through binary units", () => {
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1024 ** 3)).toBe("1.0 GB");
    expect(formatBytes(1024 ** 4)).toBe("1.0 TB");
  });

  it("drops the decimal once the value is large within its unit", () => {
    expect(formatBytes(150 * 1024)).toBe("150 KB");
  });

  it("returns a placeholder for a nonsense size", () => {
    expect(formatBytes(-1)).toBe("—");
    expect(formatBytes(Number.NaN)).toBe("—");
  });
});

describe("formatSpeed", () => {
  it("suffixes a rate", () => {
    expect(formatSpeed(1024)).toBe("1.0 KB/s");
  });

  it("returns a placeholder when there is no measurable rate", () => {
    expect(formatSpeed(0)).toBe("—");
    expect(formatSpeed(Number.POSITIVE_INFINITY)).toBe("—");
  });
});

describe("formatEta", () => {
  it("scales the unit to the duration", () => {
    expect(formatEta(45)).toBe("45s");
    expect(formatEta(90)).toBe("1m 30s");
    expect(formatEta(3700)).toBe("1h 1m");
  });

  it("returns a placeholder for an unknown remaining time", () => {
    expect(formatEta(Number.POSITIVE_INFINITY)).toBe("—");
    expect(formatEta(Number.NaN)).toBe("—");
    expect(formatEta(-1)).toBe("—");
  });
});

describe("formatModified", () => {
  it("treats null and a zero timestamp as unknown, not 1970", () => {
    expect(formatModified(null)).toBe("—");
    expect(formatModified(0)).toBe("—");
  });

  it("formats a real timestamp", () => {
    expect(formatModified(1700000000)).not.toBe("—");
  });
});
