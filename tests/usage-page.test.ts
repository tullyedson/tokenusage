// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Bootstrap, ProviderConfig, ProviderDefinition, ProviderReport } from "../src/types";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listeners: new Map<string, (event: { payload: ProviderReport[] }) => void>() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true, invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async (name: string, listener: (event: { payload: ProviderReport[] }) => void) => {
  mocks.listeners.set(name, listener);
  return () => { mocks.listeners.delete(name); };
} }));

function provider(id: string, name: string, showInUsage: boolean): ProviderDefinition {
  return { id, name, showInUsage, category: "llm", initials: "EX", color: "#87e4b0", description: "Example connection", helpUrl: "https://example.invalid", fields: [] };
}
function account(providerType: string, enabled = true): ProviderConfig {
  return { providerType, enabled, label: "", fields: {}, routing: { enabled: true }, sessionGeneration: 0, revision: 0 };
}
function reading(providerId: string): ProviderReport {
  return { providerId, snapshot: { plan: "Example plan", note: null, meters: [{ label: "Weekly allowance", remaining: null, limit: null, percentLeft: 75, unit: "%", resetsAt: null, note: null }] }, refreshing: false, error: null, updatedAt: 100, attemptedAt: 100 };
}
function fixture(): Bootstrap {
  const google = provider("google", "Google AI Ultra", true);
  google.fields = [{ key: "connection", label: "Usage source", kind: "select", help: "Choose a product", placeholder: "", options: [{ value: "gemini", label: "Gemini app" }, { value: "antigravity", label: "Antigravity" }] }];
  return {
    providers: [provider("cloud", "Example Cloud", true), google, provider("ollama-local", "Ollama (local)", false), provider("vllm-local", "vLLM (local)", false), provider("inference-bridge", "Example inference bridge", false)],
    settings: { version: 3, refreshMinutes: 5, providers: { cloud: account("cloud"), "server-a": account("ollama-local"), "server-b": account("ollama-local"), "server-c": account("vllm-local"), "server-d": account("inference-bridge") }, routing: { enabled: true, port: 43129, pools: [{ name: "local-pool", mode: "loadDistribution", members: [{ accountId: "server-a", model: "example-model" }] }] } },
    reports: [reading("cloud"), reading("server-a"), { ...reading("server-b"), error: "Example server offline" }], configuredSecrets: {}, startupError: null,
    inference: { "ollama-local": { description: "Local inference" }, "vllm-local": { description: "Local inference" }, "inference-bridge": { description: "Local inference" } },
    router: { running: true, baseUrl: "http://127.0.0.1:43129/v1", tokenConfigured: true, error: null },
  };
}

beforeEach(() => {
  vi.resetModules(); mocks.invoke.mockReset(); mocks.listeners.clear();
  document.body.innerHTML = '<div id="app"></div>';
  vi.stubGlobal("CSS", { escape: (value: string) => value });
  HTMLElement.prototype.scrollIntoView = vi.fn();
});
afterEach(() => { window.dispatchEvent(new Event("beforeunload")); vi.unstubAllGlobals(); });

async function start(data: Bootstrap) {
  mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
    if (command === "bootstrap") return structuredClone(data);
    if (command === "current_page") return "usage";
    if (command === "autostart_enabled") return false;
    if (command === "save_provider") {
      const id = String(args?.providerId);
      const fields = args?.fields;
      if (!fields || typeof fields !== "object") throw new Error("Expected fields");
      const values: Record<string, string> = {};
      for (const [key, value] of Object.entries(fields as Record<string, unknown>)) {
        if (typeof value !== "string") throw new Error("Expected string field");
        values[key] = value;
      }
      data.settings.providers[id] = { ...(data.settings.providers[id] ?? account(id)), enabled: args?.enabled === true, label: String(args?.label), fields: values };
      return;
    }
    if (command === "sign_in") {
      data.reports.push(reading(String(args?.providerId)));
      mocks.listeners.get("usage-updated")?.({ payload: structuredClone(data.reports) });
      return "Account connected.";
    }
    if (command === "refresh_usage") return;
    throw new Error(`Unexpected fixture command: ${command}`);
  });
  await import("../src/main");
  await vi.waitFor(() => expect(document.querySelector(".refresh-all")).not.toBeNull());
}

it("hides every routing-only account and opens the unconfigured Google account directly from Usage", async () => {
  const data = fixture();
  const previousRouting = structuredClone(data.settings.routing);
  const previousLocal = structuredClone(data.settings.providers["server-a"]);
  await start(data);
  expect([...document.querySelectorAll<HTMLElement>(".usage-card")].map(card => card.dataset.provider)).toEqual(["cloud"]);
  expect(document.querySelector(".overview-line")?.textContent).toContain("1 account tracked");
  expect(document.querySelector("#content")?.textContent).not.toMatch(/Ollama \(local\)|vLLM|inference bridge|server offline/);
  const connect = document.querySelector<HTMLButtonElement>('[data-configure="google"]')!;
  expect(connect.textContent).toContain("Google AI Ultra");
  connect.click();
  const detail = document.querySelector<HTMLDetailsElement>('details[data-provider="google"]')!;
  expect(detail.open).toBe(true);
  expect(detail.closest<HTMLDetailsElement>(".provider-group")?.open).toBe(true);
  expect(detail.closest<HTMLDetailsElement>(".category")?.open).toBe(true);
  const form = detail.querySelector<HTMLFormElement>("form")!;
  form.querySelector<HTMLSelectElement>('[name="connection"]')!.value = "antigravity";
  form.querySelector<HTMLButtonElement>("[data-connect]")!.click();
  await vi.waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("sign_in", { providerId: "google" }));
  expect(data.settings.providers.google?.fields.connection).toBe("antigravity");
  document.querySelector<HTMLButtonElement>('[data-page="usage"]')!.click();
  expect(document.querySelector('.usage-card[data-provider="google"]')?.textContent).toContain("75%");
  expect(document.querySelector('.usage-setup [data-configure="google"]')).toBeNull();
  expect(data.settings.routing).toEqual(previousRouting);
  expect(data.settings.providers["server-a"]).toEqual(previousLocal);
  document.querySelector<HTMLButtonElement>('[data-page="settings"]')!.click();
  expect(document.querySelector('[data-provider-form="server-a"]')).not.toBeNull();
  expect(document.querySelector('[data-provider-form="server-c"]')).not.toBeNull();
});

it("keeps each enabled Google account visible, including failures and first-refresh states", async () => {
  const data = fixture();
  data.settings.providers["google-web"] = { ...account("google"), label: "Gemini" };
  data.settings.providers["google-desktop"] = { ...account("google"), label: "Antigravity" };
  data.reports.push({ ...reading("google-web"), error: "Sign in again" });
  await start(data);
  expect([...document.querySelectorAll<HTMLElement>(".usage-card")].map(card => card.dataset.provider)).toEqual(["cloud", "google-web", "google-desktop"]);
  expect(document.querySelector('.usage-card[data-provider="google-web"]')?.textContent).toContain("Showing the last successful reading");
  expect(document.querySelector('.usage-card[data-provider="google-desktop"]')?.textContent).toContain("Awaiting connection");
  expect(document.querySelector(".usage-setup")).toBeNull();
});

it("shows setup when only local servers are enabled and reuses an existing disabled Google account", async () => {
  const data = fixture();
  delete data.settings.providers.cloud;
  data.settings.providers["google-saved"] = { ...account("google", false), label: "Saved Google", fields: { connection: "antigravity" } };
  await start(data);
  expect(document.querySelectorAll(".usage-card")).toHaveLength(0);
  expect(document.querySelector(".empty-state")).not.toBeNull();
  expect(document.querySelector("#content")?.textContent).not.toMatch(/Ollama \(local\)|vLLM|inference bridge/);
  const connect = document.querySelector<HTMLButtonElement>('[data-configure="google-saved"]')!;
  expect(connect.textContent).toContain("Google AI Ultra");
  connect.click();
  expect(document.querySelector<HTMLDetailsElement>('details[data-provider="google-saved"]')?.open).toBe(true);
  expect(document.querySelector<HTMLInputElement>('[data-provider-form="google-saved"] [name="accountLabel"]')?.value).toBe("Saved Google");
  expect(document.querySelector<HTMLSelectElement>('[data-provider-form="google-saved"] [name="connection"]')?.value).toBe("antigravity");
  expect(mocks.invoke).not.toHaveBeenCalledWith("add_account", expect.anything());
});
