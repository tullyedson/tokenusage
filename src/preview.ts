// Development-only fixture adapter. Vite removes this module from release builds.
import type { AccountRouting, ModelPool, ModelLibrary, Bootstrap, IUsageAppApi, Page, ProviderDefinition, ProviderReport, RouterSettings, Unsubscribe, UsageMeter } from "./types";

const providers: ProviderDefinition[] = [
  { id: "openai", name: "OpenAI", category: "llm", initials: "OA", color: "#87e4b0", description: "Codex allowances included with your ChatGPT subscription.", helpUrl: "https://chatgpt.com/codex/settings/usage", fields: [{ key: "connection", label: "Connection", kind: "select", help: "Use a separate website session or the existing Codex sign-in.", placeholder: "", options: [{ value: "browser", label: "Sign in to ChatGPT" }, { value: "codex", label: "Use signed-in Codex" }] }] },
  { id: "anthropic", name: "Anthropic", category: "llm", initials: "An", color: "#dba185", description: "Claude subscription allowances.", helpUrl: "https://claude.ai/settings/usage", fields: [] },
  { id: "ollama", name: "Ollama Cloud", category: "llm", initials: "Ol", color: "#d2d8e0", description: "Cloud usage from Ollama settings.", helpUrl: "https://ollama.com/settings", fields: [] },
  { id: "suno", name: "Suno", category: "music", initials: "Su", color: "#edb276", description: "Your monthly and total credits.", helpUrl: "https://suno.com/account", fields: [{ key: "allowance", label: "Total-credit reference allowance", kind: "number", help: "Only used if the provider has no total allowance.", placeholder: "Optional", options: [] }] },
  { id: "higgsfield", name: "Higgsfield", category: "media", initials: "Hi", color: "#b8a3ef", description: "Your subscription wallet.", helpUrl: "https://higgsfield.ai/me/settings/subscription", fields: [] },
  { id: "openrouter", name: "OpenRouter", category: "llm", initials: "OR", color: "#afa8f4", description: "Account credits or a single key's allowance.", helpUrl: "https://openrouter.ai/settings/keys", fields: [{ key: "connection", label: "Usage source", kind: "select", help: "Account credits require a management key.", placeholder: "", options: [{ value: "credits", label: "Account credits (management key)" }, { value: "key", label: "This key's allowance (standard key)" }] }, { key: "api_key", label: "OpenRouter key", kind: "secret", help: "Saved in Windows Credential Manager in the desktop app.", placeholder: "Paste a key", options: [] }] },
  { id: "opencode", name: "OpenCode", category: "llm", initials: "OC", color: "#e0dcd3", description: "OpenCode Go subscription allowances.", helpUrl: "https://opencode.ai/auth", fields: [{ key: "api_key", label: "OpenCode API key", kind: "secret", help: "Use a key from the workspace with your Go subscription.", placeholder: "Paste a key", options: [] }] },
];

function sample(label: string, percentLeft: number, hours: number, remaining: number | null = null, limit: number | null = null, unit = "%"): UsageMeter {
  return { label, percentLeft, remaining, limit, unit, resetsAt: Math.floor(Date.now() / 1000) + hours * 3600, note: null };
}
function fixture(): Bootstrap {
  const empty = new URLSearchParams(location.search).has("empty");
  const pools: ModelPool[] = new URLSearchParams(location.search).has("pool") ? [{ name: "flash-models", members: [{ accountId: "opencode", model: "glm-5.3-flash" }, { accountId: "ollama", model: "deepseek-flash" }, { accountId: "ollama", model: "qwen3-coder" }] }] : [];
  const reports: ProviderReport[] = providers.map((provider, index) => ({
    providerId: provider.id, updatedAt: Math.floor(Date.now() / 1000) - 30, attemptedAt: Math.floor(Date.now() / 1000) - 30, refreshing: false, error: null,
    snapshot: { plan: ["Pro", "Max", "Pro", "Pro", "Ultimate", "Account credits", "Go subscription"][index] ?? null, note: null, meters: [
      [sample("5-hour allowance", 74, 3), sample("Weekly allowance", 42, 58)],
      [sample("5-hour allowance", 91, 4), sample("Weekly allowance", 18, 94)],
      [sample("Monthly usage", 62, 270, 62, 100, "USD")],
      [sample("Monthly credits", 68, 155, 1700, 2500, "credits")],
      [sample("Subscription credits", 36, 188, 1080, 3000, "credits")],
      [{ ...sample("Account credits", 75, 0, 75, 100, "USD"), resetsAt: null }],
      [sample("5-hour allowance", 75, 3), sample("Weekly allowance", 50, 62), sample("Monthly allowance", 40, 182)],
    ][index] ?? [] },
  }));
  return { providers, settings: { version: 3, refreshMinutes: 5, routing: { enabled: false, port: 43129, pools }, providers: empty ? {} : Object.fromEntries(providers.map(provider => [provider.id, { providerType: "", label: "", routing: { enabled: true }, enabled: true, fields: {}, sessionGeneration: 0, revision: 0 }])) }, reports: empty ? [] : reports, startupError: null, configuredSecrets: {}, inference: { opencode: { description: "OpenCode Go plan allowances." }, ollama: { description: "Ollama Cloud plan allowances." }, openrouter: { description: "Free models only." } }, router: { running: false, baseUrl: "http://127.0.0.1:43129/v1", tokenConfigured: false, error: null } };
}
export class PreviewApi implements IUsageAppApi {
  private data = fixture();
  private usage?: (reports: ProviderReport[]) => void;
  private changed?: () => void;
  private autostart = false;
  async bootstrap(): Promise<Bootstrap> { return structuredClone(this.data); }
  async currentPage(): Promise<Page> { return location.hash === "#models" ? "models" : location.hash === "#routing" ? "routing" : location.hash === "#settings" ? "settings" : "usage"; }
  async refresh(): Promise<void> { this.usage?.(structuredClone(this.data.reports)); }
  async saveProvider(id: string, enabled: boolean, label: string, fields: Record<string, string>, secrets: Record<string, string>, routing: AccountRouting): Promise<void> { if (Object.values(secrets).some(Boolean)) throw new Error("Key storage is available in the desktop app. This is a browser preview."); this.data.settings.providers[id] = { providerType: this.data.settings.providers[id]?.providerType ?? "", label, routing, enabled, fields, sessionGeneration: 0, revision: 0 }; this.changed?.(); }
  async addAccount(providerType: string): Promise<string> { const id = `account-${crypto.randomUUID()}`; this.data.settings.providers[id] = { providerType, label: "Example account", routing: { enabled: true }, enabled: false, fields: {}, sessionGeneration: 0, revision: 0 }; return id; }
  async saveRouting(routing: RouterSettings, clientToken: string): Promise<void> { if (clientToken) throw new Error("Client keys are only saved in the desktop app."); this.data.settings.routing = { ...routing, pools: this.data.settings.routing.pools }; }
  async modelLibrary(): Promise<ModelLibrary> {
    const examples: Record<string, string[]> = { opencode: ["glm-5.3-flash", "minimax-m2.7"], ollama: ["glm-5.3-flash", "deepseek-flash", "qwen3-coder"], openrouter: ["example/model:free"] };
    return { catalogs: Object.entries(this.data.settings.providers).filter(([id, config]) => examples[id] && config.enabled && config.routing.enabled).map(([accountId]) => ({ accountId, models: examples[accountId] ?? [], checkedAt: Date.now()/1000, error: null })), pools: this.data.settings.routing.pools.map(pool => ({ ...pool, automatic: false, available: true })) };
  }
  async saveModelPools(pools: ModelPool[]): Promise<void> { this.data.settings.routing.pools = structuredClone(pools); this.changed?.(); }
  async discoverModels(): Promise<string[]> { return ["example/local-model"]; }
  async connect(): Promise<string> { throw new Error("Sign-in is available in the desktop app. This is a browser preview."); }
  async forget(id: string): Promise<void> { delete this.data.settings.providers[id]; this.data.reports = this.data.reports.filter(report => report.providerId !== id); this.changed?.(); }
  async savePreferences(refreshMinutes: number): Promise<void> { this.data.settings.refreshMinutes = refreshMinutes; }
  async getAutostart(): Promise<boolean> { return this.autostart; }
  async setAutostart(enabled: boolean): Promise<void> { this.autostart = enabled; }
  async onUsage(callback: (reports: ProviderReport[]) => void): Promise<Unsubscribe> { this.usage = callback; return () => { this.usage = undefined; }; }
  async onSettings(callback: () => void): Promise<Unsubscribe> { this.changed = callback; return () => { this.changed = undefined; }; }
  async onPage(): Promise<Unsubscribe> { return () => {}; }
}
