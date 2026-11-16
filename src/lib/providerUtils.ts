import type { ProviderInfo, ProviderStatus } from "./types";

/** Human label for a connection status. */
export function statusLabel(status: ProviderStatus | string): string {
  switch (status) {
    case "connected":
      return "Connected";
    case "failed":
      return "Connection Failed";
    case "testing":
      return "Testing…";
    case "not_configured":
      return "Add Key";
    case "untested":
    default:
      return "Not Tested";
  }
}

/** Tailwind text color for the status dot indicator. */
export function statusDotClass(status: ProviderStatus | string): string {
  switch (status) {
    case "connected":
      return "bg-ok";
    case "failed":
      return "bg-err";
    case "testing":
      return "bg-warn";
    case "not_configured":
      return "bg-faint";
    case "untested":
    default:
      return "bg-dim";
  }
}

/** Text color used for the status chip label. */
export function statusTextClass(status: ProviderStatus | string): string {
  switch (status) {
    case "connected":
      return "text-ok";
    case "failed":
      return "text-err";
    case "testing":
      return "text-warn";
    default:
      return "text-dim";
  }
}

/** Mask a stored key for display: never reveal more than the last 4 chars. */
export function maskKey(key: string | null | undefined): string {
  const k = (key ?? "").trim();
  if (!k) return "";
  if (k.length <= 8) return "•".repeat(k.length);
  return `${"•".repeat(8)}${k.slice(-4)}`;
}

/** Turn a provider name into a URL-ish slug for id generation hints. */
export function slugify(name: string): string {
  const s = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return s || "provider";
}

/** Validate a custom provider name. Returns an error message or "". */
export function validateCustomName(
  name: string,
  existing: { id: string; name: string }[],
  editingId?: string,
): string {
  const trimmed = name.trim();
  if (!trimmed) return "Provider name is required";
  if (trimmed.length > 40) return "Name is too long (max 40 characters)";
  const dup = existing.find(
    (p) => p.name.toLowerCase() === trimmed.toLowerCase() && p.id !== editingId,
  );
  if (dup) return `A custom provider named "${trimmed}" already exists`;
  return "";
}

/** Validate a base URL. Returns an error message or "" (empty = allowed). */
export function validateBaseUrl(url: string): string {
  const t = url.trim();
  if (!t) return "";
  if (!/^https?:\/\//i.test(t)) return "Base URL must start with http:// or https://";
  return "";
}

/** Merge manually-entered model ids with discovered ones: deduped + sorted. */
export function mergeManualIds(existing: string[], added: string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of [...existing, ...added]) {
    const id = raw.trim();
    if (!id || seen.has(id)) continue;
    seen.add(id);
    out.push(id);
  }
  out.sort((a, b) => a.localeCompare(b));
  return out;
}

/** Filter a model list by a search term (case-insensitive). */
export function filterSearch(models: string[], search: string): string[] {
  const q = search.trim().toLowerCase();
  if (!q) return models;
  return models.filter((m) => m.toLowerCase().includes(q));
}

/** Parse a newline/comma separated manual model entry into ids. */
export function parseManualEntry(text: string): string[] {
  return text
    .split(/[\n,]/)
    .map((s) => s.trim())
    .filter(Boolean);
}

/**
 * Build the composer model menu: groups only enabled models per provider,
 * ordered built-ins first then custom providers.
 */
export function buildModelMenu(providers: ProviderInfo[]): {
  id: string;
  label: string;
  models: string[];
}[] {
  return providers
    .map((p) => ({
      id: p.id,
      label: p.label,
      models: [...(p.config.enabledModels ?? [])],
    }))
    .filter((g) => g.models.length > 0);
}

/**
 * Default model pick: the provider's defaultModel when it is still enabled,
 * else the first enabled model. Returns "" when the provider has none.
 */
export function defaultModelFor(p: ProviderInfo): string {
  const enabled = p.config.enabledModels ?? [];
  if (enabled.length === 0) return "";
  const def = p.config.defaultModel;
  if (def && enabled.includes(def)) return def;
  return enabled[0];
}
