import { describe, expect, it } from "vitest";
import { normalizeHttpUrl, safeLinkHref } from "../safeUrl";

describe("normalizeHttpUrl", () => {
  it("keeps explicit http and https urls", () => {
    expect(normalizeHttpUrl("https://example.com/docs")).toBe("https://example.com/docs");
    expect(normalizeHttpUrl("http://localhost:5173")).toBe("http://localhost:5173/");
  });

  it("adds https to bare hosts and localhost", () => {
    expect(normalizeHttpUrl("example.com")).toBe("https://example.com/");
    expect(normalizeHttpUrl("localhost:3000")).toBe("https://localhost:3000/");
    expect(normalizeHttpUrl("127.0.0.1:8080/app")).toBe("https://127.0.0.1:8080/app");
  });

  it("rejects every non-http scheme", () => {
    for (const bad of [
      "javascript:alert(1)",
      "file:///C:/Windows/System32",
      "data:text/html,<script>alert(1)</script>",
      "mailto:test@example.com",
      "//evil.example.com",
      "not a url",
    ]) {
      expect(normalizeHttpUrl(bad), bad).toBeNull();
    }
  });
});

describe("safeLinkHref", () => {
  it("allows http, https and mailto", () => {
    expect(safeLinkHref("https://example.com")).toBe("https://example.com/");
    expect(safeLinkHref("mailto:hello@example.com")).toBe("mailto:hello@example.com");
  });

  it("rejects unsafe markdown targets", () => {
    for (const bad of [
      "javascript:alert(1)",
      "file:///etc/passwd",
      "data:text/html,hi",
      "C:\\Users\\me\\secret.txt",
      "#section",
      undefined,
    ]) {
      expect(safeLinkHref(bad as string | undefined)).toBeNull();
    }
  });
});
