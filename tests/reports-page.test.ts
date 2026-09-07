// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { PreviewApi } from "../src/preview";
import { ReportsPage } from "../src/reports-page";
import type { RoutingReport } from "../src/types";

let page: ReportsPage | undefined;
afterEach(() => { page?.unmount(); document.body.innerHTML = ""; vi.useRealTimers(); vi.restoreAllMocks(); });
async function setup() {
  vi.useFakeTimers();
  document.body.innerHTML = '<main id="content"></main>';
  const api = new PreviewApi();
  const data = await api.bootstrap();
  const report = await api.routingReport();
  const read = vi.spyOn(api, "routingReport").mockResolvedValue(report);
  page = new ReportsPage(api);
  page.mount(document.querySelector<HTMLElement>("main")!, data);
  await vi.advanceTimersByTimeAsync(0);
  return { api, read, data, report, page };
}

it("shows concurrent pipelines and provider destinations, preserves expanded steps, and filters history", async () => {
  const { read } = await setup();
  expect(document.querySelectorAll("[data-active-request]")).toHaveLength(2);
  expect(document.querySelector('[data-active-request="15"]')?.textContent).toContain("Ollama Cloud");
  expect(document.querySelector('[data-active-request="14"]')?.textContent).toContain("qwen-coder");
  const details = document.querySelector<HTMLDetailsElement>('[data-request="13"]')!;
  details.open = true; details.dispatchEvent(new Event("toggle"));
  expect(details.textContent).toContain("Included allowance exhausted.");
  expect(details.textContent).toContain("Check again");
  await vi.advanceTimersByTimeAsync(1000);
  expect(read).toHaveBeenCalledTimes(2);
  expect(document.querySelector<HTMLDetailsElement>('[data-request="13"]')?.open).toBe(true);
  const search = document.querySelector<HTMLInputElement>("#report-search")!;
  search.value = "Desk server"; search.dispatchEvent(new Event("input"));
  expect(document.querySelectorAll("[data-request]")).toHaveLength(1);
  expect(document.querySelector("#report-recent")?.textContent).toContain("local-coding");
  search.value = ""; search.dispatchEvent(new Event("input"));
  const outcome = document.querySelector<HTMLSelectElement>("#report-outcome")!;
  outcome.value = "fallback"; outcome.dispatchEvent(new Event("change"));
  expect(document.querySelectorAll("[data-request]")).toHaveLength(1);
  expect(document.querySelector("#report-recent")?.textContent).toContain("flash-models");
});

it("labels load-distribution selections without counting pool position as fallback", async () => {
  const { read, report } = await setup();
  const changed = structuredClone(report);
  changed.active[1]!.mode = "loadDistribution";
  changed.active[1]!.target!.position = 3;
  changed.recent[1]!.mode = "loadDistribution";
  changed.recent[1]!.target!.position = 3;
  read.mockResolvedValue(changed);
  await vi.advanceTimersByTimeAsync(1000);
  expect(document.querySelector('[data-active-request="14"]')?.textContent).toContain("Load distribution");
  expect(document.querySelector('[data-request="12"]')?.textContent).toContain("Load distribution");
  expect(document.querySelector('[data-request="12"] .request-result')?.textContent).not.toContain("fallback");
});

it("escapes provider metadata, reports read failures, and stops polling when the tab closes", async () => {
  const { read, report, page } = await setup();
  const malicious = structuredClone(report);
  malicious.active[0]!.pool = '<img src=x onerror="alert(1)">';
  malicious.active[0]!.target!.accountLabel = '<script>private</script>';
  read.mockResolvedValueOnce(malicious);
  await vi.advanceTimersByTimeAsync(1000);
  expect(document.querySelector("#report-active img, #report-active script")).toBeNull();
  expect(document.querySelector("#report-active")?.textContent).toContain("<script>private</script>");
  read.mockRejectedValueOnce(new Error("unavailable"));
  await vi.advanceTimersByTimeAsync(1000);
  expect(document.querySelector<HTMLElement>("#report-error")?.hidden).toBe(false);
  expect(document.querySelector("#report-error")?.textContent).toContain("last successful snapshot");
  page.unmount();
  const count = read.mock.calls.length;
  await vi.advanceTimersByTimeAsync(5000);
  expect(read).toHaveBeenCalledTimes(count);
});

it("clears only finished history and ignores a stale read that completes afterward", async () => {
  const { api, read, report } = await setup();
  let resolveRead: ((value: RoutingReport) => void) | undefined;
  read.mockImplementationOnce(() => new Promise(resolve => { resolveRead = resolve; }));
  await vi.advanceTimersByTimeAsync(1000);
  const cleared = { ...report, recent: [] };
  const clear = vi.spyOn(api, "clearRoutingHistory").mockResolvedValue();
  read.mockResolvedValue(cleared);
  document.querySelector<HTMLButtonElement>("#clear-routing-history")!.click();
  await vi.advanceTimersByTimeAsync(0);
  resolveRead?.(report);
  await vi.advanceTimersByTimeAsync(0);
  expect(clear).toHaveBeenCalledOnce();
  expect(document.querySelectorAll("[data-request]")).toHaveLength(0);
  expect(document.querySelectorAll("[data-active-request]")).toHaveLength(2);
  expect(document.querySelector("#report-recent")?.textContent).toContain("No finished requests yet");
});

it("does not publish a late snapshot into a remounted tab", async () => {
  const { read, report, page, data } = await setup();
  let resolveRead: ((value: RoutingReport) => void) | undefined;
  read.mockImplementationOnce(() => new Promise(resolve => { resolveRead = resolve; }));
  await vi.advanceTimersByTimeAsync(1000);
  page.unmount();
  read.mockResolvedValue({ ...report, active: [], recent: [] });
  page.mount(document.querySelector<HTMLElement>("main")!, data);
  await vi.advanceTimersByTimeAsync(0);
  resolveRead?.(report);
  await vi.advanceTimersByTimeAsync(0);
  expect(document.querySelectorAll("[data-active-request], [data-request]")).toHaveLength(0);
});

it("keeps history and an actionable error when clearing fails", async () => {
  const { api } = await setup();
  vi.spyOn(api, "clearRoutingHistory").mockRejectedValueOnce(new Error("unavailable"));
  document.querySelector<HTMLButtonElement>("#clear-routing-history")!.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(document.querySelectorAll("[data-request]")).toHaveLength(3);
  expect(document.querySelector<HTMLElement>("#report-error")?.hidden).toBe(false);
  expect(document.querySelector("#report-error")?.textContent).toContain("Could not clear history");
});

it("shows bytes, provider tokens and pool context separately from observed account allowance", async () => {
  const { read, report } = await setup();
  const changed = structuredClone(report);
  changed.recent[0]!.metrics.allowance = { status: "observed", changes: [{ label: "Weekly", beforePercent: 12, afterPercent: 12.125, percentagePoints: 0.125 }] };
  read.mockResolvedValue(changed);
  await vi.advanceTimersByTimeAsync(1000);
  const row = document.querySelector('[data-request="13"]')!;
  expect(row.textContent).toContain("183,420 bytes from client");
  expect(row.textContent).toContain("14,280 bytes from upstream");
  expect(row.textContent).toContain("42,000");
  expect(row.textContent).toContain("720");
  expect(row.textContent).toContain("4.272%");
  expect(row.textContent).toContain("42,720 / 1,000,000 tokens in pool context");
  expect(row.textContent).toContain("Weekly +0.125 pp");
  expect(row.textContent).toContain("Other activity, rounding and reporting delays");
  expect(document.querySelector("#report-retention")?.textContent).toContain("content, headers and keys are never recorded");
});

it("keeps absent metrics unknown, labels partial responses, and escapes allowance labels", async () => {
  const { read, report } = await setup();
  const changed = structuredClone(report);
  changed.recent[0]!.status = "cancelled";
  changed.recent[0]!.metrics.tokens = null;
  changed.recent[0]!.metrics.contextUsedPercent = null;
  changed.recent[0]!.metrics.allowance = { status: "observed", changes: [{ label: "<img src=x>", beforePercent: 12, afterPercent: 12, percentagePoints: 0 }] };
  read.mockResolvedValue(changed);
  await vi.advanceTimersByTimeAsync(1000);
  const row = document.querySelector('[data-request="13"]')!;
  expect(row.querySelector("img")).toBeNull();
  expect(row.textContent).toContain("Not reported");
  expect(row.textContent).toContain("Not available");
  expect(row.textContent).toContain("bytes from upstream (partial)");
  expect(row.textContent).toContain("0 means no reported change");
  expect(row.textContent).not.toContain("0%");
});
