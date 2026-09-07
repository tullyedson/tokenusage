// @vitest-environment jsdom
import { expect, it, vi } from "vitest";
import type { AccountRouting, Bootstrap, ModelPool, RouterSettings } from "../src/types";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true, invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));

it("adds an account with models included by default and saves connection settings without overwriting pools", async () => {
  const data: Bootstrap = {
    providers: [{ id: "vllm-local", name: "vLLM (local)", category: "llm", initials: "vL", color: "#82bbec", description: "Local model server", helpUrl: "https://example.com", fields: [{ key: "base_url", label: "Server URL", kind: "text", help: "", placeholder: "", options: [] }] }],
    settings: { version: 3, refreshMinutes: 5, providers: { "vllm-local": { enabled: true, providerType: "", label: "Server one", fields: { base_url: "http://127.0.0.1:8000" }, routing: { enabled: true }, revision: 0, sessionGeneration: 0 } }, routing: { enabled: false, port: 43129, pools: [] } },
    reports: [], configuredSecrets: {}, startupError: null, inference: { "vllm-local": { description: "Local inference" } }, router: { running: false, baseUrl: "http://127.0.0.1:43129/v1", tokenConfigured: true, error: null },
  };
  mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
    if (command === "bootstrap") return structuredClone(data);
    if (command === "current_page") return "settings";
    if (command === "autostart_enabled") return false;
    if (command === "add_account") {
      data.settings.providers["account-second"] = { enabled: false, providerType: String(args?.providerType), label: "", routing: { enabled: true }, fields: {}, revision: 0, sessionGeneration: 0 };
      return "account-second";
    }
    if (command === "save_provider") {
      const id = String(args?.providerId); const old = data.settings.providers[id]; if (!old) throw new Error("Unknown fixture account");
      data.settings.providers[id] = { ...old, label: String(args?.label), enabled: Boolean(args?.enabled), fields: args?.fields as Record<string, string>, routing: args?.routing as AccountRouting }; return;
    }
    if (command === "model_library") return { catalogs: [{ accountId: "account-second", models: ["server-x"], checkedAt: 1000, error: null }], pools: [] };
    if (command === "save_model_pools") { data.settings.routing.pools = args?.pools as ModelPool[]; return; }
    if (command === "routing_report") return { active: [], recent: [], historyLimit: 100, attemptLimit: 64 };
    if (command === "save_routing") { data.settings.routing = { ...args?.routing as RouterSettings, pools: data.settings.routing.pools }; data.router.running = data.settings.routing.enabled; return; }
    if (command === "refresh_usage") return;
    throw new Error(`Unexpected fixture command: ${command}`);
  });
  document.body.innerHTML = '<div id="app"></div>';
  vi.stubGlobal("CSS", { escape: (value: string) => value });
  HTMLElement.prototype.scrollIntoView = vi.fn();
  await import("../src/main");
  await vi.waitFor(() => expect(document.querySelector("[data-add-account]")).not.toBeNull());
  document.querySelector<HTMLButtonElement>("[data-add-account]")!.click();
  await vi.waitFor(() => expect(document.querySelector('[data-provider-form="account-second"]')).not.toBeNull());
  const form = document.querySelector<HTMLFormElement>('[data-provider-form="account-second"]')!;
  expect(form.closest("details")?.open).toBe(true);
  form.querySelector<HTMLInputElement>('[name="accountLabel"]')!.value = "Server two";
  form.querySelector<HTMLInputElement>('[name="base_url"]')!.value = "http://127.0.0.1:8001";
  form.querySelector<HTMLInputElement>('[name="enabled"]')!.checked = true;
  form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await vi.waitFor(() => expect(data.settings.providers["account-second"]?.label).toBe("Server two"));
  expect(data.settings.providers["account-second"]?.routing.enabled).toBe(true);
  document.querySelector<HTMLButtonElement>('[data-page="models"]')!.click();
  await vi.waitFor(() => expect(document.querySelector("[data-catalog-model]")).not.toBeNull());
  const create = document.querySelector<HTMLFormElement>("[data-new-pool]")!;
  create.querySelector<HTMLInputElement>("input")!.value = "flash models";
  create.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  document.querySelector<HTMLButtonElement>("[data-add-to-pool]")!.click();
  document.querySelector<HTMLButtonElement>("[data-save-pools]")!.click();
  await vi.waitFor(() => expect(data.settings.routing.pools).toEqual([{ name: "flash-models", members: [{ accountId: "account-second", model: "server-x" }] }]));
  document.querySelector<HTMLButtonElement>('[data-page="routing"]')!.click();
  const routingForm = document.querySelector<HTMLFormElement>("#routing-form")!;
  routingForm.querySelector<HTMLInputElement>('[name="routerEnabled"]')!.checked = true;
  routingForm.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await vi.waitFor(() => expect(data.settings.routing.enabled).toBe(true));
  expect(data.settings.routing.pools[0]?.name).toBe("flash-models");
  expect(data.settings.providers["vllm-local"]?.fields.base_url).toBe("http://127.0.0.1:8000");
  document.querySelector<HTMLButtonElement>('[data-page="reports"]')!.click();
  await vi.waitFor(() => expect(document.querySelector("#report-active")?.textContent).toContain("No requests in progress"));
  expect(mocks.invoke).toHaveBeenCalledWith("routing_report");
  document.querySelector<HTMLButtonElement>('[data-page="models"]')!.click();
  await vi.waitFor(() => expect(document.querySelector('[data-pool="flash-models"]')).not.toBeNull());
  window.dispatchEvent(new Event("beforeunload")); vi.unstubAllGlobals();
});
