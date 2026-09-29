import { describe, expect, it } from "vitest";
import { join, parentOf, segments } from "./posixPath";

describe("posixPath", () => {
  it("joins without doubling the separator", () => {
    expect(join("/", "a")).toBe("/a");
    expect(join("/home/user", "notes.txt")).toBe("/home/user/notes.txt");
    expect(join("/home/user/", "notes.txt")).toBe("/home/user/notes.txt");
    expect(join("", "a")).toBe("/a");
  });

  it("keeps names containing characters that are special elsewhere", () => {
    expect(join("/srv", "with space #1")).toBe("/srv/with space #1");
  });

  it("stops at the root", () => {
    expect(parentOf("/")).toBeNull();
    expect(parentOf("/a")).toBe("/");
    expect(parentOf("/a/b")).toBe("/a");
    expect(parentOf("/a/b/")).toBe("/a");
  });

  it("builds cumulative breadcrumbs from the root", () => {
    expect(segments("/a/b")).toEqual([
      { label: "/", path: "/" },
      { label: "a", path: "/a" },
      { label: "b", path: "/a/b" },
    ]);
    expect(segments("/")).toEqual([{ label: "/", path: "/" }]);
  });
});
