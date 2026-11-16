// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ProviderInfo } from "../../../lib/types";
import AddKeyModal from "../AddKeyModal";
import SelectModelsModal from "../SelectModelsModal";
import CustomProviderModal from "../CustomProviderModal";

vi.mock(import("../../../lib/tauri"), async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../../lib/tauri")>();
  return {
    ...actual,
    api: {
      providerSetKey: vi.fn().mockResolvedValue(undefined),
      providerTest: vi.fn(),
      providerListModels: vi.fn(),
      providerSetConfig: vi.fn().mockResolvedValue(undefined),
      providerStatus: vi.fn().mockResolvedValue([]),
      customProviderSave: vi.fn(),
      customProviderDelete: vi.fn().mockResolvedValue(undefined),
    },
  };
});

import { api } from "../../../lib/tauri";

const groq: ProviderInfo = {
  id: "groq",
  label: "Groq",
  format: "openai",
  baseUrl: "https://api.groq.com/openai/v1",
  helpUrl: "https://console.groq.com/keys",
  custom: false,
  status: "not_configured",
  message: "",
  config: {},
};

const qwen: ProviderInfo = {
  id: "custom-qwen",
  label: "Qwen",
  format: "openai",
  baseUrl: "https://api.qwen.example/v1",
  helpUrl: "",
  custom: true,
  status: "untested",
  message: "",
  config: { enabledModels: ["qwen3-32b"], manualModels: ["qwen3-32b"] },
};

beforeEach(() => {
  vi.mocked(api.providerTest).mockResolvedValue({
    ok: true,
    status: "connected",
    message: "Connected — 2 models available",
    models: ["a", "b"],
  });
  vi.mocked(api.providerListModels).mockResolvedValue([
    "openai/gpt-oss-120b",
    "openai/gpt-oss-20b",
    "llama-3.3-70b-versatile",
  ]);
  vi.mocked(api.customProviderSave).mockResolvedValue(qwen);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("AddKeyModal", () => {
  it("disables Save until a key is entered", () => {
    render(<AddKeyModal provider={groq} open onClose={() => {}} />);
    const save = screen.getByRole("button", { name: /save/i }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    const input = document.getElementById("addkey-input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "gsk_test_key" } });
    expect(save.disabled).toBe(false);
  });

  it("saves the key and runs a real connection test", async () => {
    const onClose = vi.fn();
    render(<AddKeyModal provider={groq} open onClose={onClose} />);
    const input = document.getElementById("addkey-input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "gsk_test_key" } });
    fireEvent.click(screen.getByRole("button", { name: /save/i }));
    await waitFor(() => expect(api.providerSetKey).toHaveBeenCalledWith("groq", "gsk_test_key"));
    await waitFor(() => expect(api.providerTest).toHaveBeenCalledWith("groq"));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it("stays open with the error when the connection test fails", async () => {
    vi.mocked(api.providerTest).mockResolvedValue({
      ok: false,
      status: "failed",
      message: "Provider error 401: invalid key",
    });
    const onClose = vi.fn();
    render(<AddKeyModal provider={groq} open onClose={onClose} />);
    const input = document.getElementById("addkey-input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "gsk_bad" } });
    fireEvent.click(screen.getByRole("button", { name: /save/i }));
    await waitFor(() => expect(screen.getByText(/Provider error 401/)).toBeTruthy());
    expect(onClose).not.toHaveBeenCalled();
  });
});

describe("SelectModelsModal", () => {
  it("lists real models, toggles selection and saves the enabled set", async () => {
    const onClose = vi.fn();
    render(<SelectModelsModal provider={groq} open onClose={onClose} />);
    await waitFor(() => expect(screen.getByText("openai/gpt-oss-120b")).toBeTruthy());

    // Toggle two models on.
    fireEvent.click(screen.getByText("openai/gpt-oss-120b"));
    fireEvent.click(screen.getByText("llama-3.3-70b-versatile"));
    expect(screen.getByText("Enabled: 2 models")).toBeTruthy();

    // Toggle one back off.
    fireEvent.click(screen.getByText("openai/gpt-oss-120b"));
    expect(screen.getByText("Enabled: 1 model")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(api.providerSetConfig).toHaveBeenCalled());
    const call = vi.mocked(api.providerSetConfig).mock.calls[0];
    expect(call[0]).toBe("groq");
    expect(call[1].enabledModels).toEqual(["llama-3.3-70b-versatile"]);
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it("keeps previously enabled models listed when discovery fails", async () => {
    vi.mocked(api.providerListModels).mockRejectedValue("Provider error 429: rate limited");
    render(
      <SelectModelsModal provider={{ ...qwen, config: { enabledModels: ["qwen3-32b"] } }} open onClose={() => {}} />,
    );
    await waitFor(() => expect(screen.getByText(/rate limited/i)).toBeTruthy());
    // The previously enabled model is still selectable.
    expect(screen.getByText("qwen3-32b")).toBeTruthy();
  });

  it("select all and clear all operate on the visible list", async () => {
    render(<SelectModelsModal provider={groq} open onClose={() => {}} />);
    await waitFor(() => expect(screen.getByText("openai/gpt-oss-120b")).toBeTruthy());
    fireEvent.click(screen.getByText("Select All"));
    expect(screen.getByText("Enabled: 3 models")).toBeTruthy();
    fireEvent.click(screen.getByText("Clear All"));
    expect(screen.getByText("Enabled: 0 models")).toBeTruthy();
  });

  it("filters the list by search", async () => {
    render(<SelectModelsModal provider={groq} open onClose={() => {}} />);
    await waitFor(() => expect(screen.getByText("openai/gpt-oss-120b")).toBeTruthy());
    const search = screen.getByPlaceholderText("Search models…") as HTMLInputElement;
    fireEvent.change(search, { target: { value: "llama" } });
    expect(screen.getByText("llama-3.3-70b-versatile")).toBeTruthy();
    expect(screen.queryByText("openai/gpt-oss-120b")).toBeNull();
  });
});

describe("CustomProviderModal", () => {
  it("rejects duplicate custom provider names and disables Save", () => {
    render(<CustomProviderModal open editing={null} providers={[qwen]} onClose={() => {}} />);
    const name = document.getElementById("cp-name") as HTMLInputElement;
    fireEvent.change(name, { target: { value: "QWEN" } });
    expect(screen.getByText(/already exists/i)).toBeTruthy();
    const save = screen.getByRole("button", { name: /save/i }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
  });

  it("saves a new provider with name, endpoint and manual models", async () => {
    const onClose = vi.fn();
    render(<CustomProviderModal open editing={null} providers={[qwen]} onClose={onClose} />);
    const name = document.getElementById("cp-name") as HTMLInputElement;
    const key = document.getElementById("cp-key") as HTMLInputElement;
    fireEvent.change(name, { target: { value: "My AI" } });
    fireEvent.change(key, { target: { value: "sk-xyz" } });
    fireEvent.click(screen.getByText("Advanced Settings"));
    const url = await waitFor(() => document.getElementById("cp-url") as HTMLInputElement);
    fireEvent.change(url, { target: { value: "https://api.myai.dev/v1" } });
    fireEvent.click(screen.getByRole("button", { name: /save/i }));
    await waitFor(() => expect(api.customProviderSave).toHaveBeenCalled());
    const payload = vi.mocked(api.customProviderSave).mock.calls[0][0] as Record<string, unknown>;
    expect(payload.name).toBe("My AI");
    expect(payload.baseUrl).toBe("https://api.myai.dev/v1");
    expect(payload.format).toBe("openai");
    expect(api.providerTest).toHaveBeenCalledWith("custom-qwen");
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });
});
