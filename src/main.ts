import { isTauri } from "@tauri-apps/api/core";
import { NativeApi } from "./api";
import { balance, escapeHtml as esc, meterTone, percent, resetLabel, updatedLabel } from "./format";
import { clearSecretInputs, readFields, renderField } from "./settings-form";
import type { Bootstrap, Category, IUsageAppApi, Page, ProviderDefinition, ProviderReport, SettingField, Unsubscribe, UsageMeter } from "./types";
import "./style.css";

const root = document.querySelector<HTMLDivElement>("#app");
if (!root) throw new Error("App root missing");
const appRoot: HTMLDivElement = root;
const categories: { id: Category; title: string; description: string; icon: string }[] = [
  { id: "llm", title: "LLM", description: "Chat, reasoning and code", icon: "✳" },
  { id: "music", title: "Music", description: "Songs and soundtracks", icon: "♫" },
  { id: "speech", title: "Speech", description: "Voices and transcription", icon: "≋" },
  { id: "media", title: "Media", description: "Images and video", icon: "▧" },
];
let api: IUsageAppApi;
let data: Bootstrap;
let page: Page = "usage";
let preview = false;
let busy = false;
let toastTimer: ReturnType<typeof setTimeout> | undefined;
const subscriptions: Unsubscribe[] = [];

function logo(): string { return '<span class="logo" aria-hidden="true"><i></i><i></i><i></i></span>'; }
function icon(provider: ProviderDefinition): string { return `<span class="provider-icon" style="--provider:${esc(provider.color)}">${esc(provider.initials)}</span>`; }
function configuredProviders(): ProviderDefinition[] { return data.providers.filter(provider => data.settings.providers[provider.id]?.enabled); }
function reportFor(id: string): ProviderReport | undefined { return data.reports.find(report => report.providerId === id); }
function notify(message: string, error = false): void {
  const toast = appRoot.querySelector<HTMLDivElement>("#toast");
  if (!toast) return;
  toast.textContent = message; toast.className = `toast show${error ? " error" : ""}`;
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { toast.className = "toast"; }, 7000);
}
function errorMessage(error: unknown): string { return error instanceof Error ? error.message : typeof error === "string" ? error : "Something went wrong. Please try again."; }

function renderShell(): void {
  appRoot.innerHTML = `${preview ? '<div class="preview-banner">Browser preview · Sample readings, no accounts connected</div>' : ""}
    <header class="app-header"><a class="brand" href="#usage">${logo()}<span>AI Usage</span></a>
      <nav aria-label="Main"><button type="button" data-page="usage">Usage</button><button type="button" data-page="settings">Settings</button></nav>
      <span class="tray-note"><span class="live-dot"></span>In your tray</span>
    </header>
    <main id="content"></main>
    <footer><span>Private to this Windows account</span><span>AI Usage <span class="version">0.2.0</span></span></footer>
    <div id="toast" class="toast" role="status" aria-live="polite"></div>
    <dialog id="forget-dialog"><form method="dialog"><span class="eyebrow">DISCONNECT PROVIDER</span><h2>Forget this connection?</h2><p>This clears this app’s saved key, website session and settings for the provider. Your subscription stays active.</p><div class="form-actions"><button value="cancel" class="secondary">Cancel</button><button value="forget" class="danger">Forget connection</button></div></form></dialog>`;
  appRoot.querySelectorAll<HTMLButtonElement>("[data-page]").forEach(button => button.addEventListener("click", () => navigate(button.dataset.page === "settings" ? "settings" : "usage")));
  appRoot.querySelector(".brand")?.addEventListener("click", event => { event.preventDefault(); navigate("usage"); });
  renderPage();
}

function navigate(next: Page): void { page = next; renderPage(); }
function renderPage(): void {
  appRoot.querySelectorAll<HTMLButtonElement>("[data-page]").forEach(button => {
    button.classList.toggle("active", button.dataset.page === page);
    button.setAttribute("aria-current", button.dataset.page === page ? "page" : "false");
  });
  const content = appRoot.querySelector<HTMLElement>("#content");
  if (!content) return;
  if (page === "settings") { content.innerHTML = settingsPage(); bindSettings(); }
  else { content.innerHTML = usagePage(); bindUsage(); }
}

function meterHtml(meter: UsageMeter): string {
  const known = meter.percentLeft !== null && Number.isFinite(meter.percentLeft);
  const expired = meter.resetsAt !== null && meter.resetsAt * 1000 <= Date.now();
  const value = known ? Math.max(0, Math.min(100, meter.percentLeft ?? 0)) : 0;
  return `<div class="meter ${meterTone(meter)}${expired ? " expired" : ""}">
    <div class="meter-heading"><span>${esc(meter.label)}</span><strong>${known ? esc(percent(meter.percentLeft)) : "<span class=unknown-percent>Percent unavailable</span>"}</strong></div>
    <div class="track${known ? "" : " indeterminate"}" ${known ? `role="progressbar" aria-label="${esc(meter.label)} remaining" aria-valuenow="${value}" aria-valuemin="0" aria-valuemax="100"` : 'role="img" aria-label="Percentage unavailable"'}><div class="fill" style="width:${value}%"></div></div>
    <div class="meter-detail"><span>${esc(balance(meter))}</span><span>${esc(resetLabel(meter.resetsAt))}</span></div>
    ${meter.note ? `<p class="meter-note">${esc(meter.note)}</p>` : ""}</div>`;
}
function providerCard(provider: ProviderDefinition): string {
  const report = reportFor(provider.id);
  const status = report?.refreshing ? "Refreshing" : report?.error ? "Needs attention" : report?.snapshot ? "Connected" : "Awaiting connection";
  const stale = Boolean(report?.error && report.snapshot);
  return `<article class="usage-card${stale ? " stale" : ""}" data-provider="${esc(provider.id)}">
    <div class="card-header">${icon(provider)}<div class="provider-heading"><h2>${esc(provider.name)}</h2><span>${esc(report?.snapshot?.plan ?? categories.find(category => category.id === provider.category)?.title ?? "")}</span></div>
    <span class="connection-status${report?.error ? " warning" : ""}">${report?.refreshing ? '<span class="spinner"></span>' : '<span class="status-dot"></span>'}${status}</span></div>
    ${report?.snapshot ? report.snapshot.meters.map(meterHtml).join("") : '<p class="no-reading">Your allowances will appear here after the first successful refresh.</p>'}
    ${report?.error ? `<div class="inline-error" role="status">${stale ? "Showing the last successful reading. " : ""}${esc(report.error)}</div>` : ""}
    <div class="card-footer"><span>${esc(updatedLabel(report?.updatedAt ?? null))}</span><div><button class="text-button" data-configure="${esc(provider.id)}">Settings</button><button class="text-button" data-refresh="${esc(provider.id)}" ${report?.refreshing ? "disabled" : ""}>Refresh ↻</button></div></div>
  </article>`;
}
function usagePage(): string {
  const providers = configuredProviders();
  return `<section class="page-heading"><div><span class="eyebrow">YOUR ALLOWANCES</span><h1>Usage</h1><p>A little clarity before your next idea.</p></div><button class="secondary refresh-all" ${busy ? "disabled" : ""}>${busy ? '<span class="spinner"></span> Refreshing' : "↻ Refresh all"}</button></section>
    ${data.startupError ? `<div class="inline-error">${esc(data.startupError)}</div>` : ""}
    ${providers.length ? `<div class="overview-line"><span><span class="live-dot"></span>${providers.length} provider${providers.length === 1 ? "" : "s"} enabled</span><span>Checks every ${data.settings.refreshMinutes} minutes</span></div><div class="usage-grid">${providers.map(providerCard).join("")}</div>` : `<section class="empty-state"><div class="empty-visual">${logo()}<span class="orbit one"></span><span class="orbit two"></span></div><span class="eyebrow">ONE QUIET PLACE FOR YOUR AI</span><h2>Know what you have left.</h2><p>Connect your providers to see their remaining allowances and credits here.</p><button class="primary" id="first-provider">Connect a provider <span>↗</span></button><div class="provider-chips">${data.providers.map(provider => `<span>${esc(provider.name)}</span>`).join("")}</div></section>`}`;
}
function bindUsage(): void {
  appRoot.querySelector("#first-provider")?.addEventListener("click", () => navigate("settings"));
  appRoot.querySelector(".refresh-all")?.addEventListener("click", () => { void refresh(); });
  appRoot.querySelectorAll<HTMLButtonElement>("[data-refresh]").forEach(button => button.addEventListener("click", () => { void refresh(button.dataset.refresh); }));
  appRoot.querySelectorAll<HTMLButtonElement>("[data-configure]").forEach(button => button.addEventListener("click", () => {
    navigate("settings");
    const detail = appRoot.querySelector<HTMLDetailsElement>(`details[data-provider="${CSS.escape(button.dataset.configure ?? "")}"]`);
    if (detail) { detail.open = true; const category = detail.closest<HTMLDetailsElement>(".category"); if (category) category.open = true; detail.scrollIntoView({ block: "nearest" }); }
  }));
}

function fieldHtml(provider: ProviderDefinition, field: SettingField): string {
  return renderField(provider.id, field, data.settings.providers[provider.id]?.fields[field.key] ?? "", data.configuredSecrets[provider.id]?.includes(field.key) ?? false);
}
function providerSettings(provider: ProviderDefinition): string {
  const config = data.settings.providers[provider.id];
  const connected = Boolean(config?.enabled && reportFor(provider.id)?.snapshot);
  return `<details class="provider-settings" data-provider="${esc(provider.id)}"><summary>${icon(provider)}<span class="provider-summary">${esc(provider.name)}</span><span class="setup-state ${connected ? "is-setup" : ""}">${connected ? "✓ Connected" : config?.enabled ? "Enabled" : "Not set up"}</span><span class="chevron">›</span></summary>
    <form data-provider-form="${esc(provider.id)}"><p class="provider-description">${esc(provider.description)}</p>
      <label class="toggle-row"><span>Track this provider</span><input type="checkbox" name="enabled" ${config?.enabled ? "checked" : ""} /><span class="toggle" aria-hidden="true"></span></label>
      <div class="fields">${provider.fields.map(field => fieldHtml(provider, field)).join("")}</div>
      <p class="session-note"><span aria-hidden="true">▣</span> ${provider.fields.some(field => field.kind === "secret") ? "Enter your provider key above, then connect. A blank key field keeps the saved key." : "Sign in directly with the provider. This app keeps a separate website session on this PC."}</p>
      <div class="form-actions"><button type="submit" class="primary">Save settings</button><button type="button" class="secondary" data-connect="${esc(provider.id)}">Connect account ↗</button><button type="button" class="text-button forget" data-forget="${esc(provider.id)}" ${config ? "" : "disabled"}>Forget</button></div>
    </form></details>`;
}
function settingsPage(): string {
  return `<section class="page-heading"><div><span class="eyebrow">MAKE YOURSELF AT HOME</span><h1>Settings</h1><p>Connect an account. Its usage takes care of itself.</p></div></section>
    ${data.startupError ? `<div class="inline-error">${esc(data.startupError)}</div>` : ""}
    <section class="preferences" aria-label="General settings"><div><h2>Quietly in the background</h2><p>Closing the window keeps AI Usage in your tray.</p></div><form id="preferences"><label for="refresh-minutes">Refresh every</label><div class="interval"><input id="refresh-minutes" name="refreshMinutes" type="number" min="1" max="60" required value="${data.settings.refreshMinutes}" /><span>min</span></div><button class="secondary" type="submit">Save</button></form><label class="toggle-row startup"><span>Start with Windows</span><input type="checkbox" id="autostart" disabled /><span class="toggle" aria-hidden="true"></span></label></section>
    <div class="section-label"><span>PROVIDERS</span><span>Expand a category to connect</span></div>
    <section class="categories">${categories.map(category => {
      const providers = data.providers.filter(provider => provider.category === category.id);
      const enabled = providers.filter(provider => data.settings.providers[provider.id]?.enabled).length;
      return `<details class="category"><summary><span class="category-icon">${category.icon}</span><span class="category-name"><strong>${category.title}</strong><small>${category.description}</small></span><span class="category-count">${enabled ? `${enabled} enabled · ` : ""}${providers.length} provider${providers.length === 1 ? "" : "s"}</span><span class="chevron">›</span></summary><div class="category-body">${providers.length ? providers.map(providerSettings).join("") : '<p class="category-empty">No speech providers added yet.</p>'}</div></details>`;
    }).join("")}</section><p class="privacy-note">Keys are saved in Windows Credential Manager. Website sessions stay in this app’s private browser profiles. Usage readers only request balances and allowances.</p>`;
}
async function saveForm(form: HTMLFormElement, id: string, enabled: boolean): Promise<void> {
  const { fields, secrets } = readFields(form);
  try { await api.saveProvider(id, enabled, fields, secrets); clearSecretInputs(form); }
  finally { for (const key of Object.keys(secrets)) delete secrets[key]; }
  await reloadData(); updateSetupStatus();
}
async function withForm(form: HTMLFormElement, action: () => Promise<void>): Promise<void> {
  const buttons = form.querySelectorAll<HTMLButtonElement>("button");
  buttons.forEach(button => { button.disabled = true; });
  try { await action(); } catch (error) { notify(errorMessage(error), true); }
  finally { buttons.forEach(button => { button.disabled = false; }); }
}
async function reloadData(): Promise<void> { data = await api.bootstrap(); }
function bindSettings(): void {
  const checkbox = appRoot.querySelector<HTMLInputElement>("#autostart");
  if (checkbox) {
    void api.getAutostart().then(enabled => { if (checkbox.isConnected) { checkbox.checked = enabled; checkbox.disabled = false; } }).catch(error => notify(errorMessage(error), true));
    checkbox.addEventListener("change", () => {
      const enabled = checkbox.checked; checkbox.disabled = true;
      void api.setAutostart(enabled).catch(error => { checkbox.checked = !enabled; notify(errorMessage(error), true); }).finally(() => { checkbox.disabled = false; });
    });
  }
  appRoot.querySelector<HTMLFormElement>("#preferences")?.addEventListener("submit", event => {
    event.preventDefault(); const form = event.currentTarget as HTMLFormElement;
    void withForm(form, async () => { const minutes = Number(new FormData(form).get("refreshMinutes")); await api.savePreferences(minutes); data.settings.refreshMinutes = minutes; notify("Refresh interval saved."); });
  });
  appRoot.querySelectorAll<HTMLFormElement>("[data-provider-form]").forEach(form => {
    const id = form.dataset.providerForm ?? "";
    form.addEventListener("submit", event => {
      event.preventDefault();
      void withForm(form, async () => { await saveForm(form, id, form.querySelector<HTMLInputElement>('[name="enabled"]')?.checked ?? false); notify("Provider settings saved."); if (data.settings.providers[id]?.enabled) void refresh(id); });
    });
    form.querySelector("[data-connect]")?.addEventListener("click", () => {
      if (!form.reportValidity()) return;
      void withForm(form, async () => {
        const toggle = form.querySelector<HTMLInputElement>('[name="enabled"]'); if (toggle) toggle.checked = true;
        await saveForm(form, id, true);
        const message = await api.connect(id); updateSetupStatus(); notify(message);
      });
    });
    form.querySelector("[data-forget]")?.addEventListener("click", () => {
      const dialog = appRoot.querySelector<HTMLDialogElement>("#forget-dialog"); if (!dialog) return;
      dialog.addEventListener("close", () => { if (dialog.returnValue === "forget") void withForm(form, async () => { await api.forget(id); await reloadData(); renderPage(); notify("Connection forgotten."); }); }, { once: true });
      dialog.returnValue = "cancel"; dialog.showModal();
    });
  });
}
function updateSetupStatus(): void {
  appRoot.querySelectorAll<HTMLDetailsElement>(".provider-settings").forEach(detail => {
    const id = detail.dataset.provider ?? "";
    detail.querySelectorAll<HTMLInputElement>("input[data-secret]").forEach(input => {
      input.placeholder = data.configuredSecrets[id]?.includes(input.name) ? "Key saved. Leave blank to keep it." : "Paste a key";
    });
    const status = detail.querySelector(".setup-state");
    if (!status) return;
    const connected = Boolean(data.settings.providers[id]?.enabled && reportFor(id)?.snapshot);
    status.textContent = connected ? "✓ Connected" : data.settings.providers[id]?.enabled ? "Enabled" : "Not set up";
    status.classList.toggle("is-setup", connected);
  });
}
async function refresh(id?: string): Promise<void> {
  if (!id) busy = true;
  if (page === "usage") renderPage();
  try { await api.refresh(id); await reloadData(); }
  catch (error) { notify(errorMessage(error), true); }
  finally { if (!id) busy = false; if (page === "usage") renderPage(); else updateSetupStatus(); }
}

async function start(): Promise<void> {
  if (isTauri()) api = new NativeApi();
  else if (import.meta.env.DEV) { const { PreviewApi } = await import("./preview"); api = new PreviewApi(); preview = true; }
  else throw new Error("Open the installed AI Usage desktop app to connect your accounts.");
  subscriptions.push(await api.onUsage(reports => { if (!data) return; data.reports = reports; if (page === "usage") renderPage(); else updateSetupStatus(); }));
  subscriptions.push(await api.onSettings(() => { void reloadData().then(() => { if (page === "usage") renderPage(); else updateSetupStatus(); }).catch(error => notify(errorMessage(error), true)); }));
  subscriptions.push(await api.onPage(next => { if (data) navigate(next); else page = next; }));
  const [bootstrap, initialPage] = await Promise.all([api.bootstrap(), api.currentPage()]);
  data = bootstrap; page = initialPage; renderShell();
}
window.addEventListener("beforeunload", () => { subscriptions.forEach(unsubscribe => unsubscribe()); if (toastTimer) clearTimeout(toastTimer); });
void start().catch(error => {
  appRoot.innerHTML = `<div class="boot-error"><h1>AI Usage</h1><p>${esc(errorMessage(error))}</p><button id="retry" class="secondary">Try again</button></div>`;
  appRoot.querySelector("#retry")?.addEventListener("click", () => location.reload());
});
