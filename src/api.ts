import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AccountRouting, Bootstrap, IUsageAppApi, Page, ProviderReport, RouterSettings, Unsubscribe } from "./types";

export class NativeApi implements IUsageAppApi {
  bootstrap(): Promise<Bootstrap> { return invoke("bootstrap"); }
  currentPage(): Promise<Page> { return invoke("current_page"); }
  refresh(providerId?: string): Promise<void> { return invoke("refresh_usage", { providerId: providerId ?? null }); }
  saveProvider(providerId: string, enabled: boolean, label: string, fields: Record<string, string>, secrets: Record<string, string>, routing: AccountRouting): Promise<void> { return invoke("save_provider", { providerId, enabled, label, fields, secrets, routing }); }
  addAccount(providerType: string): Promise<string> { return invoke("add_account", { providerType }); }
  saveRouting(routing: RouterSettings, clientToken: string): Promise<void> { return invoke("save_routing", { routing, clientToken }); }
  discoverModels(providerId: string): Promise<string[]> { return invoke("discover_models", { providerId }); }
  connect(providerId: string): Promise<string> { return invoke("sign_in", { providerId }); }
  forget(providerId: string): Promise<void> { return invoke("forget_provider", { providerId }); }
  savePreferences(refreshMinutes: number): Promise<void> { return invoke("save_preferences", { refreshMinutes }); }
  getAutostart(): Promise<boolean> { return invoke("autostart_enabled"); }
  setAutostart(enabled: boolean): Promise<void> { return invoke("set_autostart", { enabled }); }
  onUsage(callback: (reports: ProviderReport[]) => void): Promise<Unsubscribe> { return listen<ProviderReport[]>("usage-updated", event => callback(event.payload)); }
  onSettings(callback: () => void): Promise<Unsubscribe> { return listen("settings-changed", callback); }
  onPage(callback: (page: Page) => void): Promise<Unsubscribe> { return listen<Page>("show-page", event => callback(event.payload)); }
}
