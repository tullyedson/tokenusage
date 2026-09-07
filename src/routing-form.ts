import { escapeHtml as esc } from "./format";

import type { AccountRouting, Bootstrap, RouterSettings } from "./types";



export function accountRouting(config: AccountRouting | undefined, description: string | undefined): string {

  if (!description) return '<p class="session-note">Usage monitoring only. A supported plan-only query connection is not available for this provider yet.</p>';

  return `<fieldset class="account-routing"><legend>Model routing · Plan only</legend><p>${esc(description)}</p><label class="toggle-row"><span>Include this account in model pools</span><input name="routingEnabled" type="checkbox" ${config?.enabled !== false ? "checked" : ""}><span class="toggle" aria-hidden="true"></span></label><p>All eligible models appear automatically. Create common names and order their models on the Models page.</p></fieldset>`;

}

export function readAccountRouting(form: HTMLFormElement): AccountRouting {

  return { enabled: form.querySelector<HTMLInputElement>('[name="routingEnabled"]')?.checked ?? false };

}

export function routingPage(data: Bootstrap): string {

  const settings = data.settings.routing;

  return `<section class="page-heading"><div><span class="eyebrow">CONNECT YOUR APPS</span><h1>Routing</h1><p>One local connection for all your model pools.</p></div><span class="connection-status ${data.router.error ? "warning" : ""}">${data.router.running ? "Listening" : "Stopped"}</span></section>

    ${data.router.error ? `<p class="inline-error">${esc(data.router.error)}</p>` : ""}

    <form id="routing-form" class="routing-form"><section class="routing-section"><h2>Connect a calling app</h2><p>Choose an OpenAI-compatible chat completions connection in your app. Set its base URL to <code>${esc(data.router.baseUrl)}</code> and its API key to the client key you save here.</p><p class="session-note">Local apps on this PC only. Paid fallback is disabled. Routing starts only when enabled below.</p><label class="toggle-row"><span>Enable local router</span><input name="routerEnabled" type="checkbox" ${settings.enabled ? "checked" : ""}><span class="toggle" aria-hidden="true"></span></label><div class="router-connection"><label>Port<input name="routerPort" type="number" min="1024" max="65535" required value="${settings.port}"></label><label>Client key<input id="client-token" name="clientToken" type="password" autocomplete="new-password" spellcheck="false" minlength="32" maxlength="256" placeholder="${data.router.tokenConfigured ? "Key saved. Leave blank to keep it." : "Generate a key"}"></label><button type="button" class="secondary" data-generate-token>Generate key</button><button type="button" class="secondary" data-copy-token>Copy new key</button></div><p class="session-note">Copy a new key into your calling app before saving. Saved keys are never displayed again. Generating a replacement takes effect when you save.</p></section>

    <section class="routing-section"><h2>Models and fallback</h2><p>The Models page lists your discovered models and lets you create ordered pools. Use a pool name such as <code>flash-models</code> as the model in your calling app. The router uses plan allowances, free models or local models, and stops when every entry is unavailable.</p></section><div class="form-actions"><button type="submit" class="primary">Save routing settings</button></div></form>`;

}

export function readRouting(form: HTMLFormElement): RouterSettings {

  // The native connection command preserves pools saved by the Models page.

  return { enabled: form.querySelector<HTMLInputElement>('[name="routerEnabled"]')?.checked ?? false, port: Number(form.querySelector<HTMLInputElement>('[name="routerPort"]')?.value), pools: [] };

}
