# Adding a provider

The extension boundary is `IUsageProvider` in `src-tauri/src/providers/mod.rs`. Every production setting and usage card is built from its DTOs. The frontend contains no provider-specific connection or parsing branches.

## Files to add or change

1. Add `src-tauri/src/providers/example.rs`, implementing `IUsageProvider`.
2. For a website session, add the reader expression in `src-tauri/src/providers/scripts/example.js` and include it in `browser_spec()`.
3. Declare the module and add `Arc::new(example::Example)` in `registry()`.
4. Add parser fixtures in the Rust module and reader tests in `tests/provider-scripts.test.mjs`.

That is enough to make the provider appear in both Settings and Usage. Choose an existing category: `llm`, `music`, `speech` or `media`. IDs must be unique, stable lowercase letters, digits or hyphens because they are also used as session-profile identifiers.

The ID `router` is reserved for local client authentication. Do not use it as a provider ID.

## Contract

| Method | Responsibility |
| --- | --- |
| `definition()` | Stable ID, name, category, initials, color, description, help URL and setting-field metadata |
| `browser_spec()` | `Some(BrowserSpec)` for an HTTPS sign-in/usage URL, exact allowed reader hostnames and bundled reader script; defaults to `None` for native connections |
| `parse(value, config)` | Convert a source response into validated `UsageSnapshot` and `UsageMeter` DTOs |
| `connect(context, config)` | Default opens the provider's isolated sign-in browser and returns `BrowserOpened`. Override for another connection and return `Ready` when an immediate usage read can run |
| `fetch(context, config)` | Default reads the website then calls `parse`. Override for another transport, honoring `context.cancelled` and bounding all network/process waits |

`ProviderDefinition.fields` supports `text`, `number`, `select` and `secret`. Text, number and select values are **nonsecret configuration only**, such as workspace IDs, executable paths and reference allowances. Declare a key with `SettingField::secret("api_key", "API key", "Help text")`. The generic frontend renders a blank password input and sends its value in a separate `secrets` argument to `save_provider`. Native validation rejects secret fields in the ordinary fields map, and only nonsecret fields enter `ProviderConfig` or settings JSON.

The service saves secret values through the method-only `ISecretStore` boundary, currently backed by Windows Credential Manager. `Bootstrap.configuredSecrets` contains field names whose keys are saved, never values. Blank secrets mean preserve the saved value. Credential changes are rolled back if the atomic settings save fails. Forget deletes the account's secret fields. Do not read the vault directly from a provider or expose a command to retrieve keys into HTML; use `context.secrets.get(&context.account_id, field)` in native Rust. Retrieved strings use `Zeroizing` buffers. Never substitute the provider type ID for the account ID: multiple accounts share one adapter but must not share credentials or browser profiles.

`FetchContext` provides the account ID, browser session, cancellation flag and an `ISecretStore`. Interfaces use methods; DTOs carry data. Provider internals stay out of the UI, service scheduler and persistence layer. New accounts have stable opaque IDs; legacy account IDs retain the provider ID so upgrades preserve saved connections. `ProviderConfig.provider_type(account_id)` resolves the adapter type without renaming the account.

## Optional inference support

Implement `IUsageProvider::inference()` to return an `IInferenceProvider` from `routing/engine.rs`. That interface provides metadata, configuration validation, model discovery, and preflight request preparation. Metadata enables a generic pool-inclusion toggle in Settings. The adapter catalog automatically populates the Models page and client model list. The engine and service contain no provider-ID branches. See [Routing architecture and contract](ROUTING.html) for the full lifecycle.

Inference is separate from usage monitoring. A query adapter must enforce local/free/included-only spending before it is registered. Never use subscription meters as authorization for an unrelated paid API key. Check every applicable quota window for the requested upstream model; return `Unavailable` with a provider reset/retry time and account/model scope. An expired reset permits another check, not an invented fresh allowance. If enforcement or allowance status cannot be established, fail closed. The HTTP adapters demonstrate verified model catalogs, loopback/private server validation and a provider-side zero-price guard.

The common transport speaks OpenAI-compatible chat completions. Bounded JSON and SSE forwarding keep the requested pool name in the response model field and expose the selected upstream ID in a response header. ModelPool owns an ordered list of accountId/model pairs. Only edited pools are persisted; exact model names are otherwise grouped automatically. No provider branches or per-account model mappings are needed. `InferenceContext` also supplies the request clock and optional validated session ID. `PreparedRequest.headers` carries only adapter-owned, allowlisted protocol headers; never forward arbitrary caller headers or the router's client key. A provider using a different response protocol needs a tested conversion at the inference boundary. Do not pass native Responses/Messages output through as chat completions. Native agent runtimes are not generic chat HTTP endpoints.

`routing/subscriptions.rs` implements Go and Ollama Cloud. Provider account billing settings enforce the final no-overage boundary: Go needs Use balance off and no BYOK, while Ollama currently accepts only legacy limits without extra credits. Plan-only is the default policy. These external settings cannot be inspected or changed through the available APIs; explain the provider prerequisites in adapter metadata and do not claim that a saved app toggle enforces them. There is no paid fallback option. Go additionally checks every current window before submission. Unknown/malformed Go usage fails closed, and resets trigger new checks. Ollama availability is determined by provider submission responses. Monthly-credit Ollama plans remain ineligible until a supported no-extra-credit guard is available.

Test through `IInferenceProvider` with fictional local HTTP fixtures. Cover named pools, exact member order, automatic catalog discovery, multiple accounts, quota exhaustion, reset recovery, cancellation, paid-request rejection, and interrupted responses without replay. Never construct the Tauri usage registry in these tests.

The OpenRouter and OpenCode adapters demonstrate native-key connections. Return `Ready` from `connect`, load the key in `fetch`, and call `http::get_usage` with a verified constant HTTPS endpoint and the cancellation flag. This transport permits GET only, rejects redirects, uses bounded waits and response sizes, and returns redacted errors. Parse only quota fields into a `UsageSnapshot`. Avoid logging raw responses, keys or account identity. Use a helpful static 403 message when the source requires a specific key type or subscription.

The OpenAI adapter demonstrates an optional local-process transport without special cases in the service or dashboard. It launches a hidden, bounded subprocess, issues only the usage request, and terminates the subprocess on completion, failure or cancellation.

## Meter semantics

Use `UsageMeter::used_percent(label, used, reset)` when the source reports percent consumed. The helper calculates percent remaining and clamps an exhausted allowance to zero. Use `UsageMeter::balance(label, remaining, limit, unit, reset)` for credits, money or tokens. Pass `None` when the limit is unknown. Never fabricate a denominator from a plan name or merge unrelated windows. Purchased top-ups may make a total balance larger than the recurring allowance.

Reset timestamps are UTC Unix seconds. The `timestamp()` helper also accepts RFC 3339 input. A missing value stays absent. `UsageSnapshot::new()` rejects empty readings, so a login page or changed response cannot silently become a zero-usage success.

Do not hardcode a weekly, hourly or monthly window when the provider actually gives its duration. Include model-specific windows as separate meters. Attach a note when a percentage uses a user-provided comparison allowance.

## Website reader shape

The script file is a standalone async function expression accepting the provider's nonsecret fields:

```javascript
(async function (fields) {
  // Use the real endpoint verified on this provider's own website.
  const response = await fetch("/verified-usage-endpoint", {
    credentials: "include",
    cache: "no-store",
    signal: AbortSignal.timeout(15000)
  });
  if (!response.ok) throw new Error("Sign in to Example, then refresh.");
  const value = await response.json();
  return { remaining: value.remaining, allowance: value.allowance };
})
```

Replace the example route and fields with a verified contract. Return an explicit allowlist of quota fields, never a complete authentication response, token, email or workspace object. A provider may require a bearer token available to its own webpage; use it only inside that page's request and do not return it to native code.

Remote provider windows have no native-command capability. Native Rust evaluates the bundled reader and receives only its result. Scripts run after load, on explicitly allowed HTTPS hosts. Background refresh cannot open sign-in popups; the user opens an interactive window to handle authentication. Do not add arbitrary URL/script settings or challenge-solving automation.

## Lifecycle and tests

The service serializes refreshes per provider, retains a failed refresh's last successful reading, and reports errors separately. Every save changes a configuration revision and cancels the active fetch. A late result from an earlier configuration is discarded. Disabling stops work and closes provider windows. Forget cancels reads, removes saved keys, clears an optional browser session and advances its profile generation so old cookies cannot be reused.

Use fictional payload fixtures for consumption-to-remaining conversion, multiple windows, missing limits, exhausted quotas, unexpected login pages and changed response shapes. Test the actual JavaScript reader with mocked fetch. Keep pure parser tests on the concrete provider type: constructing the whole dynamic registry drags Windows UI imports into a Rust test executable that lacks the application's UI manifest.

Run the commands in the README. Then verify sign-in, refresh, session persistence and Forget with an authorized real test account. Public-site contract research and fixtures alone do not prove that a site's sign-in flow accepts WebView2. Record that limitation until a real session has been exercised.

The main Tauri window must keep `dragDropEnabled: false`: Tauri native file-drop interception otherwise suppresses HTML5 pool drag/drop on Windows. See [Tauri configuration](https://v2.tauri.app/reference/config/#windowconfig). Provider sign-in windows retain their independent security configuration.

The engine records bounded routing metadata in `routing/reports.rs`. New inference adapters appear in Reports through their existing account/provider IDs, without UI branches or an extra callback. Keep failure messages static and safe; raw upstream bodies must never enter reports. The request owns a report guard, which moves into the streaming worker and finalizes on completion, error or cancellation. Native `routing_report` and `clear_routing_history` commands are restricted to the local main window; there is no public HTTP reports endpoint. Verify adapter failure/reset behavior through the router so the report reflects the same decisions as inference.
