import { afterEach, describe, expect, it, vi } from "vitest";
import { AiUsageSession } from "../examples/opencode/ai-usage-session.js";

describe("OpenCode router session integration", () => {
  afterEach(() => vi.unstubAllGlobals());
  it("imports all model names, preserves explicit limits, and tolerates an offline router", async () => {
    const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ data: [{ id: "flash-models" }, { id: "glm-flash" }, { id: "bad name" }] })));
    vi.stubGlobal("fetch", fetcher);
    const config = { provider: { "ai-usage": { options: { baseURL: "http://127.0.0.1:43129/v1", apiKey: "fictional_client_key_for_tests_0000000" }, models: { "glm-flash": { limit: { context: 100000, output: 8000 } } } } } };
    const hooks = await AiUsageSession(); await hooks.config(config);
    expect(Object.keys(config.provider["ai-usage"].models)).toEqual(["glm-flash", "flash-models"]);
    expect(config.provider["ai-usage"].models["glm-flash"].limit.context).toBe(100000);
    expect(fetcher.mock.calls[0][0].href).toBe("http://127.0.0.1:43129/v1/models");
    expect(fetcher.mock.calls[0][1].redirect).toBe("error");
    fetcher.mockRejectedValue(new Error("offline"));
    await hooks.config(config);
    expect(config.provider["ai-usage"].models["flash-models"]).toBeDefined();
  });
  it("never sends the client key to a remote URL or an unrelated provider", async () => {
    const fetcher = vi.fn(); vi.stubGlobal("fetch", fetcher);
    const hooks = await AiUsageSession();
    for (const baseURL of ["https://example.com/v1", "http://127.0.0.1:43129/admin", "http://user@127.0.0.1/v1", "http://127.0.0.1/v1?x=y"]) {
      await hooks.config({ provider: { "ai-usage": { options: { baseURL, apiKey: "fictional_client_key_for_tests_0000000" } } } });
    }
    await hooks.config({ provider: { unrelated: {} } });
    expect(fetcher).not.toHaveBeenCalled();
  });
  it("replaces the old 128k GLM guess and imports chain metadata for arbitrary aliases", async () => {
    const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ data: [
      { id: "glm-5.3-flash", limit: { context: 1000000, output: 131072, input: null } },
      { id: "mixed-chain", limit: { context: 32768, output: 8192, input: 30000 } },
    ] })));
    vi.stubGlobal("fetch", fetcher);
    const model = { name: "Custom label", tool_call: false, options: { temperature: 0.3 }, limit: { context: 131072, output: 16384, input: 120000 } };
    const config = { provider: { "ai-usage": { options: { apiKey: "fictional_client_key_for_tests_0000000" }, models: { "glm-5.3-flash": model } } } };
    await (await AiUsageSession()).config(config);
    expect(model.limit).toEqual({ context: 1000000, output: 131072 });
    expect(model.name).toBe("Custom label"); expect(model.tool_call).toBe(false);
    expect(model.options).toEqual({ temperature: 0.3 });
    expect(config.provider["ai-usage"].models["mixed-chain"].limit).toEqual({ context: 32768, output: 8192, input: 30000 });
  });
  it("does not keep oversized limits when a chain shrinks or its metadata becomes unknown", async () => {
    const fetcher = vi.fn(); vi.stubGlobal("fetch", fetcher);
    const config = { provider: { "ai-usage": { options: { apiKey: "fictional_client_key_for_tests_0000000" }, models: {} } } };
    const hooks = await AiUsageSession();
    for (const [limit, expected] of [
      [{ context: 1000000, output: 131072 }, { context: 1000000, output: 131072 }],
      [{ context: 32768, output: 8192 }, { context: 32768, output: 8192 }],
      [{ context: null, output: null }, { context: 16384, output: 4096 }],
      [{ context: -1, output: "131072", input: 2.5 }, { context: 16384, output: 4096 }],
      [{ context: 2048, output: 8192 }, { context: 2048, output: 2048 }],
    ]) {
      fetcher.mockResolvedValueOnce(new Response(JSON.stringify({ data: [{ id: "chain", limit }] })));
      await hooks.config(config);
      expect(config.provider["ai-usage"].models.chain.limit).toEqual(expected);
    }
  });
  it("keeps an opaque conversation ID stable across messages and leaves other providers alone", async () => {
    const hooks = await AiUsageSession();
    const headers = { "existing-header": "preserved" };
    const input = { model: { providerID: "ai-usage" }, sessionID: "session_fixture" };
    await hooks["chat.headers"](input, { headers });
    await hooks["chat.headers"](input, { headers });
    expect(headers).toEqual({ "existing-header": "preserved", "x-ai-usage-session": "session_fixture" });
    const other = { headers: {} };
    await hooks["chat.headers"]({ ...input, model: { providerID: "other" } }, other);
    expect(other.headers).toEqual({});
  });
});
