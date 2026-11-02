import { describe, expect, it } from "vitest";
import type { Snippet } from "../api/types";
import { fillPlaceholders, filterSnippets, snippetPlaceholders } from "./toolbox";

function snippet(name: string, command: string, description = ""): Snippet {
  return { id: name, category_id: "c", name, command, description, sort_order: 0, created_at: "", updated_at: "" };
}

describe("snippetPlaceholders", () => {
  it("lists unique names in order of appearance", () => {
    expect(snippetPlaceholders("chown -R {{user}}:{{ group }} {{path}} # {{user}}")).toEqual(["user", "group", "path"]);
  });

  it("ignores single braces like find -exec {}", () => {
    expect(snippetPlaceholders("find . -exec ls {} \;")).toEqual([]);
  });
});

describe("fillPlaceholders", () => {
  it("replaces every occurrence and keeps unfilled ones", () => {
    expect(fillPlaceholders("{{a}} {{ b }} {{a}}", { a: "x" })).toBe("x {{ b }} x");
  });
});

describe("filterSnippets", () => {
  const list = [snippet("Disk usage", "df -h", "Free space"), snippet("Ping", "ping -c 4 {{host}}")];

  it("returns everything for a blank query", () => {
    expect(filterSnippets(list, "  ")).toHaveLength(2);
  });

  it("requires every word to match somewhere", () => {
    expect(filterSnippets(list, "free DF").map((s) => s.name)).toEqual(["Disk usage"]);
    expect(filterSnippets(list, "ping disk")).toEqual([]);
  });
});
