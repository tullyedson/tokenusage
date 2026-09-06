import { describe, expect, it } from "vitest";
import { AiUsageSession } from "../examples/opencode/ai-usage-session.js";

describe("OpenCode router session integration", () => {
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
