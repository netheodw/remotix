import { describe, expect, it, vi } from "vitest";
import { dragPayload, moveItems } from "./MoveToGroup";

describe("dragPayload", () => {
  it("carries the whole selection when the dragged row is selected", () => {
    expect(dragPayload("b", new Set(["a", "b", "c"]))).toEqual(["a", "b", "c"]);
  });

  it("carries only the dragged row when it is not part of the selection", () => {
    expect(dragPayload("z", new Set(["a", "b"]))).toEqual(["z"]);
  });
});

describe("moveItems", () => {
  const groups: Record<string, string | null> = { a: null, b: "g1", c: "g2" };

  it("moves only items that are not already in the target group, in order", async () => {
    const move = vi.fn().mockResolvedValue(undefined);
    await moveItems(["a", "b", "c"], "g1", (id) => groups[id], move);
    expect(move.mock.calls).toEqual([
      ["a", "g1"],
      ["c", "g1"],
    ]);
  });

  it("can move items out of every group", async () => {
    const move = vi.fn().mockResolvedValue(undefined);
    await moveItems(["a", "b"], null, (id) => groups[id], move);
    expect(move.mock.calls).toEqual([["b", null]]);
  });

  it("stops at the first failure and leaves the remaining items alone", async () => {
    const move = vi.fn().mockRejectedValueOnce(new Error("not found")).mockResolvedValue(undefined);
    await expect(moveItems(["a", "c"], "g1", (id) => groups[id], move)).rejects.toThrow("not found");
    expect(move).toHaveBeenCalledTimes(1);
  });
});
