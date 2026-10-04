import { readFileSync } from "node:fs";
import vm from "node:vm";
import { describe, expect, it } from "vitest";

const script = readFileSync(new URL("../src-tauri/src/providers/scripts/google-gemini.js", import.meta.url), "utf8");
function run(globals = {}) {
  return vm.runInNewContext(script, {
    URLSearchParams, AbortSignal,
    location: { hostname: "gemini.google.com", protocol: "https:" },
    window: { WIZ_global_data: { SNlM0e: "fictional-page-token", privateIdentity: "private@example.invalid" } },
    ...globals
  })({});
}
const rpc = payload => ")]}'\n\n512\n" + JSON.stringify([["wrb.fr", "jSf9Qc", JSON.stringify(payload)]]) + "\n";
const reply = text => ({ ok: true, text: async () => text });

describe("Google Gemini usage reader", () => {
  it("reads only the usage RPC and returns numeric windows without credentials or account data", async () => {
    const calls = [];
    const value = await run({ fetch: async (url, options) => {
      calls.push(url);
      expect(options.method).toBe("POST");
      expect(options.credentials).toBe("include");
      expect(options.redirect).toBe("error");
      const body = new URLSearchParams(options.body);
      expect(JSON.parse(body.get("f.req"))).toEqual([[["jSf9Qc", "[]", null, "generic"]]]);
      expect(body.get("at")).toBe("fictional-page-token");
      return reply(rpc([3, [
        [null, 0.375, 1, [["1791200000", 0]]],
        [null, 0.8, 2, [[1791500000, 0]]],
        ["900", null, 3],
        ["private@example.invalid", 0.4, 99]
      ]]));
    }});
    expect(calls).toEqual(["/_/BardChatUi/data/batchexecute?rpcids=jSf9Qc&source-path=%2Fusage&hl=en"]);
    expect(value).toEqual({source:"gemini",plan:"Google AI Ultra",windows:[
      {kind:"five_hour",usedFraction:0.375,resetsAt:1791200000},
      {kind:"weekly",usedFraction:0.8,resetsAt:1791500000}
    ],credits:900});
    expect(JSON.stringify(value)).not.toMatch(/fictional|private|token|identity/i);
  });
  it("preserves exhausted and unused windows without guessing reset times or credit balances", async () => {
    const value = await run({fetch: async () => reply(rpc([null, [[null,1,1],[null,0,2],[null,null,3]]]))});
    expect(value.windows).toEqual([
      {kind:"five_hour",usedFraction:1,resetsAt:null},
      {kind:"weekly",usedFraction:0,resetsAt:null}
    ]);
    expect(value.plan).toBeNull();
    expect(value.credits).toBeNull();
  });
  it("does not read other Google hosts or send a request without its own session", async () => {
    const fetch = () => { throw new Error("Must not send a request"); };
    await expect(run({fetch, location:{hostname:"accounts.google.com",protocol:"https:"}})).rejects.toThrow(/Sign in/);
    await expect(run({fetch, location:{hostname:"gemini.google.com",protocol:"http:"}})).rejects.toThrow(/Sign in/);
    await expect(run({fetch, window:{}})).rejects.toThrow(/Sign in/);
  });
  it("rejects login pages, changed contracts, unrelated RPCs and invalid fractions", async () => {
    for (const text of ["<html>Sign in</html>", rpc({windows:[]}), rpc([3,[[null,-0.2,1],[null,1.2,2]]]), JSON.stringify([["wrb.fr","other","[]"]])]) {
      await expect(run({fetch:async()=>reply(text)})).rejects.toThrow(/usage|allowances/);
    }
    await expect(run({fetch:async()=>({ok:false})})).rejects.toThrow(/sign-in/);
    await expect(run({fetch:async()=>reply("x".repeat(1024*1024+1))})).rejects.toThrow(/oversized/);
  });
});
