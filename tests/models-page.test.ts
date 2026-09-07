// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { NativeApi } from "../src/api";
import { ModelsPage } from "../src/models-page";
import type { Bootstrap, ModelLibrary } from "../src/types";
import desktop from "../src-tauri/tauri.conf.json";

const library: ModelLibrary = { catalogs: [
  { accountId: "go", models: ["glm-flash", "second-model"], checkedAt: 1000, error: null },
  { accountId: "cloud", models: ["deepseek-flash", "glm-flash"], checkedAt: 1000, error: null },
  { accountId: "local", models: ["Qwen"], checkedAt: 1000, error: null },
], pools: [] };
const data: Bootstrap = {
  providers: [], reports: [], configuredSecrets: {}, inference: {}, startupError: null,
  router: { running: true, baseUrl: "http://127.0.0.1:43129/v1", tokenConfigured: true, error: null },
  settings: { version: 3, refreshMinutes: 5, providers: {}, routing: { enabled: true, port: 43129, pools: [] } },
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
  await vi.waitFor(() => expect(save).toHaveBeenCalledWith([{ name: "flash-models", members: [
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
  await vi.waitFor(() => expect(save).toHaveBeenCalledWith([{ name: "glm-flash", members: [
    { accountId: "cloud", model: "glm-flash" }, { accountId: "go", model: "glm-flash" },
  ] }]));
  await vi.waitFor(() => expect(button("[data-save-pools]").textContent).toBe("Save pools"));
  button('[data-pool="glm-flash"] [data-delete-pool]').click();
  button("[data-save-pools]").click();
  await vi.waitFor(() => expect(save).toHaveBeenLastCalledWith([]));
});
