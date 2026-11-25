const SCHEME_RE = /^[a-zA-Z][a-zA-Z0-9+.-]*:/;

/**
 * Normalize user-entered browser URLs. Only http/https survive; everything
 * else (file:, javascript:, data:, UNC paths, malformed input) returns null.
 */
export function normalizeHttpUrl(input: string): string | null {
  const raw = input.trim();
  if (!raw) return null;

  let candidate = raw;
  // "localhost:3000" looks like a URI scheme to the URL parser; treat
  // host:port forms as hosts before checking for an explicit scheme.
  const hostWithPort = /^[\w.-]+:\d{1,5}(\/.*)?$/.test(raw);
  if (!SCHEME_RE.test(raw) || hostWithPort) {
    // Accept localhost, IPv4/IPv6 literals and plain host names with an
    // optional port and path, then default them to https.
    if (!/^(localhost|\[[0-9a-fA-F:]+\]|[\w.-]+)(:\d{1,5})?(\/.*)?$/i.test(raw)) return null;
    if (/^[\w.-]+$/.test(raw) && !raw.includes(".") && !/^localhost$/i.test(raw)) {
      // A single word is more likely a search term than a host.
      return null;
    }
    candidate = `https://${raw}`;
  }

  let url: URL;
  try {
    url = new URL(candidate);
  } catch {
    return null;
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") return null;
  if (!url.hostname) return null;
  return url.toString();
}

/** Safe href for links rendered from model Markdown. */
export function safeLinkHref(href?: string): string | null {
  const raw = href?.trim() ?? "";
  if (!raw || /[\u0000-\u0020]/.test(raw)) return null;
  if (/^mailto:/i.test(raw)) return raw;
  return normalizeHttpUrl(raw);
}
