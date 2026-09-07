import { escapeHtml as esc } from "./format";
import type { Bootstrap, IUsageAppApi, ModelLibrary, ModelLimits, ModelPool, PoolMember } from "./types";

const same = (a: PoolMember, b: PoolMember): boolean => a.accountId === b.accountId && a.model === b.model;
export const poolName = (value: string): string => value.trim().replace(/\s+/g, "-");
type Drag = { member: PoolMember; pool?: string; index?: number };

/** Owns the draft so catalog refreshes and page navigation cannot erase edits. */
export class ModelsPage {
  private root?: HTMLElement;
  private data?: Bootstrap;
  private library: ModelLibrary = { catalogs: [], pools: [] };
  private custom: ModelPool[] = [];
  private initialized = false;
  private dirty = false;
  private loading = false;
  private saving = false;
  private error = "";
  private search = "";
  private accountFilter = "";
  private showAutomatic = false;
  private selected = "";
  private opened = new Set<string>();
  private drag?: Drag;
  constructor(private api: IUsageAppApi, private notify: (message: string, error?: boolean) => void) {}

  mount(root: HTMLElement, data: Bootstrap): void {
    this.root = root; this.data = data;
    if (!this.initialized || !this.dirty) this.custom = structuredClone(data.settings.routing.pools);
    this.initialized = true;
    this.selected ||= this.custom[0]?.name ?? "";
    if (this.selected) this.opened.add(this.selected);
    this.render(); void this.load(false);
  }
  unmount(): void { this.root = undefined; this.drag = undefined; }
  private account(id: string): string {
    const config = this.data?.settings.providers[id];
    const provider = this.data?.providers.find(p => p.id === (config?.providerType || id));
    return `${provider?.name ?? id}${config?.label ? ` · ${config.label}` : ""}`;
  }
  private automatic(): ModelPool[] {
    const pools = new Map<string, ModelPool>();
    for (const catalog of this.library.catalogs) for (const { id: model } of catalog.models) {
      const pool = pools.get(model) ?? { name: model, members: [] };
      pool.members.push({ accountId: catalog.accountId, model }); pools.set(model, pool);
    }
    return [...pools.values()].sort((a, b) => a.name.localeCompare(b.name));
  }
  private pools(): ModelPool[] {
    return [...this.custom, ...this.automatic().filter(pool => !this.custom.some(item => item.name === pool.name))];
  }
  private edit(name: string): ModelPool | undefined {
    let pool = this.custom.find(p => p.name === name);
    if (!pool) {
      const source = this.automatic().find(p => p.name === name);
      if (!source) return;
      pool = structuredClone(source); this.custom.push(pool);
    }
    this.dirty = true; return pool;
  }
  private add(name: string, member: PoolMember, before?: number): void {
    if (this.saving) return;
    const pool = this.edit(name); if (!pool) return;
    if (pool.members.some(item => same(item, member))) { this.notify("That account/model is already in this pool."); return; }
    pool.members.splice(before ?? pool.members.length, 0, structuredClone(member));
    this.selected = name; this.opened.add(name); this.render();
  }
  private move(name: string, from: number, to: number): void {
    if (this.saving) return;
    const pool = this.edit(name); if (!pool || to < 0 || to >= pool.members.length) return;
    const member = pool.members.splice(from, 1)[0]; if (member) pool.members.splice(to, 0, member);
    this.render();
  }
  private async load(force: boolean): Promise<void> {
    if (this.loading || this.saving) return;
    this.loading = true; this.error = ""; this.render();
    try { this.library = await this.api.modelLibrary(force); }
    catch (error) { this.error = error instanceof Error ? error.message : String(error); }
    finally { this.loading = false; this.render(); }
  }
  private async save(): Promise<void> {
    if (this.saving) return;
    this.saving = true; this.render();
    try {
      await this.api.saveModelPools(structuredClone(this.custom));
      this.dirty = false;
      if (this.data) this.data.settings.routing.pools = structuredClone(this.custom);
      this.notify("Model pools saved. Calling apps can use these names now.");
    } catch (error) { this.notify(error instanceof Error ? error.message : String(error), true); }
    finally { this.saving = false; this.render(); }
  }
  private limitText(limits: ModelLimits): string {
    const number = (value: number | null): string => value === null ? "unknown" : value.toLocaleString();
    return `${number(limits.context)} context · ${number(limits.output)} output`;
  }
  private poolLimits(pool: ModelPool): ModelLimits {
    const limits = pool.members.filter(member => {
      const config = this.data?.settings.providers[member.accountId];
      return config?.enabled && config.routing.enabled;
    }).map(member => {
      const catalog = this.library.catalogs.find(c => c.accountId === member.accountId && !c.error);
      return catalog?.models.find(model => model.id === member.model)?.limits;
    });
    const lowest = (values: (number | null | undefined)[]): number | null => {
      if (!values.length || values.some(value => value == null)) return null;
      return Math.min(...values.filter((value): value is number => value != null));
    };
    return { context: lowest(limits.map(limit => limit?.context)), output: lowest(limits.map(limit => limit?.output)), input: null };
  }
  private catalogHtml(): string {
    const search = this.search.toLowerCase();
    return this.library.catalogs.filter(c => !this.accountFilter || c.accountId === this.accountFilter).map(catalog => {
      const models = catalog.models.filter(model => `${model.id} ${this.account(catalog.accountId)}`.toLowerCase().includes(search));
      if (!models.length && search) return "";
      return `<section class="catalog-account"><h3>${esc(this.account(catalog.accountId))}<span>${models.length}</span></h3>${catalog.error ? `<p class="catalog-error">${esc(catalog.error)}${catalog.models.length ? " Showing the last catalog; availability is checked before routing." : ""}</p>` : ""}${models.map(({ id: model, limits }) => `<div class="catalog-model" draggable="true" data-catalog-account="${esc(catalog.accountId)}" data-catalog-model="${esc(model)}"><span class="drag-handle" aria-hidden="true">⠿</span><div class="model-label"><code>${esc(model)}</code><small>${this.limitText(limits)}</small></div><button class="text-button" type="button" data-add-to-pool ${this.selected && !this.saving ? "" : "disabled"} aria-label="Add ${esc(model)} from ${esc(this.account(catalog.accountId))} to selected pool">Add</button></div>`).join("")}${!models.length && !catalog.error ? '<p class="session-note">No eligible models in this catalog.</p>' : ""}</section>`;
    }).join("") || `<p class="category-empty">${this.loading ? "Reading provider model catalogs…" : "No models found. Connect a supported account in Settings, then refresh models."}</p>`;
  }
  private poolHtml(pool: ModelPool): string {
    const custom = this.custom.some(p => p.name === pool.name);
    const automaticExists = this.automatic().some(p => p.name === pool.name);
    const limits = this.poolLimits(pool);
    return `<details class="model-pool ${this.selected === pool.name ? "selected" : ""}" data-pool="${esc(pool.name)}" ${this.opened.has(pool.name) ? "open" : ""}><summary><code>${esc(pool.name)}</code><span class="pool-badge">${custom ? "Custom" : "Automatic"}</span><small>${pool.members.length} ${pool.members.length === 1 ? "entry" : "entries"}</small><span class="chevron">›</span></summary><div class="pool-body"><p class="pool-hint">Try from top to bottom. Return to the first available entry after reset.</p><p class="pool-limits">Chain limit: ${this.limitText(limits)}. Lowest across enabled entries; unknown limits stay unknown.</p><ol class="pool-members">${pool.members.map((member, index) => `<li draggable="true" data-member-index="${index}"><span class="drag-handle" aria-hidden="true">⠿</span><span class="member-number">${index + 1}</span><div class="member-label"><code>${esc(member.model)}</code><small>${esc(this.account(member.accountId))}</small></div><div class="member-actions"><button type="button" class="text-button" data-up ${index === 0 ? "disabled" : ""} aria-label="Move ${esc(member.model)} up">↑</button><button type="button" class="text-button" data-down ${index === pool.members.length - 1 ? "disabled" : ""} aria-label="Move ${esc(member.model)} down">↓</button><button type="button" class="text-button" data-remove aria-label="Remove ${esc(member.model)}">×</button></div></li>`).join("")}</ol><div class="pool-drop">${pool.members.length ? "Drop another model here" : "Drag models here, or select this pool and use Add on the left."}</div><div class="pool-footer"><button type="button" class="secondary" data-select-pool>${this.selected === pool.name ? "Selected for Add" : "Select for Add"}</button>${custom ? `<button type="button" class="text-button" data-delete-pool>${automaticExists ? "Reset to automatic" : "Delete pool"}</button>` : ""}</div></div></details>`;
  }
  private render(): void {
    const root = this.root; if (!root?.isConnected) return;
    const pools = this.pools();
    root.innerHTML = `<section class="page-heading"><div><span class="eyebrow">ONE NAME, YOUR CHOICE</span><h1>Models</h1><p>Build a pool. Put its models in the order you want to use them.</p></div><span class="plan-badge">Plan only</span></section><div class="models-toolbar"><p aria-live="polite">${this.dirty ? "Unsaved pool changes" : "All discovered model names are available automatically."}</p><button class="secondary" data-refresh-models ${this.loading || this.saving ? "disabled" : ""}>${this.loading ? "Refreshing…" : "↻ Refresh models"}</button><button class="primary" data-save-pools ${!this.dirty || this.saving ? "disabled" : ""}>${this.saving ? "Saving…" : "Save pools"}</button></div>${this.error ? `<p class="inline-error">${esc(this.error)}</p>` : ""}<div class="models-layout"><section class="models-panel"><div class="models-panel-heading"><h2>Provider models</h2><p>Drag a model into a pool, or use Add.</p></div><div class="models-filters"><input type="search" aria-label="Search provider models" placeholder="Search models or accounts" data-model-search value="${esc(this.search)}"><select aria-label="Filter by account" data-account-filter><option value="">All accounts</option>${this.library.catalogs.map(c => `<option value="${esc(c.accountId)}" ${c.accountId === this.accountFilter ? "selected" : ""}>${esc(this.account(c.accountId))}</option>`).join("")}</select><label>Add to<select data-target-pool aria-label="Target pool"><option value="">Select a pool</option>${pools.map(pool => `<option value="${esc(pool.name)}" ${this.selected === pool.name ? "selected" : ""}>${esc(pool.name)}</option>`).join("")}</select></label></div><div class="catalog-list">${this.catalogHtml()}</div></section><section class="models-panel pool-panel"><div class="models-panel-heading"><h2>Your model pools</h2><p>The pool name is the model name other apps see.</p></div><form class="new-pool" data-new-pool><label>Common model name<input name="poolName" required maxlength="200" placeholder="flash-models" autocomplete="off" spellcheck="false"></label><button class="primary" ${this.saving ? "disabled" : ""}>Create</button><small>Spaces become hyphens. Models in a pool can be completely different.</small></form><div class="pool-list">${pools.filter(p => this.showAutomatic || this.custom.some(c => c.name === p.name) || p.name === this.selected).map(pool => this.poolHtml(pool)).join("") || '<p class="category-empty">Create a name above, then add your first model.</p>'}</div><label class="automatic-toggle"><input type="checkbox" data-show-automatic ${this.showAutomatic ? "checked" : ""}>Show automatic model names (${this.automatic().length})</label></section></div><p class="privacy-note">Only included plans, free models and local models are eligible. Keep paid overages and automatic top-ups disabled at your providers. Some providers currently support usage monitoring only.</p>`;
    this.bind(root);
  }
  private bind(root: HTMLElement): void {
    root.querySelector("[data-refresh-models]")?.addEventListener("click", () => { void this.load(true); });
    root.querySelector("[data-save-pools]")?.addEventListener("click", () => { void this.save(); });
    root.querySelector<HTMLInputElement>("[data-model-search]")?.addEventListener("input", event => {
      const input = event.currentTarget as HTMLInputElement; this.search = input.value;
      const list = root.querySelector(".catalog-list"); if (list) { list.innerHTML = this.catalogHtml(); this.bindCatalog(list); }
    });
    root.querySelector<HTMLSelectElement>("[data-account-filter]")?.addEventListener("change", event => { this.accountFilter = (event.currentTarget as HTMLSelectElement).value; this.render(); });
    root.querySelector<HTMLSelectElement>("[data-target-pool]")?.addEventListener("change", event => { this.selected = (event.currentTarget as HTMLSelectElement).value; this.opened.add(this.selected); this.render(); });
    root.querySelector<HTMLInputElement>("[data-show-automatic]")?.addEventListener("change", event => { this.showAutomatic = (event.currentTarget as HTMLInputElement).checked; this.render(); });
    root.querySelector<HTMLFormElement>("[data-new-pool]")?.addEventListener("submit", event => {
      event.preventDefault(); if (this.saving) return;
      const form = event.currentTarget as HTMLFormElement;
      const name = poolName(String(new FormData(form).get("poolName") ?? ""));
      if (!/^[a-zA-Z0-9._:/-]{1,200}$/.test(name)) { this.notify("Use letters, numbers, dots, underscores, colons, slashes or hyphens.", true); return; }
      if (this.pools().some(pool => pool.name === name)) { this.selected = name; this.opened.add(name); this.render(); this.notify("That name already exists. Edit its pool below."); return; }
      this.custom.push({ name, members: [] }); this.selected = name; this.opened.add(name); this.dirty = true; this.render();
    });
    this.bindCatalog(root);
    root.querySelectorAll<HTMLDetailsElement>("[data-pool]").forEach(card => {
      const name = card.dataset.pool ?? "";
      card.addEventListener("toggle", () => { if (!card.isConnected) return; if (card.open) this.opened.add(name); else this.opened.delete(name); });
      card.querySelector("[data-select-pool]")?.addEventListener("click", () => { this.selected = name; this.render(); });
      card.querySelector("[data-delete-pool]")?.addEventListener("click", () => {
        if (this.saving) return;
        this.custom = this.custom.filter(p => p.name !== name); this.dirty = true;
        if (this.selected === name) this.selected = this.custom[0]?.name ?? "";
        this.render();
      });
      card.addEventListener("dragover", event => { if (this.drag && !this.saving) { event.preventDefault(); card.classList.add("drag-over"); if (event.dataTransfer) event.dataTransfer.dropEffect = this.drag.pool === name ? "move" : "copy"; } });
      card.addEventListener("dragleave", event => { if (!(event.relatedTarget instanceof Node) || !card.contains(event.relatedTarget)) card.classList.remove("drag-over"); });
      card.addEventListener("drop", event => {
        event.preventDefault(); card.classList.remove("drag-over");
        const drag = this.drag; this.drag = undefined; if (!drag || this.saving) return;
        const row = event.target instanceof Element ? event.target.closest<HTMLElement>("[data-member-index]") : null;
        const length = this.pools().find(p => p.name === name)?.members.length ?? 0;
        const before = row ? Number(row.dataset.memberIndex) : length;
        if (drag.pool === name && drag.index !== undefined) this.move(name, drag.index, before > drag.index ? before - 1 : before);
        else this.add(name, drag.member, before);
      });
      card.querySelectorAll<HTMLElement>("[data-member-index]").forEach(row => {
        const index = Number(row.dataset.memberIndex);
        row.addEventListener("dragstart", event => {
          if (this.saving) { event.preventDefault(); return; }
          const member = this.pools().find(p => p.name === name)?.members[index];
          if (member) { this.drag = { member, pool: name, index }; event.dataTransfer?.setData("text/plain", member.model); if (event.dataTransfer) event.dataTransfer.effectAllowed = "copyMove"; }
        });
        row.addEventListener("dragend", () => { this.drag = undefined; });
        row.querySelector("[data-up]")?.addEventListener("click", () => this.move(name, index, index - 1));
        row.querySelector("[data-down]")?.addEventListener("click", () => this.move(name, index, index + 1));
        row.querySelector("[data-remove]")?.addEventListener("click", () => { if (this.saving) return; this.edit(name)?.members.splice(index, 1); this.render(); });
      });
    });
  }
  private bindCatalog(root: ParentNode): void {
    root.querySelectorAll<HTMLElement>("[data-catalog-model]").forEach(row => {
      const member = { accountId: row.dataset.catalogAccount ?? "", model: row.dataset.catalogModel ?? "" };
      row.addEventListener("dragstart", event => { if (this.saving) { event.preventDefault(); return; } this.drag = { member }; event.dataTransfer?.setData("text/plain", member.model); if (event.dataTransfer) event.dataTransfer.effectAllowed = "copy"; });
      row.addEventListener("dragend", () => { this.drag = undefined; });
      row.querySelector("[data-add-to-pool]")?.addEventListener("click", () => this.add(this.selected, member));
    });
  }
}
