import { describe, expect, it } from "vitest";
import type { PortForward, TermiusForwardPreview, TermiusHostPreview } from "../api/types";
import { defaultSelection, forwardKey, listenerConflicts } from "./termiusImport";

function fwd(name: string, listen_port: number, overrides: Partial<TermiusForwardPreview> = {}): TermiusForwardPreview {
  return {
    name,
    forward_type: "local",
    bind_address: "127.0.0.1",
    listen_port,
    target_hostname: "localhost",
    target_port: 5432,
    problem: null,
    ...overrides,
  };
}

function host(index: number, forwards: TermiusForwardPreview[], overrides: Partial<TermiusHostPreview> = {}): TermiusHostPreview {
  return {
    index,
    name: `Host ${index}`,
    hostname: `10.0.0.${index}`,
    port: 22,
    username: "ops",
    authentication_kind: "password",
    problem: null,
    existing_profile_name: null,
    forwards,
    ...overrides,
  };
}

function existingForward(name: string, listen_port: number): PortForward {
  return {
    id: name,
    port_forward_group_id: null,
    ssh_profile_id: "p",
    name,
    forward_type: "local",
    bind_address: "127.0.0.1",
    listen_port,
    target_hostname: "db",
    target_port: 5432,
    start_on_launch: false,
    sort_order: 0,
    created_at: "",
    updated_at: "",
    notes: "",
    local_hostname: null,
  };
}

describe("defaultSelection", () => {
  it("skips hosts with problems and hosts that already exist", () => {
    const hosts = [host(0, []), host(1, [], { problem: "no password" }), host(2, [], { existing_profile_name: "X" })];
    expect(defaultSelection(hosts)).toEqual([0]);
  });
});

describe("listenerConflicts", () => {
  it("keeps the first selected forward on a port and flags later ones", () => {
    const hosts = [host(0, [fwd("Logs", 10000)]), host(1, [fwd("Redis", 10000), fwd("DB", 10002)])];
    const conflicts = listenerConflicts(hosts, new Set([0, 1]), []);
    expect([...conflicts.keys()]).toEqual([forwardKey(1, 0)]);
    expect(conflicts.get(forwardKey(1, 0))).toContain('"Logs"');
  });

  it("ignores unselected hosts so their ports stay free", () => {
    const hosts = [host(0, [fwd("Logs", 10000)]), host(1, [fwd("Redis", 10000)])];
    expect(listenerConflicts(hosts, new Set([1]), []).size).toBe(0);
  });

  it("flags forwards that collide with an existing forward", () => {
    const conflicts = listenerConflicts([host(0, [fwd("DB", 9000)])], new Set([0]), [existingForward("Old DB", 9000)]);
    expect(conflicts.get(forwardKey(0, 0))).toContain('"Old DB"');
  });

  it("does not treat remote forwards or different bind addresses as conflicts", () => {
    const hosts = [
      host(0, [fwd("Remote", 9000, { forward_type: "remote" }), fwd("Any", 9000, { bind_address: "0.0.0.0" })]),
      host(1, [fwd("Loopback", 9000)]),
    ];
    expect(listenerConflicts(hosts, new Set([0, 1]), []).size).toBe(0);
  });
});
