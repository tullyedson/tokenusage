// Copy to your project's .opencode/plugins/ai-usage-session.js.
// Imports model names from the local router on OpenCode startup and preserves
// conversation IDs. Reads only the ai-usage client key from OpenCode's auth file.
import { readFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";

export const AiUsageSession = async () => ({
  config: async (config) => {
    const provider = config.provider?.["ai-usage"];
    if (!provider) return;
    try {
      const url = new URL(provider.options?.baseURL ?? "http://127.0.0.1:43129/v1");
      if (url.protocol !== "http:" || url.hostname !== "127.0.0.1" || url.username || url.password || url.search || url.hash || !/^\/v1\/?$/.test(url.pathname)) return;
      let key = provider.options?.apiKey;
      if (!key) {
        const path = join(process.env.XDG_DATA_HOME || join(homedir(), ".local", "share"), "opencode", "auth.json");
        const auth = JSON.parse(await readFile(path, "utf8"))["ai-usage"];
        if (auth?.type === "api") key = auth.key;
      }
      if (typeof key !== "string" || !/^[a-zA-Z0-9_-]{32,256}$/.test(key)) return;
      url.pathname = "/v1/models";
      const response = await fetch(url, { headers: { authorization: `Bearer ${key}` }, redirect: "error", signal: AbortSignal.timeout(15000) });
      if (!response.ok) return;
      const reader = response.body?.getReader(); if (!reader) return;
      let size = 0;
      const parts = [];
      try {
        while (true) {
          const chunk = await reader.read(); if (chunk.done) break;
          size += chunk.value.length;
          if (size > 4 * 1024 * 1024) return;
          parts.push(chunk.value);
        }
      } finally { await reader.cancel(); }
      const bytes = new Uint8Array(size); let offset = 0;
      for (const part of parts) { bytes.set(part, offset); offset += part.length; }
      const catalog = JSON.parse(new TextDecoder().decode(bytes));
      if (!Array.isArray(catalog.data)) return;
      provider.models ??= {};
      for (const row of catalog.data) {
        if (typeof row?.id !== "string" || !/^[a-zA-Z0-9._:/-]{1,200}$/.test(row.id)) continue;
        // Client defaults, not claims about upstream capabilities or maximums.
        // Explicit per-model settings in opencode.json take precedence.
        if (!Object.hasOwn(provider.models, row.id)) {
          Object.defineProperty(provider.models, row.id, { enumerable: true, configurable: true, writable: true, value: {
            name: row.id, tool_call: true, limit: { context: 16384, output: 4096 },
          } });
        }
      }
    } catch { /* Keep configured models usable when the tray app is offline. */ }
  },
  "chat.headers": async (input, output) => {
    if (input.model.providerID === "ai-usage") {
      output.headers["x-ai-usage-session"] = input.sessionID;
    }
  },
});
