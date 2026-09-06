# AI Usage

A Rust and Tauri 2 app for Windows that shows AI account usage in the system tray and provides an optional local model router.

**Version 0.4.0** adds OpenCode Go and Ollama Cloud subscription routing, including GLM-5.3-Flash, account failover and OpenCode client setup. It retains multiple accounts, usage/reset bars, model aliases, explicit substitutions, streaming, the Ollama reset-time fix and the separate Codex-Spark meter removal.

| Page | What you can do |
| --- | --- |
| **Usage** | See every enabled account's reported allowances, balances, percentages remaining and reset times. |
| **Settings** | Connect accounts, add more accounts at a provider, configure models, and choose refresh/startup behavior. |
| **Routing** | Choose account priority, model substitutions, and the local API connection used by calling apps. |

**Routing supports OpenCode Go, eligible Ollama Cloud subscriptions, Ollama (local), vLLM (local or LAN), and verified OpenRouter free models.** Go and Ollama Cloud require the provider billing setup below to stop at included allowances. OpenAI/Codex, Anthropic, Suno and Higgsfield remain usage-monitoring connections only. A subscription usage bar alone does not enable inference. The router never selects a paid fallback.

[Install](#install-and-open) · [Connect accounts](#connect-accounts-and-see-usage) · [Set up routing](#set-up-routing) · [Calling-app example](#connect-a-calling-app) · [Troubleshooting](#troubleshooting) · [Build from source](#build-from-source)

This README describes the checked-out source version. `main` changes only after an owner-approved merge. To build a change still under review, check out that pull request's branch before running the build commands.

## Install and open

Use Windows x64 and Microsoft Edge WebView2. You do not need Rust or Node.js to run an installer supplied by the maintainer; those are only needed to build the app.

1. Run the **AI Usage 0.4.0 x64 NSIS installer**. It installs for the current Windows user and installs WebView2 if it is missing. Generated installers are outside Git; if you have source only, follow [Build from source](#build-from-source).
2. Launch **AI Usage** from the Start menu. If you cannot see its tray icon, open Windows' hidden-icons area.
3. Click or double-click the tray icon to open **Usage**. Right-click it for **Show usage**, **Settings**, or **Exit**.
4. Use the **Usage**, **Settings** and **Routing** tabs in the app window. Closing this window hides it; **Exit** stops the app and its router.

To upgrade, finish active routed requests, choose **Exit**, run the newer installer, then reopen AI Usage. Existing settings and account connections are preserved. In **Settings**, enable **Start with Windows** if you want the app to start with its window hidden. The router also starts when the app starts if you have enabled and saved it.

## Connect accounts and see usage

1. Open **Settings**. Expand a category, then a provider, then its account. Categories are **LLM**, **Music**, **Speech** and **Media**; Speech has no providers yet.
2. Enter an **Account label** so you can distinguish connections, such as `Personal`, `Work` or `Local server`.
3. Fill in the connection fields in the table below. For website connections, choose **Connect account**, sign in directly with the provider, complete any multifactor step, and close the sign-in window. For a key or local-server connection, enter its fields and choose **Connect account**. Connecting saves the fields and enables the account.
4. Open **Usage** and choose **Refresh all**, or **Refresh** on one account. A successful read adds a connection checkmark in Settings and shows that account's meters. Usage monitoring sends no generation requests.
5. To change saved options later, select **Enable this account** as needed and choose **Save settings**. Automatic refresh defaults to five minutes; change **Refresh every** in Settings to any value from 1 to 60 minutes and choose **Save**.

Use **Add another account** inside a provider for another connection. Each account has its own label, saved keys, website session, usage readings and model mappings. Existing connections migrate without changing their credential targets or browser profiles. Multiple **Use signed-in Codex** connections follow the same installed Codex login; use independent website connections to monitor separate OpenAI accounts.

## Included providers

| Provider | Category | What to enter or connect | What appears on Usage |
| --- | --- | --- | --- |
| OpenAI | LLM | **Sign in to ChatGPT**, or **Use signed-in Codex** if the installed Codex app/CLI is already signed in. Leave **Codex executable** blank for automatic discovery. **ChatGPT account ID** is optional for a specific website workspace. | Codex subscription windows and any additional credit balance. Other ChatGPT chat-model caps are not exposed by this source. |
| Anthropic | LLM | Sign in to Claude. Leave **Organization ID** blank for automatic selection; specify the subscribed organization if the reader reports several choices. | Five-hour and weekly usage, available model-specific windows and enabled extra-usage spending allowance. |
| Ollama Cloud | LLM | Sign in for usage. Routing additionally needs an **Ollama API key** for that account and eligible **Subscription billing** settings. | Monthly included-credit spending and any session, hourly or weekly percentages shown in settings, with each window's reported reset date and local time. |
| OpenRouter | LLM | Choose **Account credits (management key)** or **This key's allowance (standard key)**, then enter the matching **OpenRouter key**. | Account USD balance, or the selected key's remaining spending allowance. |
| OpenCode | LLM | Enter an **OpenCode API key** from the workspace/member with an active Go subscription. | Go five-hour, weekly and monthly percentages and reset times. |
| Suno | Music | Sign in to Suno. **Total-credit reference allowance** is optional. | Monthly subscription credits and total credits, including top-ups. |
| Higgsfield | Media | Sign in and select the intended workspace on Higgsfield. **Total-credit reference allowance** is optional. | Subscription allowance, total wallet and any auto-refill balance for that workspace. |
| Ollama (local) | LLM | A running server's **Server URL**, usually `http://127.0.0.1:11434`. **Server API key** is optional. | Server connectivity and eligible local model count. There is no subscription quota. |
| vLLM (local) | LLM | A running server's **Server URL**, usually `http://127.0.0.1:8000`, and **Server API key** if authentication is enabled. | Server connectivity and model count. There is no subscription quota. |

OpenAI's source reports **Codex usage**. The separate **GPT-5.3-Codex-Spark** five-hour and weekly meters are omitted; the regular Codex allowances retain their reported percentages and reset times. It does not expose all ChatGPT chat-model caps. Consumer subscription allowances are separate from API billing. No model generation, paid API calls or usage-reset purchases are part of these readers.

For **OpenRouter**, choose **Account credits (management key)** to see the account balance. Obtain a management key in your OpenRouter account settings. The percentage compares the remaining balance with the provider's total purchased credits, not a recurring monthly budget. **This key's allowance (standard key)** uses the key's own configured daily, weekly, monthly or lifetime spending cap. An uncapped key has no remaining allowance to calculate; this does not mean its account has unlimited credits. Management-key connections make only read-only usage requests.

For **OpenCode**, use an API key from the workspace and member with an active **Go** subscription. The three meters use OpenCode's reported consumption and reset timestamps, without hardcoded dollar limits. **Zen pay-as-you-go wallet credits and usage of other providers through the OpenCode CLI are not included** in this connection. Track other providers with their respective adapters.

Balances without a reported denominator display **Percent unavailable**. Suno and Higgsfield offer an optional reference allowance for their total-credit bars; it is your comparison value, not a provider-reported limit. Separate subscription bars use the provider's reported allowance. Reset times are displayed only when supplied; reaching a reset time never invents a new balance.

The website readers depend on the providers' current website contracts, which can change. A login challenge or an expired session is shown as a connection problem. Open **Connect account** to resolve it yourself. No browser challenges are bypassed. Multiple Claude organizations require the organization ID in that provider's settings.

Failed refreshes retain the last successful reading for the current app session and show an error. If a reset time passes, the card says **Reset time passed. Refresh to confirm.** It does not assume the allowance refilled. Local-server cards show connectivity and model counts with **Percent unavailable**, because local capacity is not a subscription allowance.

## Set up routing

You need a supported account or local server, at least one saved model mapping, and a calling app on the same Windows PC that supports an **OpenAI-compatible Chat Completions** connection. AI Usage must stay running in the tray. Routing starts disabled.

### 1. Prepare a supported connection

**OpenCode Go, then Ollama Cloud**

These are upstream providers. The **OpenCode application** is a separate client that connects to the AI Usage router.

1. In your Go workspace, turn **Use balance off** and remove any bring-your-own-provider keys. Go can otherwise charge Zen balance or use those keys. In **AI Usage > Settings > LLM > OpenCode**, save that workspace's API key. Under **Subscription billing**, select **Go only: Use balance off, no BYOK** after checking both settings.
2. For the backup, **Ollama Cloud** currently supports its legacy session/weekly subscription with no extra usage credits. Sign in for usage, save an **Ollama API key (routing)** for the same account, and confirm **Legacy session/weekly plan, no extra credits** under **Subscription billing**. The newer monthly-credit plans are not eligible for included-only routing in this version. They can automatically spend extra credits after included credits.
3. On both accounts, choose **List server models** and add `glm-5.3-flash`. Use `glm-5.3-flash` as both **Client model** and **Server model ID**. The direct Ollama Cloud API uses this ID without `:cloud`.
4. Enable each account and its routing toggle, save, then put **OpenCode above Ollama Cloud** on the Routing page. No model-substitution rule is needed because both mappings use the same model name.

Go's API checks all three allowance windows before every request. A depleted window skips Go until the last applicable reset. A provider rejection at submission, including a concurrent quota exhaustion, can hand the request to Ollama Cloud. On subsequent requests Go regains priority once a fresh check confirms allowance. Ollama determines its availability at submission; its 429/402 responses skip the account. The router honors `Retry-After`, or rechecks after 60 seconds when the provider supplies no retry time. If both providers are exhausted it returns an error.

**Provider billing settings are required.** These APIs have no verified per-request switch that prohibits paid balance. The saved billing selection records your confirmation; it does not read or change provider billing settings. Leave subscription routing disabled until the conditions above are true, and disable it before changing those conditions. Usage monitoring works without confirming billing. Credentials stay in Windows Credential Manager. Model listing and Go quota checks do not generate text.

Model catalogs are discovered through each adapter. You can map other catalog models using the same controls; the selected model must support Chat Completions and the request's tools/options. GLM-5.3-Flash is the initial integration target. A Responses-only or Messages-only model requires a separate protocol adapter.

**Ollama on this PC**

Install and start [Ollama for Windows](https://docs.ollama.com/windows). It normally runs in the background on port 11434. Download a local chat model using the [Ollama CLI](https://docs.ollama.com/cli). For example, [llama3.2:1b](https://ollama.com/library/llama3.2:1b) is a small local model you can use for a first connection:

```powershell
ollama pull llama3.2:1b
ollama list
```

In AI Usage, choose **Settings > LLM > Ollama (local)**, enter `http://127.0.0.1:11434` as **Server URL**, leave **Server API key** blank for a normal local installation, and choose **Connect account**. **Ollama Cloud** is the separate hosted subscription connection.

Only models verified as local are eligible. Cloud-backed aliases are excluded. If you also want to disable cloud features in Ollama itself, set the user environment variable `OLLAMA_NO_CLOUD=1` and restart Ollama. This disables Ollama's cloud models and web search; see its [local-only configuration instructions](https://docs.ollama.com/faq#how-do-i-disable-ollama-cloud-features).

**vLLM on your hardware**

Install vLLM using its [quickstart](https://docs.vllm.ai/en/latest/getting_started/quickstart/) for your hardware and serving environment. Start an [OpenAI-compatible server](https://docs.vllm.ai/en/latest/serving/online_serving/openai_compatible_server/) with a chat-capable model and its required chat template. Replace `YOUR_CHAT_MODEL_ID` below with the model you will serve:

```bash
vllm serve "YOUR_CHAT_MODEL_ID" --host 127.0.0.1 --port 8000
```

In AI Usage, choose **Settings > LLM > vLLM (local)**, set **Server URL** to `http://127.0.0.1:8000`, enter your server's key in **Server API key** if it requires authentication, and choose **Connect account**. A server in WSL or a container must expose its port to the Windows host. For a server on another machine, configure its listener for your LAN and use its private IP, such as `http://192.168.1.50:8000`.

Local server URLs accept a root URL or `/v1`. They accept `localhost`, loopback IPs and private LAN IPs; public hosts and custom DNS names are rejected. These connections are for model servers running on your hardware. AI Usage does not install models, start these servers, or manage their hardware.

**OpenRouter free models**

Choose **Settings > LLM > OpenRouter**, set **Usage source** to **This key's allowance (standard key)**, enter a standard OpenRouter API key, and choose **Connect account**. A management key is for account-balance monitoring and cannot be used for routing. You can add a second OpenRouter connection if you want management-key balance monitoring as well.

**List server models** returns only explicit `:free` IDs whose catalog prices are zero. Choose from that list; paid models and automatic selectors are excluded. The router enforces zero-price limits and disables provider-side fallback. The USD usage bar does not report how many free requests remain: OpenRouter determines availability on each request, and a rate-limit response can move the request to your next configured route.

### 2. Save model names for each account

1. Save or connect the account before listing models. **List server models** uses the saved URL and key.
2. In that account's **Model routing** section, choose **List server models**, select a model, then choose **Add selected model**. You can also choose **Add model** and enter an exact server ID yourself.
3. Set **Client model** to the name the calling app will request, for example `writer`. Leave **Server model ID** as the exact ID returned by the server, including any tag or `:free` suffix. For the Ollama example above, map `writer` to `llama3.2:1b`.
4. Turn on both **Enable this account** and **Use this account for routing**, then choose **Save settings**.
5. Repeat for each account or server you want to use. Give equivalent models the same **Client model** name on several accounts to make them backups for that name. Different model names require a fallback rule if you want automatic substitution.

Providers without a routing adapter show **Usage monitoring only** instead of model controls. Connecting them for usage does not enable subscription routing in this version.

### 3. Choose account priority and model substitutions

Open the **Routing** tab. Under **Account priority**, use the up/down arrows to put your preferred account first. Accounts marked **Routing off** are skipped. The order applies to every requested model.

For a different model as backup, first save a mapping for that model on an eligible account, such as client name `small`. Under **Model substitutions**, choose **Add fallback rule**, enter `writer` as **Requested model**, and `small` as **Alternatives, in order**. Multiple alternatives are comma-separated, for example `small, offline`. These are client model names from your mappings. Choose **Save routing settings** to apply the order and rules.

For a `writer` request, the router:

1. Tries accounts mapping `writer` in priority order.
2. If none can serve it, tries accounts mapping `small`, then any later configured alternatives. An exact requested-model match takes precedence over every substitution.
3. Skips accounts or models with an active quota/retry cooldown. Once the provider's reset or retry time passes, the preferred account is checked again on the next request. It regains priority if available.
4. Returns an error when no eligible route remains. It never chooses an unconfigured alternative or switches to a paid request.

Fallback rules can point to other rules; traversal is breadth first and cycles are rejected. Failover occurs only when it is safe to try another route. An ambiguous submission failure or an interrupted response stream is not replayed on another account. Saving account or routing settings cancels active routed requests, so finish important requests before changing configuration.

### 4. Enable the local API

1. Under **Routing > Connect a calling app**, leave **Port** at `43129` or choose an unused port from 1024 to 65535.
2. Choose **Generate key**, then **Copy new key**. Save that key in your calling app before saving this form. It is the AI Usage client key, separate from every provider/server key.
3. Turn on **Enable local router** and choose **Save routing settings**. The status should change to **Listening**.
4. Use the displayed base URL, normally `http://127.0.0.1:43129/v1`, in the calling app. Keep AI Usage and the selected model server running.

The saved client key cannot be shown again. Leave its field blank when changing other settings. If you lose it, generate, copy and save a replacement, then update every calling app. Generating a key does not replace the active key until you save.

## Connect a calling app

### OpenCode

1. Merge the provider from [examples/opencode/opencode.json](examples/opencode/opencode.json) into your project's `opencode.json`, preserving existing providers. The example starts with `glm-5.3-flash` and conservative client limits of 128K context and 16K output; these are client limits, not a claim about the model's maximum capacity.
2. In OpenCode, use `/connect`, choose **Other**, enter provider ID **ai-usage**, and save the **AI Usage client key**. Provider IDs must match exactly. OpenCode stores this key in its own native auth file; keep keys out of project JSON and Git.
3. Copy [ai-usage-session.js](examples/opencode/ai-usage-session.js) into your project's `.opencode/plugins/` directory. It forwards only an opaque conversation ID to the router so Go receives its session header. It does not read credentials, prompts or project files.
4. Start a new OpenCode session in the project and use `/models` to select **AI Usage local router > GLM-5.3-Flash**, or `opencode --model ai-usage/glm-5.3-flash`. Your conversation continues through the same local provider while AI Usage selects Go or Ollama Cloud underneath.

To make another router model selectable, save its mappings in AI Usage and add its **Client model** name under `provider.ai-usage.models` in OpenCode's config. Choose client context/output limits supported by every intended upstream. `opencode models ai-usage` lists configured OpenCode choices; authenticated `GET /v1/models` lists the router's active mappings. Neither list proves a live inference request succeeded.

### Other clients

| Client setting | Value |
| --- | --- |
| Connection/API type | OpenAI-compatible **Chat Completions** |
| Base URL | `http://127.0.0.1:43129/v1`, or the URL displayed on Routing |
| API key | The **client key** generated and saved in AI Usage |
| Model | A saved **Client model** name, such as `writer` |
| Streaming | Supported through server-sent events when the selected upstream supports it |

If a client asks for the complete chat endpoint instead of a base URL, use `http://127.0.0.1:43129/v1/chat/completions`. Do not append `/v1` twice. A Responses-only or Anthropic Messages-only client cannot use this API directly. Embeddings, image/music/audio generation and native agent execution are outside this API.

The listener is restricted to this PC's IPv4 loopback. Both endpoints require `Authorization: Bearer <client key>`. Direct browser JavaScript requests with an `Origin` header are rejected; use a native client, command-line tool, or backend running on this PC. A browser tab opened at the base URL is not a connection test.

### Try it from PowerShell

Configure the `writer` mapping and enable the router first. This example prompts for the client key without putting it in command history, lists configured model names, then sends one short chat request. The list describes configuration, not guaranteed current model availability.

```powershell
$usageBaseUrl = 'http://127.0.0.1:43129/v1'
$usageClientKey = Read-Host 'Paste the AI Usage client key' -AsSecureString
$usageCredential = [PSCredential]::new('client', $usageClientKey)
$usageHeaders = @{ Authorization = 'Bearer ' + $usageCredential.GetNetworkCredential().Password }

try {
    $usageModels = Invoke-RestMethod -Uri "$usageBaseUrl/models" -Headers $usageHeaders -ErrorAction Stop
    $usageModels.data | Select-Object id

    $usageBody = @{
        model = 'writer'
        messages = @(@{ role = 'user'; content = 'Reply with one short greeting.' })
        stream = $false
        max_tokens = 64
    } | ConvertTo-Json -Depth 6

    $usageReply = Invoke-RestMethod -Method Post -Uri "$usageBaseUrl/chat/completions" -Headers $usageHeaders -ContentType 'application/json' -Body $usageBody -ErrorAction Stop
    $usageReply.choices[0].message.content
}
finally {
    $usageHeaders.Clear()
    $usageClientKey.Dispose()
    Remove-Variable usageCredential, usageClientKey
}
```

For a streaming client, use the same endpoint with `"stream": true` and consume the SSE stream. Send an optional `x-ai-usage-session` header containing a stable opaque conversation ID (1-200 letters, numbers, underscores or hyphens). `x-opencode-session` is also accepted. Without one, the router creates an ID for that request. Only Go receives this session metadata; arbitrary caller headers are not forwarded. Successful responses include `x-ai-usage-account`, `x-ai-usage-requested-model`, `x-ai-usage-model` and `x-ai-usage-upstream-model` headers so a client can identify the selected account and model. The response body's model remains the upstream ID.

The [HTML routing guide](docs/ROUTING.html) has the complete supported request fields, error contract, limits, selection rules and provider extension interface. GitHub displays HTML as source; download or clone it and open `docs/ROUTING.html` in a browser for the formatted guide.

## Local data

Configuration is stored at `%LOCALAPPDATA%\com.aiusagetray.desktop\settings.json`. Settings contain nonsecret options, not account passwords or copied access tokens. Website sessions are kept in separate WebView2 profiles under the same app directory. The providers handle website authentication directly.

Provider/server keys and the router client key are stored as generic credentials in **Windows Credential Manager**, private to the current Windows account, with target names starting `com.aiusagetray.desktop/`. Keys are sent from the local password field to native code for storage, never saved in settings JSON or returned to the frontend. A blank password field preserves the saved key; entering a replacement updates it. Native cloud usage reads use HTTPS. Local model connections may use HTTP on loopback or a private LAN. Requests reject redirects, honor cancellation, and have time and size limits.

The router forwards your prompt to the selected provider or local server. AI Usage does not log or persist prompts or responses; the selected server or provider has its own data handling. Ordinary settings contain account labels, nonsecret connection options, model mappings and routing order.

Source and installer builds do not include those local profiles or existing Codex credentials. Each recipient connects their own accounts. Distribute the source or generated installer, never a copy of the app's runtime data directory.

Native app commands are restricted to the local main window. Remote sign-in pages have no app-command permissions. Readers run only on their declared HTTPS hostnames and return usage fields. The Codex connection uses the installed app's existing login and requests `account/rateLimits/read` over its local app server.

**Forget** clears this app's saved keys, website session and settings for the selected account. It does not revoke the key at the provider, cancel a subscription or sign out a separate Codex installation. Disabling an account stops its refreshes and routing, and closes its provider windows while preserving its connection for later use. Usage snapshots are held in memory, so a fresh app launch checks enabled accounts again. Use Forget on accounts you want removed before uninstalling; uninstalling the program preserves account data, including the saved router client key.

## Troubleshooting

| What you see | What to check |
| --- | --- |
| No accounts on Usage | Connect an account or turn on **Enable this account** and choose **Save settings**. Then use **Refresh all**. |
| Sign-in required or **Needs attention** | Open that account's **Connect account** window, finish sign-in or the provider's challenge, close it, and refresh. Old readings may remain visible with the error. |
| No reset time or **Percent unavailable** | The source did not provide a usable timestamp or denominator. Local servers have no subscription percentage. For Ollama's split reset layout, use 0.3.1 or later. |
| **Usage monitoring only** / **Routing off** | Check the supported-provider list above. A supported account needs both enable toggles, saved model mappings and **Save settings**. |
| **No eligible models found** | Save the URL/key before listing, check that the server is running and has a chat model installed, and use its exact model ID. Ollama cloud aliases and non-free OpenRouter models are excluded. |
| Router **Stopped** or connection refused | Start AI Usage, save a client key, enable the router and save. If the port is busy, choose another port and update the calling app's base URL. |
| HTTP 401 | Use the AI Usage client key, not a provider key. Check that a new key was saved. Use the displayed loopback URL and a native/backend client without a browser Origin header. |
| HTTP 400 | Check the model name, JSON body and supported fields. Use Chat Completions rather than Responses/Messages. Provider routing overrides and unsupported paid extensions are rejected. |
| HTTP 429 | Read the error's `attempts` and `retry_at` fields when present. No configured route may be eligible, an upstream may be rate-limited, or all eight request slots may be busy. Check both account toggles and mappings, then wait for retry/reset or configure an eligible alternative. |
| HTTP 502 / 504 | Check the selected server's availability and supported chat parameters. A request that may already have been submitted is not automatically replayed. |
| HTTP 503 or a stream stops during configuration | Saving settings cancels active routing work. Finish saving, confirm **Listening**, and submit a new request if appropriate. |
| Need to remove one connection | Use **Forget** on that account. Other accounts are preserved. Turning an account off preserves its saved connection for later use. |

## Build from source

Use Windows x64, Git, a current stable Rust MSVC toolchain, Node.js 22.12 or later, and Visual Studio Build Tools with the C++ desktop workload and Windows SDK. WebView2 is needed to run the app. Both lockfiles are included. The Tauri CLI obtains NSIS when building the installer.

In PowerShell, from the directory where you want the source:

```powershell
git clone https://github.com/tullyedson/tokenusage.git
cd tokenusage
npm.cmd ci
powershell -ExecutionPolicy Bypass -File scripts/build.ps1
```

The build script prepares a local test-temp directory, runs frontend tests, Rust tests and Clippy, then builds the production frontend and NSIS installer. The default outputs for this version are:

- `src-tauri/target/release/bundle/nsis/AI Usage_0.4.0_x64-setup.exe`, the installer to distribute.
- `src-tauri/target/release/ai-usage-tray.exe`, the app executable you can run directly.

If `CARGO_TARGET_DIR` is set, the native outputs are under that directory instead. Build outputs, dependencies and account data are ignored by Git. Building the installer does not run it.

For native development, run `npm.cmd run desktop`. For a browser-only layout preview, run `npm.cmd run dev` and open `http://127.0.0.1:1420`. That preview uses labelled sample readings; account sign-in, saved keys and real routing require the desktop app. Preview fixtures are excluded from production assets.

To run the checks individually from the project root:

```powershell
npm.cmd test
npm.cmd run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm.cmd run installer
```

An optional integration test reads the existing Codex account's usage. It is ignored by default and does not print credentials or usage values:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml signed_in_codex_returns_usage -- --ignored
```

Subscription fixture tests exercise the concrete Go and Ollama Cloud adapters with real local HTTP transport, quota exhaustion, reset recovery, session headers, streaming tool calls, separate keys and rejection of unconfirmed billing. See [Verification](docs/VERIFICATION.md) for completed checks and the exact scope of live account verification.

## Add providers and contribute

Provider metadata drives both Settings and Usage. A new provider implements `IUsageProvider` and registers once. Optional inference support goes through `IInferenceProvider`, which supplies model discovery and eligibility checks. See [Adding providers](docs/ADDING_PROVIDERS.md) and the [routing contract](docs/ROUTING.html). Core account ordering and the HTML pages do not need provider-specific branches.

Follow [Contributing](CONTRIBUTING.md): submit changes through a pull request. Contributions remain outside `main` until the repository owner explicitly chooses to merge them. Automatic merging is disabled. Never commit provider keys, browser profiles, runtime settings or personal account data.

## Source references

- [Tauri tray events](https://v2.tauri.app/learn/system-tray/), [NSIS packaging](https://v2.tauri.app/distribute/windows-installer/) and [capability boundaries](https://v2.tauri.app/security/capabilities/).
- [OpenAI app server documentation](https://developers.openai.com/codex/app-server/) for the Codex account usage request.
- [Claude usage windows](https://code.claude.com/docs/en/statusline), [Suno account page](https://suno.com/account), [Higgsfield credit guidance](https://higgsfield.ai/creator-hub/help-center/credits/how-credits-work) and [Ollama Cloud documentation](https://docs.ollama.com/cloud).
- [OpenRouter account credits](https://openrouter.ai/docs/api/api-reference/credits/get-remaining-credits), [current-key allowances](https://openrouter.ai/docs/api/api-reference/api-keys/get-current-key), and [key reset semantics](https://openrouter.ai/docs/api/api-reference/api-keys/create-keys).
- [OpenCode Go](https://opencode.ai/docs/go/), [Zen](https://opencode.ai/docs/zen/), and the provider's [Go usage endpoint implementation](https://github.com/anomalyco/opencode/blob/dev/packages/console/app/src/routes/zen/go/v1/usage.ts). This endpoint is visible in the provider's public source; it is not a documented compatibility promise.

The website endpoints and DOM readers in this repository are integrations with those sites, not claims of supported public consumer APIs. No provider affiliation is implied.
