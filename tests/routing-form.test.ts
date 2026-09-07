// @vitest-environment jsdom
import { expect, it } from "vitest";
import { accountRouting, readAccountRouting, readRouting } from "../src/routing-form";
it("includes new supported accounts by default and preserves an explicit exclusion", () => {
  document.body.innerHTML = `<form>${accountRouting(undefined, "Plan only")}</form>`;
  expect(readAccountRouting(document.querySelector("form")!)).toEqual({ enabled: true });
  document.body.innerHTML = `<form>${accountRouting({ enabled: false }, "Plan only")}</form>`;
  expect(readAccountRouting(document.querySelector("form")!)).toEqual({ enabled: false });
  expect(accountRouting(undefined, undefined)).not.toContain('name="routingEnabled"');
});
it("connection form no longer carries account order or fallback rules", () => {
  document.body.innerHTML = '<form><input name="routerEnabled" type="checkbox" checked><input name="routerPort" value="43129"></form>';
  expect(readRouting(document.querySelector("form")!)).toEqual({ enabled: true, port: 43129, pools: [] });
});
