import type { ForwardType, PortForward, TermiusForwardPreview, TermiusHostPreview } from "../api/types";

export const forwardKey = (hostIndex: number, forwardIndex: number) => `${hostIndex}:${forwardIndex}`;

/** Hosts that can be imported and don't already exist as a profile. */
export function defaultSelection(hosts: TermiusHostPreview[]): number[] {
  return hosts.filter((h) => !h.problem && !h.existing_profile_name).map((h) => h.index);
}

const listensLocally = (type: ForwardType | null) => type === "local" || type === "dynamic";

/**
 * Mirrors the backend rule that only one local/dynamic forward may listen on
 * a given bind address + port, walking existing forwards and then the
 * selected hosts in the order the import creates them. Returns the reason
 * each conflicting forward will be skipped, keyed by `forwardKey`.
 */
export function listenerConflicts(
  hosts: TermiusHostPreview[],
  selected: ReadonlySet<number>,
  existing: PortForward[],
): Map<string, string> {
  const owners = new Map<string, string>();
  existing.forEach((f) => {
    if (listensLocally(f.forward_type)) owners.set(`${f.bind_address}:${f.listen_port}`, f.name);
  });

  const conflicts = new Map<string, string>();
  for (const host of hosts) {
    if (!selected.has(host.index) || host.problem) continue;
    host.forwards.forEach((f, i) => {
      if (f.problem || !listensLocally(f.forward_type)) return;
      const endpoint = `${f.bind_address}:${f.listen_port}`;
      const owner = owners.get(endpoint);
      if (owner === undefined) owners.set(endpoint, f.name);
      else conflicts.set(forwardKey(host.index, i), `${endpoint} is already used by "${owner}"`);
    });
  }
  return conflicts;
}

export function describeForward(f: TermiusForwardPreview): string {
  if (f.forward_type === "dynamic") return `D ${f.bind_address}:${f.listen_port} (SOCKS5)`;
  if (f.forward_type === "remote") return `R remote:${f.listen_port} → ${f.target_hostname}:${f.target_port}`;
  return `L ${f.bind_address}:${f.listen_port} → ${f.target_hostname}:${f.target_port}`;
}
