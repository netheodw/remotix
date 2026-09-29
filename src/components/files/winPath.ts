// Path arithmetic for the local pane. Handles drive letters (C:\), UNC shares
// (\\server\share) and POSIX roots, so the same pane works on either platform.

const DRIVE = /^[A-Za-z]:/;

/** The part of a path that has no parent: `C:\`, `\\server\share` or `/`. */
function rootOf(path: string): string | null {
  if (path.startsWith("\\\\") || path.startsWith("//")) {
    const parts = path.slice(2).split(/[\\/]/).filter(Boolean);
    if (parts.length < 2) return null;
    return `\\\\${parts[0]}\\${parts[1]}`;
  }
  if (DRIVE.test(path)) return `${path.slice(0, 2)}\\`;
  if (path.startsWith("/")) return "/";
  return null;
}

function separatorOf(root: string): string {
  return root === "/" ? "/" : "\\";
}

function partsOf(path: string, root: string): string[] {
  return path.slice(root.length).split(/[\\/]/).filter(Boolean);
}

function build(root: string, parts: string[]): string {
  if (parts.length === 0) return root;
  const separator = separatorOf(root);
  const body = parts.join(separator);
  return root.endsWith(separator) ? `${root}${body}` : `${root}${separator}${body}`;
}

export function join(dir: string, name: string): string {
  const root = rootOf(dir);
  if (root === null) return `${dir}\\${name}`;
  return build(root, [...partsOf(dir, root), name]);
}

export function parentOf(path: string): string | null {
  const root = rootOf(path);
  if (root === null) return null;
  const parts = partsOf(path, root);
  if (parts.length === 0) return null;
  return build(root, parts.slice(0, -1));
}

export function segments(path: string): { label: string; path: string }[] {
  const root = rootOf(path);
  if (root === null) return [{ label: path, path }];
  const crumbs = [{ label: root, path: root }];
  const parts = partsOf(path, root);
  for (let i = 0; i < parts.length; i++) {
    crumbs.push({ label: parts[i], path: build(root, parts.slice(0, i + 1)) });
  }
  return crumbs;
}
