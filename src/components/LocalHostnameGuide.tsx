import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { useCallback, useEffect, useState } from "react";
import { hostsLookup } from "../api/commands";
import { hostsIp, hostsLine, isLocalhostName, reachableAsLocalhost } from "../screens/localHostname";

const LOOKUP_DEBOUNCE_MS = 300;
const HOSTS_PATH = "C:\\Windows\\System32\\drivers\\etc\\hosts";

/**
 * Walks the user through making a forward reachable by name. Remotix never
 * edits the hosts file itself (antivirus flags an app that does); for a
 * `*.localhost` name nothing is needed, for any other name it shows the line
 * to add and checks (read-only) whether it is in place.
 */
export function LocalHostnameGuide({
  hostname,
  bindAddress,
  port,
}: {
  hostname: string;
  bindAddress: string;
  port: number;
}) {
  const name = hostname.trim().toLowerCase();
  const [mappedTo, setMappedTo] = useState<string | null | undefined>(undefined);
  const [copied, setCopied] = useState<"line" | "path" | null>(null);

  const check = useCallback(() => {
    if (!name || isLocalhostName(name)) return;
    setMappedTo(undefined);
    hostsLookup(name).then(setMappedTo, () => setMappedTo(null));
  }, [name]);

  useEffect(() => {
    const t = setTimeout(check, LOOKUP_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [check]);

  if (!name) {
    return (
      <span className="hint">
        Open the forward by name instead of {bindAddress}. Tip: a name ending in <code>.localhost</code> (e.g.{" "}
        <code>portainer.localhost</code>) works in the browser with no setup.
      </span>
    );
  }

  const url = `http://${name}${port === 80 ? "" : `:${port || "?"}`}`;

  if (isLocalhostName(name)) {
    return reachableAsLocalhost(bindAddress) ? (
      <div className="hostname-guide ok">
        Nothing to set up: Chrome, Edge and Firefox send <code>*.localhost</code> to this computer. Open{" "}
        <code>{url}</code>. (Other programs such as curl may not; use a hosts entry for those.)
      </div>
    ) : (
      <div className="hostname-guide warn">
        Browsers send <code>*.localhost</code> only to 127.0.0.1, so set the bind address to <code>127.0.0.1</code>,
        or use a name such as <code>{name.replace(/\.localhost$/, ".local")}</code> with a hosts entry.
      </div>
    );
  }

  const ip = hostsIp(bindAddress);
  const line = hostsLine(bindAddress, name);
  const copy = (what: "line" | "path", text: string) =>
    writeText(text).then(
      () => setCopied(what),
      () => setCopied(null),
    );

  if (mappedTo === ip) {
    return (
      <div className="hostname-guide ok">
        Ready: the hosts file sends <code>{name}</code> to {ip}. Open <code>{url}</code> once the forward is running.
      </div>
    );
  }

  return (
    <div className="hostname-guide warn">
      {mappedTo ? (
        <p>
          The hosts file sends <code>{name}</code> to {mappedTo}, not to this forward's {ip}. Change that line to the
          one below.
        </p>
      ) : (
        <p>
          One-time setup, so Windows knows <code>{name}</code>. Remotix does not change system files itself.
        </p>
      )}
      <ol>
        <li>
          Start menu → type <strong>Notepad</strong> → right-click → <strong>Run as administrator</strong>.
        </li>
        <li>
          File → Open, paste this path, and set the file type to <strong>All Files</strong>:
          <div className="hostname-guide-copy">
            <code>{HOSTS_PATH}</code>
            <button type="button" className="ghost" onClick={() => void copy("path", HOSTS_PATH)}>
              {copied === "path" ? "Copied" : "Copy"}
            </button>
          </div>
        </li>
        <li>
          Add this line at the end and save:
          <div className="hostname-guide-copy">
            <code>{line.replace("\t", " ")}</code>
            <button type="button" className="ghost" onClick={() => void copy("line", line)}>
              {copied === "line" ? "Copied" : "Copy"}
            </button>
          </div>
        </li>
        <li>
          <button type="button" onClick={check} disabled={mappedTo === undefined}>
            {mappedTo === undefined ? "Checking..." : "Check again"}
          </button>{" "}
          Then open <code>{url}</code>.
        </li>
      </ol>
      {ip === "127.0.0.1" && (
        <p className="hint">
          Tip: give each name its own address (127.0.0.2, 127.0.0.3, ...) in the bind address to reuse the same port
          for different services.
        </p>
      )}
    </div>
  );
}
