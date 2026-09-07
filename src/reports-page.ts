import { escapeHtml as esc } from "./format";
import type { Bootstrap, IUsageAppApi, RequestReport, RequestStatus, RouteTarget, RoutingReport } from "./types";

const labels: Record<RequestStatus, string> = {
  routing: "Finding pool", waiting: "Waiting for account", checking: "Checking allowance",
  connecting: "Waiting for provider", streaming: "Streaming", completed: "Completed",
  failed: "Failed", cancelled: "Cancelled",
};
function duration(ms: number): string {
  return ms < 1000 ? `${ms} ms` : ms < 60000 ? `${(ms / 1000).toFixed(1)} s` : `${(ms / 60000).toFixed(1)} min`;
}
function time(seconds: number): string { return new Date(seconds * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" }); }

export class ReportsPage {
  private container?: HTMLElement;
  private data?: Bootstrap;
  private report?: RoutingReport;
  private generation = 0;
  private inFlight?: number;
  private timer?: ReturnType<typeof setTimeout>;
  private query = "";
  private outcome = "all";
  private expanded = new Set<string>();
  private actionError = "";

  constructor(private readonly api: IUsageAppApi) {}

  mount(container: HTMLElement, data: Bootstrap): void {
    this.unmount(); this.container = container; this.data = data;
    container.innerHTML = `<section class="page-heading"><div><span class="eyebrow">FOLLOW YOUR REQUESTS</span><h1>Reports</h1><p>See which pipeline is working and where it goes.</p></div><button class="secondary" id="refresh-reports">↻ Refresh</button></section>
      <div class="report-live-line"><span><span class="live-dot"></span>Updates every second while this tab is open</span><span id="report-updated">Loading reports…</span></div>
      <div id="report-error" class="inline-error" role="status" hidden></div>
      <div id="report-stats" class="report-stats"></div>
      <section class="report-section"><div class="report-section-heading"><h2>In use now</h2><span id="report-active-count"></span></div><div id="report-active" class="report-active-grid"><p class="report-empty">Loading active requests…</p></div></section>
      <section class="report-section"><div class="report-section-heading"><div><h2>Recent requests</h2><p>Expand a request to see its route through the pool.</p></div><button class="text-button" id="clear-routing-history">Clear history</button></div>
        <div class="report-filters"><label>Find a pipeline or destination<input type="search" id="report-search" placeholder="Pool, model, provider or account" value="${esc(this.query)}"></label><label>Result<select id="report-outcome"><option value="all">All results</option><option value="completed">Completed</option><option value="failed">Failed</option><option value="cancelled">Cancelled</option><option value="fallback">Used fallback</option></select></label></div>
        <div id="report-recent" class="report-recent"><p class="report-empty">Loading recent requests…</p></div>
      </section><p class="report-privacy" id="report-retention">Routing metadata stays in memory on this PC. Prompts, responses and keys are never recorded. History clears when AI Usage exits.</p>`;
    const outcome = container.querySelector<HTMLSelectElement>("#report-outcome");
    if (outcome) outcome.value = this.outcome;
    container.querySelector("#refresh-reports")?.addEventListener("click", () => { void this.refresh(this.generation); });
    container.querySelector("#clear-routing-history")?.addEventListener("click", () => { void this.clear(); });
    container.querySelector<HTMLInputElement>("#report-search")?.addEventListener("input", event => { this.query = (event.currentTarget as HTMLInputElement).value; this.paintRecent(); });
    outcome?.addEventListener("change", () => { this.outcome = outcome.value; this.paintRecent(); });
    if (this.report) this.paint();
    void this.refresh(this.generation);
  }
  unmount(): void {
    this.generation++; this.container = undefined;
    if (this.timer) clearTimeout(this.timer);
    this.timer = undefined;
  }
  private async refresh(generation: number): Promise<void> {
    if (!this.container || generation !== this.generation || this.inFlight === generation) return;
    if (this.timer) clearTimeout(this.timer);
    this.inFlight = generation;
    try {
      const report = await this.api.routingReport();
      if (!this.container || generation !== this.generation) return;
      this.report = report; this.error(""); this.paint();
      const updated = this.container.querySelector("#report-updated");
      if (updated) updated.textContent = `Updated ${time(Date.now() / 1000)}`;
    } catch {
      if (this.container && generation === this.generation) this.error("Could not refresh routing reports. Showing the last successful snapshot, if available.");
    } finally {
      if (this.container && generation === this.generation) {
        this.inFlight = undefined;
        this.timer = setTimeout(() => { void this.refresh(generation); }, 1000);
      }
    }
  }
  private async clear(): Promise<void> {
    if (!this.container) return;
    const generation = ++this.generation;
    if (this.timer) clearTimeout(this.timer);
    const button = this.container.querySelector<HTMLButtonElement>("#clear-routing-history");
    if (button) button.disabled = true;
    try {
      await this.api.clearRoutingHistory();
      if (!this.container || generation !== this.generation) return;
      this.actionError = "";
      this.expanded.clear();
      if (this.report) this.report = { ...this.report, recent: [] };
      this.paint();
    } catch {
      if (this.container && generation === this.generation) {
        this.actionError = "Could not clear history. Try again."; this.error("");
      }
    } finally {
      if (this.container && generation === this.generation) {
        if (button) button.disabled = false;
        void this.refresh(generation);
      }
    }
  }
  private error(message: string): void {
    message = message || this.actionError;
    const element = this.container?.querySelector<HTMLElement>("#report-error");
    if (element) { element.textContent = message; element.hidden = !message; }
  }
  private provider(target: RouteTarget): string {
    return this.data?.providers.find(provider => provider.id === target.providerId)?.name || target.providerId || "Unavailable account";
  }
  private destination(target: RouteTarget): string {
    return `<span class="report-provider">${esc(this.provider(target))}</span><span class="report-account">${esc(target.accountLabel || target.accountId)}</span><code>${esc(target.model)}</code>`;
  }
  private paint(): void {
    if (!this.container || !this.report) return;
    const { active, recent, historyLimit } = this.report;
    const stats = this.container.querySelector("#report-stats");
    if (stats) stats.innerHTML = [["Active requests", active.length], ["Completed", recent.filter(row => row.status === "completed").length], ["Failed", recent.filter(row => row.status === "failed").length], ["Used fallback", recent.filter(row => row.fallbackCount > 0).length]].map(([label, count]) => `<div class="report-stat"><strong>${count}</strong><span>${label}</span></div>`).join("");
    const count = this.container.querySelector("#report-active-count");
    if (count) count.textContent = `${active.length} active`;
    const live = this.container.querySelector("#report-active");
    if (live) live.innerHTML = active.length ? active.map(row => `<article class="report-active-card" data-active-request="${esc(row.id)}"><div class="report-card-top"><span class="request-status ${row.status}">${esc(labels[row.status])}</span><span>#${esc(row.id)} · ${duration(row.durationMs)}</span></div><h3><code>${esc(row.pool)}</code></h3><div class="report-destination"><span class="route-arrow" aria-hidden="true">↓</span><div>${row.target ? this.destination(row.target) : '<span class="report-account">Choosing an eligible entry</span>'}</div></div><p>${esc(row.message)}</p><div class="report-card-bottom"><span>${row.streaming ? "Streaming request" : "JSON request"}</span><span>${row.target ? `Entry ${row.target.position}${row.fallbackCount ? " · fallback" : " · preferred"}` : "Checking pool"}</span></div></article>`).join("") : '<p class="report-empty">No requests in progress. Start a request from a connected app to see its pipeline here.</p>';
    const retained = this.container.querySelector("#report-retention");
    if (retained) retained.textContent = `Counts describe the last ${historyLimit} finished requests. Routing metadata stays in memory on this PC and clears when AI Usage exits. Prompts, responses and keys are never recorded.`;
    const clear = this.container.querySelector<HTMLButtonElement>("#clear-routing-history");
    if (clear) clear.disabled = recent.length === 0;
    const retainedIds = new Set(recent.map(row => row.id));
    for (const id of this.expanded) if (!retainedIds.has(id)) this.expanded.delete(id);
    this.paintRecent();
  }
  private matches(row: RequestReport): boolean {
    const query = this.query.trim().toLowerCase();
    const targets = [...row.attempts.map(attempt => attempt.target), ...(row.target ? [row.target] : [])];
    const searchable = [row.pool, row.id, ...targets.flatMap(target => [target.accountId, target.accountLabel, target.model, this.provider(target)])].join(" ").toLowerCase();
    return (!query || searchable.includes(query)) && (this.outcome === "all" || this.outcome === row.status || (this.outcome === "fallback" && row.fallbackCount > 0));
  }
  private paintRecent(): void {
    const list = this.container?.querySelector("#report-recent");
    if (!list || !this.report) return;
    const rows = this.report.recent.filter(row => this.matches(row));
    list.innerHTML = rows.length ? rows.map(row => `<details class="request-report" data-request="${esc(row.id)}" ${this.expanded.has(row.id) ? "open" : ""}><summary><span class="report-request-time">${esc(time(row.startedAt))}<small>#${esc(row.id)}</small></span><span class="report-request-route"><code>${esc(row.pool)}</code><small>${row.target ? `${esc(this.provider(row.target))} · ${esc(row.target.accountLabel || row.target.accountId)} → ${esc(row.target.model)}` : "No provider served this request"}</small></span><span class="request-result"><span class="request-status ${row.status}">${esc(labels[row.status])}</span><small>${duration(row.durationMs)}${row.fallbackCount ? ` · ${row.fallbackCount} fallback${row.fallbackCount === 1 ? "" : "s"}` : ""}</small></span><span class="report-chevron" aria-hidden="true">⌄</span></summary><div class="report-detail"><p>${esc(row.message)}${row.httpStatus !== null ? ` <span class="report-http">HTTP ${row.httpStatus}${row.streaming ? " (connection response)" : ""}</span>` : ""}</p>${row.omittedAttempts ? `<p class="report-muted">${row.omittedAttempts} earlier steps omitted. Showing the last ${this.report?.attemptLimit} steps.</p>` : ""}<ol class="report-attempts">${row.attempts.map(attempt => `<li class="attempt-${attempt.outcome}"><span class="attempt-position">${attempt.target.position}</span><div><strong>${esc(this.provider(attempt.target))} · ${esc(attempt.target.accountLabel || attempt.target.accountId)}</strong><code>${esc(attempt.target.model)}</code><p><span class="attempt-outcome">${attempt.outcome === "selected" ? "Selected" : attempt.outcome === "skipped" ? "Skipped" : "Failed"}.</span> ${esc(attempt.reason)}${attempt.retryAt !== null ? ` <span>Check again ${esc(new Date(attempt.retryAt * 1000).toLocaleString())}.</span>` : ""}</p></div></li>`).join("")}</ol>${!row.attempts.length ? '<p class="report-muted">No provider accepted this request.</p>' : ""}</div></details>`).join("") : `<p class="report-empty">${this.report.recent.length ? "No requests match these filters." : "No finished requests yet. Reports start with requests received after this app launch."}</p>`;
    list.querySelectorAll<HTMLDetailsElement>("[data-request]").forEach(details => details.addEventListener("toggle", () => {
      if (!details.isConnected) return;
      const id = details.dataset.request;
      if (id) { if (details.open) this.expanded.add(id); else this.expanded.delete(id); }
    }));
  }
}
