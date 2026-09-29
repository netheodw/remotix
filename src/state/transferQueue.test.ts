import { describe, expect, it } from "vitest";
import type { TransferEvent } from "../api/types";
import {
  activeCount,
  clearFinished,
  markCanceling,
  transferQueueReducer,
  type TransferItem,
} from "./transferQueue";

const ID = "item-1";

const queued: TransferEvent = {
  type: "queued",
  item_id: ID,
  direction: "download",
  name: "notes.txt",
  source: "/home/nt/notes.txt",
  destination: "D:\\dst\\notes.txt",
};

function reduce(events: TransferEvent[], initial: TransferItem[] = []): TransferItem[] {
  return events.reduce(transferQueueReducer, initial);
}

function only(state: TransferItem[]): TransferItem {
  expect(state).toHaveLength(1);
  return state[0];
}

describe("transferQueueReducer", () => {
  it("walks a transfer from queued to done", () => {
    const item = only(
      reduce([
        queued,
        { type: "planned", item_id: ID, file_count: 1, total_bytes: 100 },
        { type: "started", item_id: ID, file_index: 1, file_count: 1, current_name: "notes.txt" },
        {
          type: "progress",
          item_id: ID,
          transferred_bytes: 60,
          total_bytes: 100,
          file_index: 1,
          file_count: 1,
          bytes_per_sec: 600,
        },
        { type: "completed", item_id: ID, transferred_bytes: 100, file_count: 1, skipped: 0, failed: 0 },
      ]),
    );

    expect(item.status).toBe("done");
    expect(item.transferredBytes).toBe(100);
    expect(item.totalBytes).toBe(100);
    expect(item.bytesPerSec).toBe(0);
  });

  it("ignores an event for an item it has never seen", () => {
    const state = reduce([{ type: "progress", item_id: "ghost", transferred_bytes: 5, total_bytes: 9, file_index: 1, file_count: 1, bytes_per_sec: 1 }]);
    expect(state).toEqual([]);
  });

  it("ignores a duplicate queued event", () => {
    expect(reduce([queued, queued])).toHaveLength(1);
  });

  it("never rewinds the bar when a late progress event arrives", () => {
    const item = only(
      reduce([
        queued,
        { type: "planned", item_id: ID, file_count: 1, total_bytes: 100 },
        { type: "progress", item_id: ID, transferred_bytes: 80, total_bytes: 100, file_index: 1, file_count: 1, bytes_per_sec: 10 },
        { type: "progress", item_id: ID, transferred_bytes: 40, total_bytes: 100, file_index: 1, file_count: 1, bytes_per_sec: 10 },
      ]),
    );
    expect(item.transferredBytes).toBe(80);
  });

  it("does not resurrect a finished row", () => {
    const item = only(
      reduce([
        queued,
        { type: "completed", item_id: ID, transferred_bytes: 10, file_count: 1, skipped: 0, failed: 0 },
        { type: "canceled", item_id: ID },
        { type: "progress", item_id: ID, transferred_bytes: 1, total_bytes: 10, file_index: 1, file_count: 1, bytes_per_sec: 1 },
      ]),
    );
    expect(item.status).toBe("done");
    expect(item.transferredBytes).toBe(10);
  });

  it("lands exactly on the total when a throttled run completes short", () => {
    const item = only(
      reduce([
        queued,
        { type: "planned", item_id: ID, file_count: 1, total_bytes: 1000 },
        { type: "progress", item_id: ID, transferred_bytes: 512, total_bytes: 1000, file_index: 1, file_count: 1, bytes_per_sec: 10 },
        { type: "completed", item_id: ID, transferred_bytes: 1000, file_count: 1, skipped: 0, failed: 0 },
      ]),
    );
    expect(item.transferredBytes).toBe(item.totalBytes);
  });

  it("collects per-file failures without ending the transfer", () => {
    const item = only(
      reduce([
        queued,
        { type: "planned", item_id: ID, file_count: 3, total_bytes: 30 },
        { type: "file_failed", item_id: ID, path: "/a", message: "permission denied" },
        { type: "skipped", item_id: ID, path: "/b", reason: "symbolic link" },
      ]),
    );
    expect(item.status).toBe("running");
    expect(item.failed).toBe(1);
    expect(item.skipped).toBe(1);
    expect(item.errors).toEqual([{ path: "/a", message: "permission denied" }]);
  });

  it("reports a transfer whose every file failed as failed, not done", () => {
    const item = only(
      reduce([
        queued,
        { type: "planned", item_id: ID, file_count: 2, total_bytes: 20 },
        { type: "file_failed", item_id: ID, path: "/a", message: "permission denied" },
        { type: "file_failed", item_id: ID, path: "/b", message: "permission denied" },
        { type: "completed", item_id: ID, transferred_bytes: 0, file_count: 2, skipped: 0, failed: 2 },
      ]),
    );
    expect(item.status).toBe("failed");
  });

  it("keeps a partially failed transfer as done so the successes still count", () => {
    const item = only(
      reduce([
        queued,
        { type: "planned", item_id: ID, file_count: 2, total_bytes: 20 },
        { type: "completed", item_id: ID, transferred_bytes: 10, file_count: 2, skipped: 0, failed: 1 },
      ]),
    );
    expect(item.status).toBe("done");
    expect(item.failed).toBe(1);
  });

  it("records a job-level failure with its message", () => {
    const item = only(reduce([queued, { type: "failed", item_id: ID, message: "connection lost" }]));
    expect(item.status).toBe("failed");
    expect(item.errors[0].message).toBe("connection lost");
  });

  it("keeps a canceling row canceling until the worker confirms", () => {
    const pending = markCanceling(reduce([queued, { type: "planned", item_id: ID, file_count: 1, total_bytes: 9 }]), ID);
    expect(only(pending).status).toBe("canceling");

    const started = transferQueueReducer(pending, {
      type: "started",
      item_id: ID,
      file_index: 1,
      file_count: 1,
      current_name: "notes.txt",
    });
    expect(only(started).status).toBe("canceling");

    const confirmed = transferQueueReducer(started, { type: "canceled", item_id: ID });
    expect(only(confirmed).status).toBe("canceled");
  });

  it("treats idle as a no-op", () => {
    const before = reduce([queued]);
    expect(transferQueueReducer(before, { type: "idle" })).toBe(before);
  });
});

describe("queue helpers", () => {
  const finished = reduce([
    queued,
    { type: "completed", item_id: ID, transferred_bytes: 1, file_count: 1, skipped: 0, failed: 0 },
    { type: "queued", item_id: "item-2", direction: "upload", name: "b.txt", source: "a", destination: "b" },
  ]);

  it("counts only unfinished rows as active", () => {
    expect(activeCount(finished)).toBe(1);
  });

  it("clears finished rows and keeps running ones", () => {
    expect(clearFinished(finished).map((i) => i.itemId)).toEqual(["item-2"]);
  });
});
