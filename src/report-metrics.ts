import { escapeHtml as esc } from "./format";
import type { RequestReport } from "./types";

function known(value: number | null | undefined): value is number { return typeof value === "number" && Number.isFinite(value) && value >= 0; }
function count(value: number | null | undefined): string { return known(value) ? value.toLocaleString() : "Not reported"; }
function size(value: number | null | undefined): string {
  if (!known(value)) return "Not measured";
  if (value < 1024) return `${value} B`;
  return value < 1024 * 1024 ? `${(value / 1024).toFixed(1)} KiB` : `${(value / (1024 * 1024)).toFixed(2)} MiB`;
}
function percent(value: number): string { return value > 0 && value < 0.001 ? "<0.001" : value.toLocaleString(undefined, { maximumFractionDigits: 3 }); }
function tile(label: string, value: string, note: string): string { return `<div class="report-measure"><dt>${esc(label)}</dt><dd>${value}</dd><small>${note}</small></div>`; }

export function metricsSummary(row: RequestReport): string {
  const m = row.metrics;
  return `<span class="report-size-summary">${size(m.requestBytes)} in · ${size(m.responseBytes)} received</span>`;
}

export function renderMetrics(row: RequestReport): string {
  const m = row.metrics;
  const partial = row.status === "failed" || row.status === "cancelled";
  const pending = row.finishedAt === null;
  const inputNote = known(m.tokens?.cachedInput) ? `${count(m.tokens?.cachedInput)} cached (included)` : "Reported by provider";
  const outputNote = known(m.tokens?.reasoning) ? `${count(m.tokens?.reasoning)} reasoning (included)` : "Reported by provider";
  const context = known(m.contextUsedPercent) && known(m.contextLimit) && m.contextLimit > 0 ? `${percent(m.contextUsedPercent)}%` : "Not available";
  const contextNote = known(m.contextLimit) ? `${count(m.tokens?.total)} / ${count(m.contextLimit)} tokens in pool context` : "Needs token counts and a known pool context limit";
  const allowance = m.allowance;
  const observed = allowance?.status === "observed" && allowance.changes.length > 0;
  const allowanceValue = observed ? allowance.changes.map(change => `<span>${esc(change.label)} <b>+${percent(change.percentagePoints)} pp</b></span>`).join("") : allowance?.status === "pending" ? "Checking…" : "Not available";
  const allowanceNote = observed ? "Percentage-point change in account usage. Other activity, rounding and reporting delays can affect this; 0 means no reported change." : allowance?.status === "pending" ? "Reading the account after this call" : "No comparable before/after allowance readings";
  return `<section class="report-measurements" aria-label="Call measurements"><dl class="report-measure-grid">
    ${tile("Request body", size(m.requestBytes), known(m.requestBytes) ? `${count(m.requestBytes)} bytes from client` : "Body size unavailable")}
    ${tile("Response received", size(m.responseBytes), known(m.responseBytes) ? `${count(m.responseBytes)} bytes from upstream${partial ? " (partial)" : pending ? " so far" : ""}` : "No upstream body measured")}
    ${tile("Input tokens", count(m.tokens?.input), inputNote)}
    ${tile("Output tokens", count(m.tokens?.output), outputNote)}
    ${tile("Context used", context, contextNote)}
    ${tile("Observed allowance change", `<span class="report-allowance">${allowanceValue}</span>`, allowanceNote)}
    </dl><p class="report-measure-note">Body sizes exclude HTTP headers; streamed responses include event framing. ${pending ? "Response measurements update as data arrives; tokens may appear only at the end." : partial ? "This request did not finish successfully. Response measurements may be partial." : "Token counts describe this call, including conversation history sent by the client."}</p></section>`;
}
