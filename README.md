# AI Usage

A Rust and Tauri 2 app for Windows that keeps your AI subscription allowances in the system tray. The local HTML interface shows each provider's actual usage windows, remaining balances, percentages and reset times.

## Install and use

Run the AI Usage NSIS installer, or build it using the instructions below. Generated installers are kept outside source control. The installer installs for the current Windows user and offers to install Microsoft Edge WebView2 if it is missing.

1. Open **AI Usage**, choose **Settings**, then expand a category and provider.
2. For website connections, choose **Connect account** and sign in directly on the provider's website, including any multifactor authentication. Close that window after sign-in to refresh usage. For key connections, paste the key into the provider's password field and choose **Connect account**.
3. For OpenAI, you can instead select **Use signed-in Codex** to read the account already connected to the installed Codex app or CLI. Leave the executable path empty for automatic discovery.
4. A check appears after the first successful reading. Each enabled provider appears on the Usage page. Unavailable readings show an error, with the last successful reading retained for the current app session.

Click or double-click the tray icon to show Usage. Its right-click menu contains **Show usage**, **Settings** and **Exit**. Closing the main window hides it in the tray. **Exit** stops it. **Start with Windows** is optional and starts the app with its window hidden. Automatic refresh defaults to five minutes and is configurable from one to sixty minutes.

Settings categories are **LLM**, **Music**, **Speech** and **Media**. Speech is present but has no initial providers.

## Included providers

| Provider | Category | Reading |
| --- | --- | --- |
| OpenAI | LLM | Codex subscription windows and additional credit balance, through the signed-in Codex app server or ChatGPT's Codex usage page |
| Anthropic | LLM | Claude's five-hour and weekly windows, available model-specific windows and enabled extra-usage spending allowance |
| Ollama Cloud | LLM | Monthly included-credit spending and any session, hourly or weekly percentages shown in settings |
| OpenRouter | LLM | Account USD credit balance with a management key, or a standard key's remaining spending allowance |
| OpenCode | LLM | Go subscription five-hour, weekly and monthly percentages and reset times, using an API key from the subscribed workspace |
| Suno | Music | Monthly subscription credits and total remaining credits, including top-ups |
| Higgsfield | Media | Subscription allowance, total wallet and any auto-refill credit balance for the current workspace |

OpenAI's source reports **Codex usage**. It does not expose all ChatGPT chat-model caps. Consumer subscription allowances are separate from API billing. No model generation, paid API calls or usage-reset purchases are part of these readers.

For **OpenRouter**, choose **Account credits (management key)** to see the account balance. Obtain a management key in your OpenRouter account settings. The percentage compares the remaining balance with the provider's total purchased credits, not a recurring monthly budget. **This key's allowance (standard key)** uses the key's own configured daily, weekly, monthly or lifetime spending cap. An uncapped key has no remaining allowance to calculate; this does not mean its account has unlimited credits. The app only makes read-only usage requests even when a management key has broader permissions.

For **OpenCode**, use an API key from the workspace and member with an active **Go** subscription. The three meters use OpenCode's reported consumption and reset timestamps, without hardcoded dollar limits. **Zen pay-as-you-go wallet credits and usage of other providers through the OpenCode CLI are not included** in this connection. Track other providers with their respective adapters.

Balances without a reported denominator display **Percent unavailable**. Suno and Higgsfield offer an optional reference allowance for their total-credit bars; it is your comparison value, not a provider-reported limit. Separate subscription bars use the provider's reported allowance. Reset times are displayed only when supplied; reaching a reset time never invents a new balance.

The website readers depend on the providers' current website contracts, which can change. A login challenge or an expired session is shown as a connection problem. Open **Connect account** to resolve it yourself. No browser challenges are bypassed. Multiple Claude organizations require the organization ID in that provider's settings.

## Local data

Configuration is stored at `%LOCALAPPDATA%\com.aiusagetray.desktop\settings.json`. Settings contain nonsecret options, not account passwords or copied access tokens. Website sessions are kept in separate WebView2 profiles under the same app directory. The providers handle website authentication directly.

OpenRouter and OpenCode keys are stored as generic credentials in **Windows Credential Manager**, private to the current Windows account, with target names starting `com.aiusagetray.desktop/`. Keys are sent from the local password field to native code for storage, never saved in settings JSON or returned to the frontend. A blank password field preserves the saved key; entering a replacement updates it. Native requests use HTTPS, reject redirects, honor cancellation, and limit request time and response size. Errors never display provider response bodies or authorization headers.

Source and installer builds do not include those local profiles or existing Codex credentials. Each recipient connects their own accounts. Distribute the source or generated installer, never a copy of the app's runtime data directory.

Native app commands are restricted to the local main window. Remote sign-in pages have no app-command permissions. Readers run only on their declared HTTPS hostnames and return usage fields. The Codex connection uses the installed app's existing login and requests `account/rateLimits/read` over its local app server.

**Forget** clears this app's saved keys, website session and settings for that provider. It does not revoke the key at the provider, cancel a subscription or sign out a separate Codex installation. Disabling a provider stops its refreshes and closes its provider windows while preserving its connection for later use. Usage snapshots are held in memory, so a fresh app launch checks enabled accounts again. Use Forget before uninstalling if you want to remove saved credentials; uninstalling the program preserves account data.

## Build from source

Use Windows x64, a current stable Rust MSVC toolchain, Node.js 22.12 or later, and Visual Studio Build Tools with the C++ desktop workload and Windows SDK. WebView2 is needed to run the app. Both lockfiles are included. The Tauri CLI obtains NSIS when building the installer.

```powershell
npm.cmd ci
powershell -ExecutionPolicy Bypass -File scripts/build.ps1
```

The build script runs frontend and Rust tests and creates the NSIS installer under `src-tauri/target/release/bundle/nsis/`. For native development, run `npm.cmd run desktop`. For a browser-only layout preview, run `npm.cmd run dev`; its clearly labelled sample data is excluded from production builds.

```powershell
npm.cmd test
npm.cmd run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

An optional integration test reads the existing Codex account's usage. It is ignored by default and does not print credentials or usage values:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml signed_in_codex_returns_usage -- --ignored
```

See [Adding providers](docs/ADDING_PROVIDERS.md) for the adapter contract and [Verification](docs/VERIFICATION.md) for release checks and outstanding account checks.

## Source references

- [Tauri tray events](https://v2.tauri.app/learn/system-tray/), [NSIS packaging](https://v2.tauri.app/distribute/windows-installer/) and [capability boundaries](https://v2.tauri.app/security/capabilities/).
- [OpenAI app server documentation](https://developers.openai.com/codex/app-server/) for the Codex account usage request.
- [Claude usage windows](https://code.claude.com/docs/en/statusline), [Suno account page](https://suno.com/account), [Higgsfield credit guidance](https://higgsfield.ai/creator-hub/help-center/credits/how-credits-work) and [Ollama Cloud documentation](https://docs.ollama.com/cloud).
- [OpenRouter account credits](https://openrouter.ai/docs/api/api-reference/credits/get-remaining-credits), [current-key allowances](https://openrouter.ai/docs/api/api-reference/api-keys/get-current-key), and [key reset semantics](https://openrouter.ai/docs/api/api-reference/api-keys/create-keys).
- [OpenCode Go](https://opencode.ai/docs/go/), [Zen](https://opencode.ai/docs/zen/), and the provider's [Go usage endpoint implementation](https://github.com/anomalyco/opencode/blob/dev/packages/console/app/src/routes/zen/go/v1/usage.ts). This endpoint is visible in the provider's public source; it is not a documented compatibility promise.

The website endpoints and DOM readers in this repository are integrations with those sites, not claims of supported public consumer APIs. No provider affiliation is implied.
