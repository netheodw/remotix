// Path arithmetic for the remote pane. Remote paths are always POSIX,
// whatever this client runs on.

export function join(dir: string, name: string): string {
  const base = dir.replace(/\/+$/, "");
  return base === "" ? `/${name}` : `${base}/${name}`;
}

export function parentOf(path: string): string | null {
  const trimmed = path.replace(/\/+$/, "");
  if (trimmed === "") return null;
  const index = trimmed.lastIndexOf("/");
  if (index < 0) return null;
  return index === 0 ? "/" : trimmed.slice(0, index);
}

export function segments(path: string): { label: string; path: string }[] {
  const crumbs = [{ label: "/", path: "/" }];
  let current = "";
  for (const part of path.split("/").filter(Boolean)) {
    current = `${current}/${part}`;
    crumbs.push({ label: part, path: current });
  }
  return crumbs;
}
