import { describe, expect, it } from "vitest";
import {
  buildModelMenu,
  defaultModelFor,
  filterSearch,
  maskKey,
  mergeManualIds,
  parseManualEntry,
  slugify,
  statusDotClass,
  statusLabel,
  statusTextClass,
  validateBaseUrl,
  validateCustomName,
} from "../providerUtils";
import type { ProviderInfo } from "../types";

const p = (over: Partial<ProviderInfo>): ProviderInfo => ({
  id: "groq",
  label: "Groq",
  format: "openai",
  baseUrl: "",
  helpUrl: "",
  custom: false,
  status: "connected",
  message: "",
  config: {},
  ...over,
});

describe("statusLabel", () => {
  it("maps every status to a human label", () => {
    expect(statusLabel("connected")).toBe("Connected");
    expect(statusLabel("failed")).toBe("Connection Failed");
    expect(statusLabel("testing")).toBe("Testing…");
    expect(statusLabel("not_configured")).toBe("Add Key");
    expect(statusLabel("untested")).toBe("Not Tested");
    expect(statusLabel("unknown")).toBe("Not Tested");
  });
});

describe("status colors", () => {
  it("uses green/red/yellow/gray indicators", () => {
    expect(statusDotClass("connected")).toBe("bg-ok");
    expect(statusDotClass("failed")).toBe("bg-err");
    expect(statusDotClass("testing")).toBe("bg-warn");
    expect(statusDotClass("not_configured")).toBe("bg-faint");
    expect(statusDotClass("untested")).toBe("bg-dim");
    expect(statusTextClass("connected")).toBe("text-ok");
    expect(statusTextClass("failed")).toBe("text-err");
  });
});

describe("maskKey", () => {
  it("never reveals more than the last 4 characters", () => {
    expect(maskKey("gsk_abcdefgh1234")).toBe("••••••••1234");
    expect(maskKey("short")).toBe("•••••");
    expect(maskKey("")).toBe("");
    expect(maskKey(null)).toBe("");
  });
});

describe("validateCustomName", () => {
  const existing = [
    { id: "custom-qwen", name: "Qwen" },
    { id: "custom-deep", name: "DeepSeek" },
  ];
  it("rejects empty and overlong names", () => {
    expect(validateCustomName("  ", existing)).not.toBe("");
    expect(validateCustomName("x".repeat(41), existing)).toContain("too long");
  });
  it("rejects duplicates case-insensitively", () => {
    expect(validateCustomName("qwen", existing)).toContain("already exists");
    expect(validateCustomName("QWEN", existing)).toContain("already exists");
  });
  it("allows the same name when editing that provider", () => {
    expect(validateCustomName("Qwen", existing, "custom-qwen")).toBe("");
  });
  it("accepts fresh names", () => {
    expect(validateCustomName("My AI", existing)).toBe("");
  });
});

describe("validateBaseUrl", () => {
  it("allows empty and http(s) urls", () => {
    expect(validateBaseUrl("")).toBe("");
    expect(validateBaseUrl("https://api.x.dev/v1")).toBe("");
    expect(validateBaseUrl("http://localhost:11434/v1")).toBe("");
  });
  it("rejects missing scheme", () => {
    expect(validateBaseUrl("api.x.dev/v1")).toContain("http");
    expect(validateBaseUrl("ftp://x")).toContain("http");
  });
});

describe("mergeManualIds", () => {
  it("dedupes, trims, drops empties and sorts", () => {
    expect(mergeManualIds(["b", " a ", ""], ["b", "c"])).toEqual(["a", "b", "c"]);
  });
});

describe("filterSearch", () => {
  it("is case-insensitive and trims", () => {
    const models = ["GPT-4.1", "llama-3.3-70b", "o3-mini"];
    expect(filterSearch(models, "gpt")).toEqual(["GPT-4.1"]);
    expect(filterSearch(models, "  ")).toEqual(models);
  });
});

describe("parseManualEntry", () => {
  it("splits on newlines and commas", () => {
    expect(parseManualEntry("a\nb, c")).toEqual(["a", "b", "c"]);
    expect(parseManualEntry("  \n ")).toEqual([]);
  });
});

describe("buildModelMenu", () => {
  it("only includes providers with enabled models", () => {
    const providers = [
      p({ id: "groq", label: "Groq", config: { enabledModels: ["gpt-oss-120b"] } }),
      p({ id: "openai", label: "OpenAI", status: "connected", config: { enabledModels: [] } }),
      p({
        id: "custom-qwen",
        label: "Qwen",
        custom: true,
        config: { enabledModels: ["qwen3-32b"] },
      }),
    ];
    const menu = buildModelMenu(providers);
    expect(menu.map((g) => g.label)).toEqual(["Groq", "Qwen"]);
    expect(menu[0].models).toEqual(["gpt-oss-120b"]);
  });
});

describe("defaultModelFor", () => {
  it("keeps the default when still enabled, else first enabled", () => {
    expect(defaultModelFor(p({ config: { enabledModels: ["a", "b"], defaultModel: "b" } }))).toBe("b");
    expect(defaultModelFor(p({ config: { enabledModels: ["a", "b"], defaultModel: "gone" } }))).toBe("a");
    expect(defaultModelFor(p({ config: { enabledModels: [] } }))).toBe("");
  });
});

describe("slugify", () => {
  it("makes url-safe slugs", () => {
    expect(slugify("My AI Provider")).toBe("my-ai-provider");
    expect(slugify("  --deep--seek-- ")).toBe("deep-seek");
    expect(slugify("###")).toBe("provider");
  });
});
