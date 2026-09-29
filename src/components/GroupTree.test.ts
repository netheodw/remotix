import { describe, expect, it } from "vitest";
import type { Group } from "../api/types";
import { buildTree, descendantIds, groupOptions } from "./GroupTree";

function group(id: string, parent_id: string | null, name: string, sort_order = 0): Group {
  return { id, parent_id, name, sort_order, created_at: "", updated_at: "" };
}

describe("buildTree", () => {
  it("nests children under their parent and sorts alphabetically within a level", () => {
    const groups = [group("b", null, "Bravo"), group("a", null, "Alpha"), group("c1", "a", "Child")];
    const tree = buildTree(groups, {});
    expect(tree.map((n) => n.id)).toEqual(["a", "b"]);
    expect(tree[0].children.map((n) => n.id)).toEqual(["c1"]);
  });

  it("treats a group whose parent is missing (deleted) as a root", () => {
    const groups = [group("orphan", "missing-parent", "Orphan")];
    const tree = buildTree(groups, {});
    expect(tree.map((n) => n.id)).toEqual(["orphan"]);
  });

  it("attaches item counts by id", () => {
    const groups = [group("a", null, "Alpha")];
    const tree = buildTree(groups, { a: 5 });
    expect(tree[0].itemCount).toBe(5);
  });
});

describe("groupOptions", () => {
  it("lists groups depth-first in tree order with their nesting depth", () => {
    const groups = [
      group("b", null, "Bravo"),
      group("a", null, "Alpha"),
      group("a2", "a", "Zulu"),
      group("a1", "a", "Mike"),
      group("a1x", "a1", "Deep"),
    ];
    expect(groupOptions(groups)).toEqual([
      { id: "a", name: "Alpha", depth: 0 },
      { id: "a1", name: "Mike", depth: 1 },
      { id: "a1x", name: "Deep", depth: 2 },
      { id: "a2", name: "Zulu", depth: 1 },
      { id: "b", name: "Bravo", depth: 0 },
    ]);
  });
});

describe("descendantIds", () => {
  it("includes the group itself and every nested descendant", () => {
    const groups = [
      group("root", null, "Root"),
      group("mid", "root", "Mid"),
      group("leaf", "mid", "Leaf"),
      group("sibling", "root", "Sibling"),
    ];
    const ids = descendantIds(groups, "root");
    expect([...ids].sort()).toEqual(["leaf", "mid", "root", "sibling"]);
  });

  it("does not include unrelated branches", () => {
    const groups = [group("root", null, "Root"), group("mid", "root", "Mid"), group("other", null, "Other")];
    const ids = descendantIds(groups, "mid");
    expect(ids.has("other")).toBe(false);
    expect(ids.has("root")).toBe(false);
  });
});
