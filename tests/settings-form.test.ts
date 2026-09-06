// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { clearSecretInputs, readFields, renderField } from "../src/settings-form";
import type { SettingField } from "../src/types";

const key: SettingField = { key: "api_key", label: "API key", kind: "secret", help: "Stored securely", placeholder: "Paste a key", options: [] };
describe("credential settings", () => {
  it("never renders a saved secret into HTML or prepopulates a password", () => {
    const html = renderField("example", key, "fictional-sensitive-value", true);
    expect(html).not.toContain("fictional-sensitive-value");
    document.body.innerHTML = html;
    const input = document.querySelector<HTMLInputElement>("input")!;
    expect(input.type).toBe("password");
    expect(input.value).toBe("");
    expect(input.placeholder).toContain("Key saved");
  });
  it("separates credentials from ordinary fields and clears only passwords after save", () => {
    document.body.innerHTML = `<form><div class="fields">${renderField("example", key, "", false)}${renderField("example", { ...key, key: "workspace", kind: "text" }, "workspace-one", false)}</div></form>`;
    const form = document.querySelector<HTMLFormElement>("form")!;
    form.querySelector<HTMLInputElement>("[data-secret]")!.value = "fictional-key";
    expect(readFields(form)).toEqual({ fields: { workspace: "workspace-one" }, secrets: { api_key: "fictional-key" } });
    clearSecretInputs(form);
    expect(readFields(form)).toEqual({ fields: { workspace: "workspace-one" }, secrets: { api_key: "" } });
  });
});
