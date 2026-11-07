/**
 * Local hostnames of port forwards. Remotix never edits the hosts file (an
 * app that elevates itself to write it gets flagged by antivirus); these
 * helpers tell the user what to add by hand, or that nothing is needed.
 */

/** The address a hostname should map to: the bind address, or loopback when listening on every interface. */
export function hostsIp(bindAddress: string): string {
  const b = bindAddress.trim();
  if (b === "0.0.0.0") return "127.0.0.1";
  if (b === "::" || b === "[::]") return "::1";
  return b;
}

/** `*.localhost` resolves to 127.0.0.1 in Chrome, Edge and Firefox with no hosts entry. */
export function isLocalhostName(hostname: string): boolean {
  return hostname.trim().toLowerCase().endsWith(".localhost");
}

/** Bind addresses a `*.localhost` name reaches (browsers always use 127.0.0.1 / ::1). */
export function reachableAsLocalhost(bindAddress: string): boolean {
  return ["127.0.0.1", "0.0.0.0", "::1", "::", "[::]"].includes(bindAddress.trim());
}

/** The line to add to the hosts file. */
export function hostsLine(bindAddress: string, hostname: string): string {
  return `${hostsIp(bindAddress)}\t${hostname.trim().toLowerCase()}`;
}
