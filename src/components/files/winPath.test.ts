import { describe, expect, it } from "vitest";
import { join, parentOf, segments } from "./winPath";

describe("winPath", () => {
  it("joins without doubling the separator at a drive root", () => {
    expect(join("C:\\", "Users")).toBe("C:\\Users");
    expect(join("C:\\Users", "nt")).toBe("C:\\Users\\nt");
    expect(join("C:\\Users\\", "nt")).toBe("C:\\Users\\nt");
  });

  it("stops at a drive root", () => {
    expect(parentOf("C:\\")).toBeNull();
    expect(parentOf("C:\\Users")).toBe("C:\\");
    expect(parentOf("C:\\Users\\nt")).toBe("C:\\Users");
  });

  it("treats a UNC share as a root", () => {
    expect(parentOf("\\\\server\\share")).toBeNull();
    expect(parentOf("\\\\server\\share\\dir")).toBe("\\\\server\\share");
    expect(join("\\\\server\\share", "dir")).toBe("\\\\server\\share\\dir");
  });

  it("falls through to POSIX rules for a POSIX-rooted path", () => {
    expect(parentOf("/")).toBeNull();
    expect(parentOf("/home/nt")).toBe("/home");
    expect(join("/", "home")).toBe("/home");
    expect(join("/home", "nt")).toBe("/home/nt");
  });

  it("builds cumulative breadcrumbs", () => {
    expect(segments("C:\\Users\\nt")).toEqual([
      { label: "C:\\", path: "C:\\" },
      { label: "Users", path: "C:\\Users" },
      { label: "nt", path: "C:\\Users\\nt" },
    ]);
    expect(segments("C:\\")).toEqual([{ label: "C:\\", path: "C:\\" }]);
  });
});
