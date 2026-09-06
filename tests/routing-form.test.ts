// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { accountRouting, bindModelRows, bindRoutingRows, fallbackRow, readAccountRouting, readRouting } from "../src/routing-form";

describe("model routing forms", () => {
  it("keeps model aliases separate from upstream IDs and adds/removes mappings", () => {
    document.body.innerHTML = `<form>${accountRouting({ enabled: true, models: [{ model: "logical", upstream: "server/model:tag" }] }, "Local model server")}</form>`;
    const form = document.querySelector("form")!; bindModelRows(form);
    expect(readAccountRouting(form)).toEqual({ enabled: true, models: [{ model: "logical", upstream: "server/model:tag" }] });
    form.querySelector<HTMLButtonElement>("[data-add-model]")!.click();
    expect(readAccountRouting(form).models).toHaveLength(2);
    form.querySelector<HTMLButtonElement>("[data-remove-row]")!.click();
    expect(readAccountRouting(form).models).toEqual([{ model: "", upstream: "" }]);
  });
  it("persists visible account order and explicit fallback order", () => {
    document.body.innerHTML = `<form><input name="routerEnabled" type="checkbox" checked><input name="routerPort" value="43129"><ol><li data-order-account="first"><button type="button" data-move="down">Down</button></li><li data-order-account="second"></li></ol><div data-fallback-rows>${fallbackRow("x", ["y", "z"])}</div></form>`;
    const form = document.querySelector("form")!; bindRoutingRows(form);
    form.querySelector<HTMLButtonElement>("[data-move]")!.click();
    expect(readRouting(form)).toEqual({ enabled: true, port: 43129, accountOrder: ["second", "first"], fallbacks: [{ model: "x", alternatives: ["y", "z"] }] });
    form.querySelector<HTMLButtonElement>("[data-remove-row]")!.click(); expect(readRouting(form).fallbacks).toEqual([]);
  });
  it("does not offer routing controls for usage-only connections and escapes values", () => {
    expect(accountRouting(undefined, undefined)).not.toContain('name="routingEnabled"');
    document.body.innerHTML = `<form>${fallbackRow('<img src=x onerror=alert(1)>', ['" onfocus="alert(2)'])}</form>`;
    expect(document.querySelector("img")).toBeNull(); expect(document.querySelector("[onfocus]")).toBeNull();
  });
});
