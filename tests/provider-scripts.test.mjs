import { readFileSync } from "node:fs";
import vm from "node:vm";
import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";

function reader(name, globals) {
  const script = readFileSync(new URL(`../src-tauri/src/providers/scripts/${name}.js`, import.meta.url), "utf8");
  return vm.runInNewContext(script, { AbortSignal, setTimeout: callback => { queueMicrotask(callback); return 0; }, ...globals });
}
function response(data, status = 200, url = "") {
  return { ok: status === 200, status, url, json: async () => data, text: async () => data };
}

describe("subscription website readers", () => {
  it("waits for a cold website session to load before reading credits", async () => {
    for (const name of ["suno", "higgsfield"]) {
      const window = {};
      let waited = false;
      const run = reader(name, {
        window,
        setTimeout: callback => {
          waited = true;
          window.Clerk = { loaded: true, session: { getToken: async () => "fictional-test-token" } };
          queueMicrotask(callback);
          return 0;
        },
        fetch: async () => response({ total_credits_left: 20, subscription_balance: 20 })
      });
      await run({});
      expect(waited).toBe(true);
    }
  });
  it("Suno requests only billing and returns no session token or account identity", async () => {
    const calls = [];
    const run = reader("suno", { window: { Clerk: { session: { getToken: async () => "fictional-test-token" } } }, fetch: async (url, options) => {
      calls.push({ url, method: options.method ?? "GET" });
      return response({ total_credits_left: 1700, monthly_limit: 2500, monthly_usage: 800, email: "fiction@example.invalid" });
    } });
    const result = await run({});
    expect(calls).toEqual([{ url: "https://studio-api.prod.suno.com/api/billing/info/", method: "GET" }]);
    expect(result.total_credits_left).toBe(1700);
    expect(JSON.stringify(result)).not.toContain("fiction");
  });
  it("Higgsfield uses the current wallet contract and returns only usage fields", async () => {
    const run = reader("higgsfield", { window: { Clerk: { session: { getToken: async () => "fictional-test-token" } } }, fetch: async url => {
      expect(url).toBe("https://fnf-api-gw.higgsfield.ai/fnf/workspaces/wallet");
      return response({ workspace_id: "private-account", subscription_balance: 400, total_credits: 1000, credits_balance: 550 });
    } });
    const result = await run({});
    expect(result.subscription_balance).toBe(400);
    expect(result.workspace_id).toBeUndefined();
  });
  it("does not make authenticated requests without a website session", async () => {
    for (const name of ["suno", "higgsfield"]) {
      let called = false;
      const run = reader(name, { window: {}, fetch: () => { called = true; throw new Error("Unexpected network request"); } });
      await expect(run({})).rejects.toThrow(/Sign in/);
      expect(called).toBe(false);
    }
  });
  it("Claude requires an explicit choice when several organizations are available", async () => {
    const run = reader("anthropic", { fetch: async () => response([{ uuid: "example-one", capabilities: ["chat"] }, { uuid: "example-two", capabilities: ["chat"] }]) });
    await expect(run({})).rejects.toThrow(/organization ID/);
  });
  it("Claude reports current quota fields and strips account details", async () => {
    const calls = [];
    const run = reader("anthropic", { fetch: async url => { calls.push(url); return response({ five_hour: { utilization: 12, resets_at: "2026-09-07T10:00:00Z" }, account: "private" }); } });
    const result = await run({ organization: "example-one" });
    expect(calls).toEqual(["/api/organizations/example-one/usage"]);
    expect(result.five_hour.utilization).toBe(12);
    expect(result.account).toBeUndefined();
  });
  it("OpenAI reads usage without forwarding the authentication response", async () => {
    const calls = [];
    const run = reader("openai", { fetch: async url => {
      calls.push(url);
      return response(url === "/api/auth/session" ? { accessToken: "fictional-test-token", user: { email: "private" } } : { rate_limit: { primary_window: { used_percent: 20 } }, email: "private" });
    } });
    const result = await run({});
    expect(calls).toEqual(["/api/auth/session", "/backend-api/wham/usage"]);
    expect(result.rate_limit.primary_window.used_percent).toBe(20);
    expect(JSON.stringify(result)).not.toContain("private");
    expect(JSON.stringify(result)).not.toContain("token");
  });
  it("Ollama distinguishes monthly dollar usage from legacy percentages and reads resets", async () => {
    const dom = new JSDOM();
    const html = `<section><div><span>Monthly usage</span><span>$4.50 of $20 used</span><time data-time="2026-10-01T00:00:00Z"></time></div><div><span>Session usage</span><span>35% used</span></div><div><span>Weekly usage</span><span>74% used</span></div></section>`;
    const run = reader("ollama", { DOMParser: dom.window.DOMParser, fetch: async () => response(html, 200, "https://ollama.com/settings") });
    const result = await run({});
    expect(result.windows).toHaveLength(3);
    expect(result.windows[0]).toMatchObject({ label: "Monthly usage", used: 4.5, limit: 20, resetsAt: "2026-10-01T00:00:00Z" });
    expect(result.windows[2].usedPercent).toBe(74);
    dom.window.close();
  });
  it("Ollama does not silently turn a login page into zero usage", async () => {
    const dom = new JSDOM();
    const run = reader("ollama", { DOMParser: dom.window.DOMParser, fetch: async () => response("<form>Sign in</form>", 200, "https://signin.ollama.com/") });
    await expect(run({})).rejects.toThrow(/Sign in/);
    dom.window.close();
  });
});
