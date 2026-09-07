// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { NativeApi } from "../src/api";
import { ModelsPage } from "../src/models-page";
import type { Bootstrap, ModelLibrary } from "../src/types";
import desktop from "../src-tauri/tauri.conf.json";

const library: ModelLibrary = { catalogs: [
  { accountId: "go", models: ["glm-flash", "second-model"].map(id => ({ id, limits: { context: 1000000, input: null, output: 131072 } })), checkedAt: 1000, error: null },
  { accountId: "cloud", models: ["deepseek-flash", "glm-flash"].map(id => ({ id, limits: { context: 1000000, input: null, output: 131072 } })), checkedAt: 1000, error: null },
  { accountId: "local", models: ["Qwen"].map(id => ({ id, limits: { context: 1000000, input: null, output: 131072 } })), checkedAt: 1000, error: null },
], pools: [] };
const data: Bootstrap = {
  providers: [], reports: [], configuredSecrets: {}, inference: {}, startupError: null,
  router: { running: true, baseUrl: "http://127.0.0.1:43129/v1", tokenConfigured: true, error: null },
  settings: { version: 3, refreshMinutes: 5, providers: Object.fromEntries(["go", "cloud", "local"].map(id => [id, { providerType: id, label: "Fixture", routing: { enabled: true }, enabled: true, fields: {}, sessionGeneration: 0, revision: 0 }])), routing: { enabled: true, port: 43129, pools: [] } },
};
afterEach(() => vi.restoreAllMocks());
function button(selector: string): HTMLButtonElement { return document.querySelector<HTMLButtonElement>(selector)!; }
async function setup() {
  const api = new NativeApi();
  const read = vi.spyOn(api, "modelLibrary").mockResolvedValue(structuredClone(library));
  const save = vi.spyOn(api, "saveModelPools").mockResolvedValue();
  const notify = vi.fn(); const page = new ModelsPage(api, notify);
  document.body.innerHTML = '<main id="models"></main>';
  const root = document.querySelector<HTMLElement>("main")!;
  page.mount(root, structuredClone(data));
  await vi.waitFor(() => expect(document.querySelectorAll("[data-catalog-model]")).toHaveLength(5));
  return { page, root, read, save, notify };
}
function create(name: string) {
  const form = document.querySelector<HTMLFormElement>("[data-new-pool]")!;
  form.querySelector<HTMLInputElement>("input")!.value = name;
  form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
}
function drag(source: Element, target: Element) {
  source.dispatchEvent(new Event("dragstart", { bubbles: true }));
  target.dispatchEvent(new Event("drop", { bubbles: true, cancelable: true }));
}
it("shows the lowest chain context while editing and marks missing metadata unknown", async () => {
  const { read } = await setup(); create("mixed-chain");
  const target = document.querySelector('[data-pool="mixed-chain"]')!;
  for (const model of ["glm-flash", "deepseek-flash"]) drag(document.querySelector(`[data-catalog-model="${model}"]`)!, document.querySelector('[data-pool="mixed-chain"]')!);
  const text = () => document.querySelector('[data-pool="mixed-chain"] .pool-limits')?.textContent;
  expect(text()).toContain("1,000,000 context");
  const changed = structuredClone(library);
  changed.catalogs[1]!.models[0]!.limits.context = 32768;
  changed.catalogs[1]!.models[0]!.limits.output = 8192;
  read.mockResolvedValue(changed); button("[data-refresh-models]").click();
  await vi.waitFor(() => expect(text()).toContain("32,768 context"));
  changed.catalogs[1]!.error = "Fixture metadata unavailable";
  button("[data-refresh-models]").click();
  await vi.waitFor(() => expect(text()).toContain("unknown context"));
  expect(target.isConnected).toBe(false);
});
it("creates a mixed pool, supports drag and keyboard ordering, and saves the exact fallback sequence", async () => {
  // Tauri's native file-drop interception suppresses HTML5 drop events on Windows.
  expect(desktop.app.windows.find(window => window.label === "main")?.dragDropEnabled).toBe(false);
  const { save } = await setup(); create(" flash models ");
  const target = () => document.querySelector('[data-pool="flash-models"]')!;
  for (const model of ["glm-flash", "deepseek-flash", "Qwen"]) {
    drag(document.querySelector(`[data-catalog-model="${model}"]`)!, target());
  }
  const rows = () => target().querySelectorAll("[data-member-index]");
  expect(rows()).toHaveLength(3);
  drag(rows()[2]!, rows()[0]!);
  button('[data-pool="flash-models"] [data-down]').click();
  button('[data-pool="flash-models"] [data-member-index="1"] [data-down]').click();
  button("[data-save-pools]").click();
  await vi.waitFor(() => expect(save).toHaveBeenCalledWith([{ name: "flash-models", mode: "failover", members: [
    { accountId: "go", model: "glm-flash" }, { accountId: "cloud", model: "deepseek-flash" }, { accountId: "local", model: "Qwen" },
  ] }]));
});
it("keeps unsaved edits across navigation and refresh, rejects duplicates, and retains the draft after a failed save", async () => {
  const { page, root, read, save, notify } = await setup(); create("flash-models");
  button('[data-catalog-account="go"][data-catalog-model="glm-flash"] button').click();
  button('[data-catalog-account="go"][data-catalog-model="glm-flash"] button').click();
  expect(document.querySelectorAll("[data-member-index]")).toHaveLength(1);
  page.unmount(); root.innerHTML = "Another page"; page.mount(root, structuredClone(data));
  await vi.waitFor(() => expect(button("[data-refresh-models]").disabled).toBe(false));
  button("[data-refresh-models]").click();
  await vi.waitFor(() => expect(read).toHaveBeenLastCalledWith(true));
  expect(document.querySelector('[data-pool="flash-models"] code')?.textContent).toBe("flash-models");
  save.mockRejectedValueOnce(new Error("Fixture save failed")); button("[data-save-pools]").click();
  await vi.waitFor(() => expect(notify).toHaveBeenCalledWith("Fixture save failed", true));
  expect(button("[data-save-pools]").disabled).toBe(false);
  expect(document.querySelectorAll("[data-member-index]")).toHaveLength(1);
});
it("automatically groups same IDs and allows custom order to reset to discovery", async () => {
  const { save } = await setup();
  create("glm-flash");
  expect(document.querySelector('[data-pool="glm-flash"] .pool-badge')?.textContent).toBe("Automatic");
  button('[data-pool="glm-flash"] [data-down]').click();
  button("[data-save-pools]").click();
  await vi.waitFor(() => expect(save).toHaveBeenCalledWith([{ name: "glm-flash", mode: "failover", members: [
    { accountId: "cloud", model: "glm-flash" }, { accountId: "go", model: "glm-flash" },
  ] }]));
  await vi.waitFor(() => expect(button("[data-save-pools]").textContent).toBe("Save pools"));
  button('[data-pool="glm-flash"] [data-delete-pool]').click();
  button("[data-save-pools]").click();
  await vi.waitFor(() => expect(save).toHaveBeenLastCalledWith([]));
});

it("saves load distribution as an independent draft setting and can switch back to failover", async () => {
  const { page, root, save } = await setup(); create("balanced");
  button('[data-catalog-account="go"][data-catalog-model="glm-flash"] button').click();
  const select = () => document.querySelector<HTMLSelectElement>('[data-pool="balanced"] [data-pool-mode]')!;
  expect(select().value).toBe("failover");
  select().value = "loadDistribution"; select().dispatchEvent(new Event("change"));
  expect(document.querySelector('[data-pool="balanced"]')?.textContent).toContain("x-ai-usage-instance");
  page.unmount(); page.mount(root, structuredClone(data));
  await vi.waitFor(() => expect(button("[data-refresh-models]").disabled).toBe(false));
  expect(select().value).toBe("loadDistribution");
  button("[data-save-pools]").click();
  await vi.waitFor(() => expect(save).toHaveBeenLastCalledWith([{ name: "balanced", mode: "loadDistribution", members: [{ accountId: "go", model: "glm-flash" }] }]));
  await vi.waitFor(() => expect(select().disabled).toBe(false));
  select().value = "failover"; select().dispatchEvent(new Event("change"));
  button("[data-save-pools]").click();
  await vi.waitFor(() => expect(save).toHaveBeenLastCalledWith([{ name: "balanced", mode: "failover", members: [{ accountId: "go", model: "glm-flash" }] }]));
});
