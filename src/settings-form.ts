import { escapeHtml as esc } from "./format";
import type { SettingField } from "./types";

export function renderField(providerId: string, field: SettingField, savedValue: string, keySaved: boolean): string {
  const id = `${providerId}-${field.key}`;
  const secret = field.kind === "secret";
  const value = secret ? "" : savedValue;
  const placeholder = secret && keySaved ? "Key saved. Leave blank to keep it." : field.placeholder;
  const control = field.kind === "select" ? `<select id="${esc(id)}" name="${esc(field.key)}">${field.options.map(option => `<option value="${esc(option.value)}" ${value === option.value ? "selected" : ""}>${esc(option.label)}</option>`).join("")}</select>`
    : `<input id="${esc(id)}" name="${esc(field.key)}" type="${secret ? "password" : field.kind === "number" ? "number" : "text"}" ${secret ? 'data-secret="true"' : ""} ${field.kind === "number" ? 'min="0.000001" step="any"' : ""} value="${esc(value)}" placeholder="${esc(placeholder)}" autocomplete="${secret ? "new-password" : "off"}" spellcheck="false" maxlength="2048" />`;
  return `<div class="field"><label for="${esc(id)}">${esc(field.label)}</label>${control}<small>${esc(field.help)}</small></div>`;
}

export function readFields(form: HTMLFormElement): { fields: Record<string, string>; secrets: Record<string, string> } {
  const fields: Record<string, string> = {};
  const secrets: Record<string, string> = {};
  form.querySelectorAll<HTMLInputElement | HTMLSelectElement>(".fields input, .fields select").forEach(input => {
    (input.dataset.secret === "true" ? secrets : fields)[input.name] = input.value.trim();
  });
  return { fields, secrets };
}

export function clearSecretInputs(form: HTMLFormElement): void {
  form.querySelectorAll<HTMLInputElement>("input[data-secret]").forEach(input => { input.value = ""; });
}
