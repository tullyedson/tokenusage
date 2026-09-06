// @vitest-environment jsdom
import { expect, it, vi } from "vitest";
import type { AccountRouting, Bootstrap, RouterSettings } from "../src/types";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true, invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));

it("adds a second account, maps its model, and saves routing order through native commands", async () => {
  const data: Bootstrap = {
    providers: [{ id: "vllm-local", name: "vLLM (local)", category: "llm", initials: "vL", color: "#82bbec", description: "Local model server", helpUrl: "https://example.com", fields: [{ key: "base_url", label: "Server URL", kind: "text", help: "", placeholder: "", options: [] }] }],
    settings: { version: 2, refreshMinutes: 5, providers: { "vllm-local": { enabled: true, providerType: "", label: "Server one", fields: { base_url: "http://127.0.0.1:8000" }, routing: { enabled: true, models: [{ model: "x", upstream: "server-x" }] }, revision: 0, sessionGeneration: 0 } }, routing: { enabled: false, port: 43129, accountOrder: ["vllm-local"], fallbacks: [] } },
    reports: [], configuredSecrets: {}, startupError: null, inference: { "vllm-local": { description: "Local inference" } }, router: { running: false, baseUrl: "http://127.0.0.1:43129/v1", tokenConfigured: true, error: null },
  };
  mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
    if (command === "bootstrap") return structuredClone(data);
    if (command === "current_page") return "settings";
    if (command === "autostart_enabled") return false;
    if (command === "add_account") {
      data.settings.providers["account-second"] = { enabled: false, providerType: String(args?.providerType), label: "", routing: { enabled: false, models: [] }, fields: {}, revision: 0, sessionGeneration: 0 };
      data.settings.routing.accountOrder.push("account-second"); return "account-second";
    }
    if (command === "save_provider") {
      const id = String(args?.providerId); const old = data.settings.providers[id]; if (!old) throw new Error("Unknown fixture account");
      data.settings.providers[id] = { ...old, label: String(args?.label), enabled: Boolean(args?.enabled), fields: args?.fields as Record<string, string>, routing: args?.routing as AccountRouting }; return;
    }
    if (command === "discover_models") return ["server-x"];
    if (command === "save_routing") { data.settings.routing = args?.routing as RouterSettings; data.router.running = data.settings.routing.enabled; return; }
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
  await vi.waitFor(() => expect(form.querySelector<HTMLButtonElement>("[data-list-models]")!.disabled).toBe(false));
  form.querySelector<HTMLButtonElement>("[data-list-models]")!.click();
  await vi.waitFor(() => expect(form.querySelector("[data-model-catalog] select")).not.toBeNull());
  form.querySelector<HTMLButtonElement>("[data-model-catalog] button")!.click();
  form.querySelector<HTMLInputElement>("[data-model]")!.value = "x";
  form.querySelector<HTMLInputElement>('[name="routingEnabled"]')!.checked = true;
  form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await vi.waitFor(() => expect(data.settings.providers["account-second"]?.routing).toEqual({ enabled: true, models: [{ model: "x", upstream: "server-x" }] }));
  await vi.waitFor(() => expect(form.querySelector<HTMLButtonElement>('[type="submit"]')!.disabled).toBe(false));
  document.querySelector<HTMLButtonElement>('[data-page="routing"]')!.click();
  const routingForm = document.querySelector<HTMLFormElement>("#routing-form")!;
  routingForm.querySelector<HTMLButtonElement>('[data-order-account="vllm-local"] [data-move="down"]')!.click();
  routingForm.querySelector<HTMLInputElement>('[name="routerEnabled"]')!.checked = true;
  routingForm.querySelector<HTMLButtonElement>("[data-add-fallback]")!.click();
  routingForm.querySelector<HTMLInputElement>("[data-fallback-model]")!.value = "x";
  routingForm.querySelector<HTMLInputElement>("[data-alternatives]")!.value = "y, z";
  routingForm.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await vi.waitFor(() => expect(data.settings.routing).toEqual({ enabled: true, port: 43129, accountOrder: ["account-second", "vllm-local"], fallbacks: [{ model: "x", alternatives: ["y", "z"] }] }));
  expect(data.settings.providers["vllm-local"]?.fields.base_url).toBe("http://127.0.0.1:8000");
  window.dispatchEvent(new Event("beforeunload")); vi.unstubAllGlobals();
});
