import { escapeHtml as esc } from "./format";
import type { AccountRouting, Bootstrap, ModelMapping, RouterSettings } from "./types";

export function modelRow(mapping: ModelMapping = { model: "", upstream: "" }): string {
  return `<div class="mapping-row"><label>Client model<input data-model value="${esc(mapping.model)}" placeholder="my-model" maxlength="200" required></label><span aria-hidden="true">→</span><label>Server model ID<input data-upstream value="${esc(mapping.upstream)}" placeholder="Exact model ID" maxlength="200" required></label><button type="button" class="text-button" data-remove-row aria-label="Remove model mapping">Remove</button></div>`;
}
export function accountRouting(config: AccountRouting | undefined, description: string | undefined): string {
  if (!description) return '<p class="session-note">Usage monitoring only. An included-only query connection has not been implemented for this provider.</p>';
  return `<fieldset class="account-routing"><legend>Model routing</legend><p>${esc(description)}</p><label class="toggle-row"><span>Use this account for routing</span><input name="routingEnabled" type="checkbox" ${config?.enabled ? "checked" : ""}><span class="toggle" aria-hidden="true"></span></label><p>Give the same client model name to equivalent models on different accounts. Configure substitutions on the Routing page.</p><div data-model-rows>${config?.models.map(modelRow).join("") ?? ""}</div><div class="form-actions"><button class="secondary" type="button" data-add-model>Add model</button><button class="secondary" type="button" data-list-models>List server models</button></div><div data-model-catalog class="model-catalog" aria-live="polite"></div></fieldset>`;
}
export function readAccountRouting(form: HTMLFormElement): AccountRouting {
  return { enabled: form.querySelector<HTMLInputElement>('[name="routingEnabled"]')?.checked ?? false,
    models: [...form.querySelectorAll<HTMLElement>("[data-model-rows] .mapping-row")].map(row => ({ model: row.querySelector<HTMLInputElement>("[data-model]")?.value.trim() ?? "", upstream: row.querySelector<HTMLInputElement>("[data-upstream]")?.value.trim() ?? "" })) };
}
export function bindModelRows(form: HTMLFormElement): void {
  form.querySelector("[data-add-model]")?.addEventListener("click", () => { form.querySelector("[data-model-rows]")?.insertAdjacentHTML("beforeend", modelRow()); });
  form.addEventListener("click", event => { if (event.target instanceof Element && event.target.closest("[data-remove-row]")) event.target.closest(".mapping-row")?.remove(); });
}
export function fallbackRow(model = "", alternatives: string[] = []): string {
  return `<div class="mapping-row"><label>Requested model<input data-fallback-model value="${esc(model)}" maxlength="200" placeholder="preferred-model" required></label><span aria-hidden="true">→</span><label>Alternatives, in order<input data-alternatives value="${esc(alternatives.join(", "))}" placeholder="backup-model, local-model" required></label><button class="text-button" type="button" data-remove-row aria-label="Remove fallback rule">Remove</button></div>`;
}
export function routingPage(data: Bootstrap): string {
  const settings = data.settings.routing;
  const known = Object.keys(data.settings.providers);
  const order = [...settings.accountOrder.filter(id => known.includes(id)), ...known.filter(id => !settings.accountOrder.includes(id))];
  return `<section class="page-heading"><div><span class="eyebrow">YOUR MODELS, YOUR ORDER</span><h1>Routing</h1><p>Use the first eligible account. Switch only when needed.</p></div><span class="connection-status ${data.router.error ? "warning" : ""}">${data.router.running ? "Listening" : "Stopped"}</span></section>
    ${data.router.error ? `<p class="inline-error">${esc(data.router.error)}</p>` : ""}
    <form id="routing-form" class="routing-form"><section class="routing-section"><h2>Connect a calling app</h2><p>Choose an OpenAI-compatible chat completions connection in your app. Set its base URL to <code>${esc(data.router.baseUrl)}</code> and its API key to the client key you save here.</p><p class="session-note">Local apps on this PC only. Paid fallback is disabled. Routing starts only when enabled below.</p><label class="toggle-row"><span>Enable local router</span><input name="routerEnabled" type="checkbox" ${settings.enabled ? "checked" : ""}><span class="toggle" aria-hidden="true"></span></label><div class="router-connection"><label>Port<input name="routerPort" type="number" min="1024" max="65535" required value="${settings.port}"></label><label>Client key<input id="client-token" name="clientToken" type="password" autocomplete="new-password" spellcheck="false" minlength="32" maxlength="256" placeholder="${data.router.tokenConfigured ? "Key saved. Leave blank to keep it." : "Generate a key"}"></label><button type="button" class="secondary" data-generate-token>Generate key</button><button type="button" class="secondary" data-copy-token>Copy new key</button></div><p class="session-note">Copy a new key into your calling app before saving. Saved keys are never displayed again. Generating a replacement takes effect when you save.</p></section>
    <section class="routing-section"><h2>Account priority</h2><p>For each model, accounts are tried from top to bottom. A recovered account regains its place on the next request. Add accounts and their model IDs in Settings.</p><ol class="account-order">${order.map(id => {
      const config = data.settings.providers[id]; const provider = data.providers.find(p => p.id === (config?.providerType || id));
      const eligible = Boolean(config?.enabled && config.routing.enabled && data.inference[provider?.id ?? ""]);
      return `<li data-order-account="${esc(id)}"><div><strong>${esc(config?.label || provider?.name || id)}</strong><small>${esc(provider?.name ?? id)} · ${eligible ? `${config?.routing.models.length ?? 0} mapped models` : "Routing off"}</small></div><button type="button" class="secondary" data-move="up" aria-label="Move account up">↑</button><button type="button" class="secondary" data-move="down" aria-label="Move account down">↓</button></li>`;
    }).join("") || '<li class="category-empty">No accounts saved yet.</li>'}</ol></section>
    <section class="routing-section"><h2>Model substitutions</h2><p>Try the requested model across all accounts first. If it is unavailable, try these alternatives in order. No substitution occurs unless you add a rule. Cycles are rejected.</p><div data-fallback-rows>${settings.fallbacks.map(rule => fallbackRow(rule.model, rule.alternatives)).join("")}</div><button type="button" class="secondary" data-add-fallback>Add fallback rule</button></section><div class="form-actions"><button type="submit" class="primary">Save routing settings</button></div></form>`;
}
export function readRouting(form: HTMLFormElement): RouterSettings {
  return { enabled: form.querySelector<HTMLInputElement>('[name="routerEnabled"]')?.checked ?? false, port: Number(form.querySelector<HTMLInputElement>('[name="routerPort"]')?.value),
    accountOrder: [...form.querySelectorAll<HTMLElement>("[data-order-account]")].map(row => row.dataset.orderAccount ?? ""),
    fallbacks: [...form.querySelectorAll<HTMLElement>("[data-fallback-rows] .mapping-row")].map(row => ({ model: row.querySelector<HTMLInputElement>("[data-fallback-model]")?.value.trim() ?? "", alternatives: (row.querySelector<HTMLInputElement>("[data-alternatives]")?.value ?? "").split(",").map(s => s.trim()).filter(Boolean) })) };
}
export function bindRoutingRows(form: HTMLFormElement): void {
  form.querySelector("[data-add-fallback]")?.addEventListener("click", () => { form.querySelector("[data-fallback-rows]")?.insertAdjacentHTML("beforeend", fallbackRow()); });
  form.addEventListener("click", event => {
    if (!(event.target instanceof Element)) return;
    if (event.target.closest("[data-remove-row]")) event.target.closest(".mapping-row")?.remove();
    const button = event.target.closest<HTMLButtonElement>("[data-move]"); const row = button?.closest("[data-order-account]");
    if (button && row) { if (button.dataset.move === "up") row.previousElementSibling?.before(row); else row.nextElementSibling?.after(row); }
  });
}
