import type { Snippet } from "../api/types";

/** `{{name}}`, with optional spaces inside the braces. */
const PLACEHOLDER = /\{\{\s*([^{}]+?)\s*\}\}/g;

/** Placeholder names in the order they first appear, without duplicates. */
export function snippetPlaceholders(command: string): string[] {
  const names: string[] = [];
  for (const match of command.matchAll(PLACEHOLDER)) {
    if (!names.includes(match[1])) names.push(match[1]);
  }
  return names;
}

/** Substitutes every placeholder; ones without a value are left as typed. */
export function fillPlaceholders(command: string, values: Record<string, string>): string {
  return command.replace(PLACEHOLDER, (whole, name: string) => (values[name] ? values[name] : whole));
}

/** Case-insensitive match on name, command, or description; every word must match. */
export function filterSnippets(snippets: Snippet[], query: string): Snippet[] {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return snippets;
  return snippets.filter((s) => {
    const haystack = `${s.name}\n${s.command}\n${s.description}`.toLowerCase();
    return words.every((w) => haystack.includes(w));
  });
}
