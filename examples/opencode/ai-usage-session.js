// Copy to your project's .opencode/plugins/ai-usage-session.js.
// OpenCode supplies an opaque conversation ID. No prompts, keys, paths or
// account details are read by this plugin.
export const AiUsageSession = async () => ({
  "chat.headers": async (input, output) => {
    if (input.model.providerID === "ai-usage") {
      output.headers["x-ai-usage-session"] = input.sessionID;
    }
  },
});
