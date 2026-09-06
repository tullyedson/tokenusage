export type Page = "usage" | "settings" | "routing";
export type Category = "llm" | "music" | "speech" | "media";
export type SettingField = { key: string; label: string; kind: string; help: string; placeholder: string; options: { value: string; label: string }[] };
export type ProviderDefinition = { id: string; name: string; category: Category; initials: string; color: string; description: string; helpUrl: string; fields: SettingField[] };
export type ModelMapping = { model: string; upstream: string };
export type AccountRouting = { enabled: boolean; models: ModelMapping[] };
export type RouterSettings = { enabled: boolean; port: number; accountOrder: string[]; fallbacks: { model: string; alternatives: string[] }[] };
export type ProviderConfig = { providerType: string; label: string; routing: AccountRouting; enabled: boolean; fields: Record<string, string>; sessionGeneration: number; revision: number };
export type Settings = { version: number; refreshMinutes: number; providers: Record<string, ProviderConfig>; routing: RouterSettings };
export type UsageMeter = { label: string; remaining: number | null; limit: number | null; percentLeft: number | null; unit: string; resetsAt: number | null; note: string | null };
export type UsageSnapshot = { meters: UsageMeter[]; plan: string | null; note: string | null };
export type ProviderReport = { providerId: string; snapshot: UsageSnapshot | null; updatedAt: number | null; attemptedAt: number | null; error: string | null; refreshing: boolean };
export type Bootstrap = { providers: ProviderDefinition[]; settings: Settings; reports: ProviderReport[]; startupError: string | null; configuredSecrets: Record<string, string[]>; inference: Record<string, { description: string }>; router: { running: boolean; baseUrl: string; tokenConfigured: boolean; error: string | null } };
export type Unsubscribe = () => void;

export interface IUsageAppApi {
  bootstrap(): Promise<Bootstrap>;
  currentPage(): Promise<Page>;
  refresh(providerId?: string): Promise<void>;
  saveProvider(providerId: string, enabled: boolean, label: string, fields: Record<string, string>, secrets: Record<string, string>, routing: AccountRouting): Promise<void>;
  addAccount(providerType: string): Promise<string>;
  saveRouting(routing: RouterSettings, clientToken: string): Promise<void>;
  discoverModels(providerId: string): Promise<string[]>;
  connect(providerId: string): Promise<string>;
  forget(providerId: string): Promise<void>;
  savePreferences(refreshMinutes: number): Promise<void>;
  getAutostart(): Promise<boolean>;
  setAutostart(enabled: boolean): Promise<void>;
  onUsage(callback: (reports: ProviderReport[]) => void): Promise<Unsubscribe>;
  onSettings(callback: () => void): Promise<Unsubscribe>;
  onPage(callback: (page: Page) => void): Promise<Unsubscribe>;
}
