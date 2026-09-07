# AI Usage

A Rust and Tauri 2 app for Windows that shows AI account usage in the system tray and provides an optional local model router.

**Version 0.7.0** adds sticky load distribution alongside ordered failover. Each pool has its own route type. Provider context/input/output limits pass through the router and into OpenCode. Each model pool advertises the lowest supported limits across its enabled entries. The Models page shows these limits; Reports shows active destinations and recent fallback history. Create common names such as `flash-models` on Models, drag in models from different providers or local servers, and choose how to route them. Calling apps use that one name while the router follows the pool's selection policy. Plan-only routing is the default for supported accounts.

| Page | What you can do |
| --- | --- |
| **Usage** | See every enabled account's reported allowances, balances, percentages remaining and reset times. |
| **Settings** | Connect accounts, add more accounts at a provider, and choose refresh/startup behavior. |
| **Models** | Browse every discovered model, create common names, and choose failover or sticky load distribution. |
| **Reports** | Follow active pipelines, see their destinations, and inspect recent requests and fallback steps. |
| **Routing** | Enable the local API and manage its port and client key. |

**Routing supports OpenCode Go, eligible Ollama Cloud subscriptions, Ollama (local), vLLM (local or LAN), and verified OpenRouter free models.** Go and Ollama Cloud require the provider billing setup below to stop at included allowances. OpenAI/Codex, Anthropic, Suno and Higgsfield remain usage-monitoring connections only. A subscription usage bar alone does not enable inference. There is no paid fallback option in the app. Provider-side overages must also be disabled as described below.

[Install](#install-and-open) · [Connect accounts](#connect-accounts-and-see-usage) · [Set up routing](#set-up-routing) · [Calling-app example](#connect-a-calling-app) · [Troubleshooting](#troubleshooting) · [Build from source](#build-from-source)

This README describes the checked-out source version. `main` changes only after an owner-approved merge. To build a change still under review, check out that pull request's branch before running the build commands.

## Install and open

Use Windows x64 and Microsoft Edge WebView2. You do not need Rust or Node.js to run an installer supplied by the maintainer; those are only needed to build the app.

1. Run the **AI Usage 0.7.0 x64 NSIS installer**. It installs for the current Windows user and installs WebView2 if it is missing. Generated installers are outside Git; if you have source only, follow [Build from source](#build-from-source).
2. Launch **AI Usage** from the Start menu. If you cannot see its tray icon, open Windows' hidden-icons area.
3. Click or double-click the tray icon to open **Usage**. Right-click it for **Show usage**, **Settings**, or **Exit**.
4. Use the **Usage**, **Models**, **Reports**, **Routing** and **Settings** tabs in the app window. Closing this window hides it; **Exit** stops the app and its router.

To upgrade, finish active routed requests, choose **Exit**, run the newer installer, then reopen AI Usage. Existing settings and account connections are preserved. In **Settings**, enable **Start with Windows** if you want the app to start with its window hidden. The router also starts when the app starts if you have enabled and saved it.

## Connect accounts and see usage

1. Open **Settings**. Expand a category, then a provider, then its account. Categories are **LLM**, **Music**, **Speech** and **Media**; Speech has no providers yet.
2. Enter an **Account label** so you can distinguish connections, such as `Personal`, `Work` or `Local server`.
3. Fill in the connection fields in the table below. For website connections, choose **Connect account**, sign in directly with the provider, complete any multifactor step, and close the sign-in window. For a key or local-server connection, enter its fields and choose **Connect account**. Connecting saves the fields and enables the account.
4. Open **Usage** and choose **Refresh all**, or **Refresh** on one account. A successful read adds a connection checkmark in Settings and shows that account's meters. Usage monitoring sends no generation requests.
5. To change saved options later, select **Enable this account** as needed and choose **Save settings**. Automatic refresh defaults to five minutes; change **Refresh every** in Settings to any value from 1 to 60 minutes and choose **Save**.

Use **Add another account** inside a provider for another connection. Each account has its own label, saved keys, website session, usage readings and pool membership. Existing connections migrate without changing their credential targets or browser profiles. Multiple **Use signed-in Codex** connections follow the same installed Codex login; use independent website connections to monitor separate OpenAI accounts.

## Included providers

| Provider | Category | What to enter or connect | What appears on Usage |
| --- | --- | --- | --- |
| OpenAI | LLM | **Sign in to ChatGPT**, or **Use signed-in Codex** if the installed Codex app/CLI is already signed in. Leave **Codex executable** blank for automatic discovery. **ChatGPT account ID** is optional for a specific website workspace. | Codex subscription windows and any additional credit balance. Other ChatGPT chat-model caps are not exposed by this source. |
| Anthropic | LLM | Sign in to Claude. Leave **Organization ID** blank for automatic selection; specify the subscribed organization if the reader reports several choices. | Five-hour and weekly usage, available model-specific windows and enabled extra-usage spending allowance. |
| Ollama Cloud | LLM | Sign in for usage. Routing additionally needs an **Ollama API key** for that account and a plan that stops at its limits without extra credits. | Monthly included-credit spending and any session, hourly or weekly percentages shown in settings, with each window's reported reset date and local time. |
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

You need a supported account or local server, a discovered chat model or a saved pool, and a calling app on the same Windows PC that supports an **OpenAI-compatible Chat Completions** connection. AI Usage must stay running in the tray. Routing starts disabled.

### 1. Prepare a supported connection

**OpenCode Go, then Ollama Cloud**

These are upstream providers. The **OpenCode application** is a separate client that connects to the AI Usage router.

1. In your Go workspace, turn **Use balance off** and remove bring-your-own-provider keys. Go can otherwise charge Zen balance or use those keys. In **Settings > LLM > OpenCode**, save that workspace's API key and enable the account. **Include this account in model pools** is on by default.
2. For **Ollama Cloud**, use its legacy session/weekly subscription that stops at its limits, without extra usage credits or automatic top-ups. Save the same account's **Ollama API key (routing)** and enable the account. Monthly-credit accounts that can automatically spend extra credits are not supported for plan-only routing. Website usage sign-in is separate from the routing key.
3. Open **Models** and choose **Refresh models**. Every eligible model name from both catalogs appears automatically. For a preferred order, create or select `glm-5.3-flash` and put the Go entry first, then the Ollama Cloud entry. Choose **Save pools**. The direct Ollama Cloud API uses the model ID without `:cloud`.

Go checks all three allowance windows before every request. An exhausted window skips that account until its applicable reset, followed by a fresh quota check. Ollama determines availability at submission; 429/402 responses move to the next pool entry. The router honors `Retry-After`, or rechecks after 60 seconds when no retry time is provided. It stops when all pool entries are unavailable.

**Plan only is the app's default policy, with no paid fallback switch.** These provider APIs do not expose a verified per-request no-overage switch or a way to inspect their external billing settings. Keep paid overages, extra credits and automatic top-ups disabled at the provider. Disable an account's pool toggle before changing those conditions. The app does not change your provider billing settings. Credentials remain in Windows Credential Manager, and model discovery/quota reads do not generate text.

Catalog names are discovered through each provider adapter. A selected model must support Chat Completions and the requested tools/options. A Responses-only or Messages-only model requires a separate protocol adapter. OpenAI/Codex, Anthropic, Suno and Higgsfield still support usage monitoring only.

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

Local server URLs accept a root URL or `/v1`. They accept `localhost`, loopback IPs, private LAN IPs and `.local` hostnames, such as `http://node-a.local:11434`. The operating system must resolve the hostname. Every `.local` connection uses only the loopback/private addresses from that lookup; empty or mixed public/private answers are rejected. Other custom DNS names and public hosts are rejected. These connections are for model servers running on your hardware. AI Usage does not install models, start these servers, or manage their hardware.

**OpenRouter free models**

Choose **Settings > LLM > OpenRouter**, set **Usage source** to **This key's allowance (standard key)**, enter a standard OpenRouter API key, and choose **Connect account**. A management key is for account-balance monitoring and cannot be used for routing. You can add a second OpenRouter connection if you want management-key balance monitoring as well.

**Models > Refresh models** discovers only explicit `:free` IDs whose catalog prices are zero. Choose from that list; paid models and automatic selectors are excluded. The router enforces zero-price limits and disables provider-side fallback. The USD usage bar does not report how many free requests remain: OpenRouter determines availability on each request, and a rate-limit response can move the request to your next configured route.

### 2. Browse models and create a pool

Open **Models**. The left side shows models grouped by account, with search and an account filter. All eligible names are discovered automatically when you open this page or a client requests `/v1/models`. Catalogs are cached for five minutes; **Refresh models** reads them again. Failed reads retain the previous catalog with an error until the account settings change. The router checks actual availability again before generation.

1. Under **Common model name**, enter `flash-models` and choose **Create**. Spaces are converted to hyphens so `flash models` becomes `flash-models`. Names are case-sensitive.
2. Drag a model from the left into the pool. You can also select the pool in **Add to** and use a model's **Add** button.
3. Add more models from any supported account. Different model families can share a pool; they do not need the same provider name or model ID.
4. Drag entries to reorder them, or use the up/down arrows. Remove an entry with its × button.
5. Choose **Save pools**. The saved pool name is the model name other apps request. Empty pools can be saved but are not advertised to clients.

For example, a pool named `flash-models` could contain:

| Order | Account | Upstream model |
| --- | --- | --- |
| 1 | OpenCode Go | `glm-5.3-flash` |
| 2 | Ollama Cloud | Your chosen DeepSeek Flash model from its catalog |
| 3 | Ollama (local) | Your installed Qwen model from its catalog |

Use the exact IDs shown in your own catalog. The examples do not install models or assume that those models exist on every account.

**Automatic names:** Models with the same exact ID are grouped automatically across eligible accounts, in stable account-ID order. Expand **Show automatic model names** to inspect or customize those groups. Editing an automatic group saves a custom pool with that name. **Reset to automatic** discards that override. A custom pool name takes precedence over automatic discovery; there are no separate account-priority or fallback-rule controls.

Draft changes survive page navigation and catalog refresh. They take effect only after **Save pools**. A failed save leaves the draft intact.

### Choose a route type

Each pool has a **Route type** selector. Choose a type and **Save pools**. Existing pools and automatic names default to **Failover**.

| Route type | Selection behavior |
| --- | --- |
| **Failover** | Try entries from top to bottom. Each new request returns to the first available entry after its allowance resets or server recovers. |
| **Load distribution** | Keep a caller on its assigned account/model. For a new caller, choose an eligible entry with the fewest active requests, then the fewest assigned callers. Rotate ties in pool order. An unavailable server causes reassignment to another eligible entry. |

For example, connect three Ollama servers in Settings, each with its own account label and `.local` Server URL. Create `gemma4`, add each server's exact Gemma model ID from the left-hand catalog, choose **Load distribution**, and save. The model IDs may differ across servers. The shared context limit is still the lowest supported limit across all enabled entries.

A calling app should send **`x-ai-usage-instance`** with a stable, unique ID for that app instance, such as `writer-1`. Keep it the same on every request from that instance; use `writer-2` for another instance. IDs accept 1-200 ASCII letters, digits, underscores or hyphens. This identifier controls routing only and is never forwarded to providers. Do not use a password, API key, user name or prompt as an identifier.

- With an instance ID, all that instance's conversations stay on its assigned server for the requested pool.
- Without an instance ID, `x-ai-usage-session` (or `x-opencode-session`) provides stickiness per conversation. The existing OpenCode plugin supplies this automatically.
- Without either ID, requests are distributed independently. The shared API key and client IP are not treated as caller identity, so different apps on the same PC can spread across servers.

Stickiness takes priority over moving an existing caller to an idle server. Requests still serialize per account, including streams; different accounts can run concurrently within the router's eight-request limit. Load reflects requests seen by this router, not other clients' GPU usage. Selection reserves load before waiting for an account, and releases it on completion, disconnect or cancellation.

After a failure, the caller stays on its replacement; recovered entries can receive new callers. Assignments are held only in memory, expire after 30 minutes idle, and reset when accounts, pools or routing settings are saved or the app restarts. At most 4,096 caller/pool assignments are retained; the least recently used idle assignment is evicted when full. An active assignment is retained until its requests finish. Caller IDs never enter Reports or persisted settings.

Availability and plan-only checks apply to both route types. Only failures known to be safe to retry can select another entry. Ambiguous submissions, interrupted responses and streams are never replayed.

### Context and output limits

The Models page shows each discovered model's context and output limits, plus the current chain limit while you edit a pool. The router publishes `limit.context`, `limit.input` and `limit.output` in `/v1/models`, with `context_length` and `max_output_tokens` aliases for compatible clients. Values are token counts; `null` means unknown. Other clients must consume these extension fields or configure their own matching limits.

A chain uses the **lowest limit across all enabled, routing-eligible entries**, independent of order or remaining quota. For GLM-5.3-Flash on Go (1,000,000 context) and Ollama Cloud (1,048,576 context), the chain advertises **1,000,000**. Adding a 32,768-context local model lowers that chain to 32,768. Temporary depletion does not increase the chain limit. A missing model, failed catalog or unknown member bound makes that bound unknown; it is never ignored as unlimited. Disabled/excluded accounts do not participate until re-enabled. Save pool changes and restart OpenCode to refresh its limits.

| Source | Where limits come from |
| --- | --- |
| OpenCode Go | Exact provider/model metadata in OpenCode's public [Models.dev catalog](https://models.dev), supplementing its ID-only Go endpoint. |
| Ollama Cloud | `/api/show` context metadata, supplemented by exact `ollama-cloud` Models.dev input/output fields. |
| vLLM | The running server's `/v1/models`, including configured `max_model_len`. This can be smaller than the model's trained maximum. |
| Ollama (local) | `/api/show` model `num_ctx` parameter, bounded by the trained maximum. An unreported server default stays unknown. |
| OpenRouter free models | The eligible model's context and top-provider completion/context limits. |

The supplemental public catalog is read without credentials, cached for five minutes and bounded to 16 MiB. Failed refreshes do not reuse expired limits; names can still be discovered. Metadata reads never generate text, and names absent from an account's own eligible catalog are never added by the supplement.

For local Ollama, configure the context you intend to run in that model's Modelfile with `PARAMETER num_ctx`, then refresh models. The router does not increase GPU memory allocation or assume a theoretical 1M model runs with 1M context locally. See [Ollama context length](https://docs.ollama.com/context-length) and [Modelfile parameters](https://docs.ollama.com/modelfile#parameter).

The proxy accepts JSON requests up to **16 MiB**, including tools and message history. This replaces the old 1 MiB cap so normal long-context requests can reach the provider. Token limits and this byte limit are separate. The provider performs token counting and enforces its actual capacity; the proxy does not truncate prompts or estimate tokens from character counts.

### 3. How fallback works

For a `flash-models` request, the router tries that pool's entries from top to bottom. Each entry identifies one account and one upstream model. Disabled accounts, depleted allowances, unavailable models, and active cooldowns are skipped. A later exact-name match does not jump ahead of an earlier different model. Two models from the same account may appear in the same pool; duplicate account/model pairs are rejected.

Every new request starts at the beginning of the pool. After a quota reset or retry time, the preferred entry is checked again and regains its position when available. If all entries are unavailable, the router returns an error with account/model reasons and a retry time when known. It never chooses a model outside the requested pool.

JSON responses and streaming chunks expose the pool name in their `model` field. The actual selected model remains available in the `x-ai-usage-upstream-model` response header. Interrupted streams and ambiguous submission failures are not replayed on another account. Saving account, pool or routing settings cancels active requests, so finish important requests first.

Upgrading converts old aliases, account priority and model-substitution rules into explicit pools in the same effective order. Existing enabled/excluded accounts, keys and browser profiles are preserved. New supported accounts have **Include this account in model pools** enabled by default; the global router still starts disabled on a new installation.

### 4. Enable the local API

1. Under **Routing > Connect a calling app**, leave **Port** at `43129` or choose an unused port from 1024 to 65535.
2. Choose **Generate key**, then **Copy new key**. Save that key in your calling app before saving this form. It is the AI Usage client key, separate from every provider/server key.
3. Turn on **Enable local router** and choose **Save routing settings**. The status should change to **Listening**.
4. Use the displayed base URL, normally `http://127.0.0.1:43129/v1`, in the calling app. Keep AI Usage and the selected model server running.

The saved client key cannot be shown again. Leave its field blank when changing other settings. If you lose it, generate, copy and save a replacement, then update every calling app. Generating a key does not replace the active key until you save.

## See which pipeline and provider are in use

Open **Reports** while a connected app sends requests through AI Usage. No additional client setup is needed.

- **In use now** shows each active request's pool name, provider, account label, actual upstream model, elapsed time and selected pool position. It distinguishes finding a pool, waiting for an account, checking allowance, contacting a provider and streaming. Concurrent requests have separate cards.
- **Recent requests** keeps the last 100 finished requests. Expand a row to see the selected provider and skipped entries, with safe reasons and provider retry/reset times when available. Each new request gets its own report, so recovery to the preferred pool entry is visible after a reset.
- Search by pool, model, provider, account or request number. Filter completed, failed, cancelled or fallback requests. Counts describe retained history, not lifetime usage or provider billing.
- **Clear history** removes finished reports while active requests keep running. This does not change pools, settings, allowances or routing. Reports refresh once per second while the tab is open.

Reports label the route type and explain whether a caller stayed on its server or received a distributed assignment. Selecting entry 2 or 3 for distribution is not counted as fallback unless an earlier attempted entry failed or was skipped.

Reports contain routing metadata only and stay in memory until the app exits. They do not retain prompts, completions, tool arguments, session or instance IDs, keys, server URLs or raw provider errors. Only valid requests admitted to the router's eight active slots are recorded; model-list calls, authentication failures, malformed requests, disabled-router responses and busy rejections are not included. Request numbers restart with the app and are returned as `x-ai-usage-request-id` headers for correlation.

A streaming connection can return HTTP 200 and later fail. Reports mark success only after a completion marker and a clean upstream finish, and distinguish stream errors, truncation and cancellation. A completed report means the router received the response, not that the calling application acted on it. Failed streams are not replayed. Very long pools retain the latest 64 routing steps and show how many earlier steps were omitted.

## Connect a calling app

### OpenCode

1. Merge [examples/opencode/opencode.json](examples/opencode/opencode.json) into your project's `opencode.json`, preserving existing providers. The example leaves model limits to the discovery plugin.
2. In OpenCode, use `/connect`, choose **Other**, enter provider ID **ai-usage**, and save the **AI Usage client key**. Keep provider keys and the router client key out of project JSON and Git.
3. Copy [ai-usage-session.js](examples/opencode/ai-usage-session.js) into the project's `.opencode/plugins/` directory. It imports all model and pool names and their chain limits from the local router when OpenCode starts. It reads the `ai-usage` key from OpenCode's auth store (or an already configured API key), sends it only to the IPv4 loopback router, and forwards an opaque conversation ID. It never reads project source or prompts, and does not log keys.
4. Restart OpenCode in that project after saving or renaming pools. Use `/models` to select **AI Usage local router**, then the desired model or `flash-models`. You can also use `opencode --model ai-usage/flash-models` after creating that pool.

No individual JSON entry is needed for newly discovered model names. **The plugin synchronizes limits even for existing entries**, replacing old context/output guesses such as the earlier 131,072-context GLM example. Labels, options and other explicit model settings are preserved. An unknown chain limit uses an unverified **client budget** of 16,384 context or 4,096 output tokens, not an upstream capability claim. Unknown input limits are removed, and output/input budgets never exceed context. If the router is offline or an older router returns names only, existing explicit limits remain. Tool support is assumed for this integration; choose chat/tool-capable entries. Tested with OpenCode 1.18.29.

To update an existing connection, replace the project plugin with the current example and restart OpenCode. Merely changing the router does not refresh an already running OpenCode session. The plugin does not rewrite project JSON or the auth file.

`opencode models ai-usage` lists imported OpenCode choices. Authenticated `GET /v1/models` lists router model and pool names. Neither list proves an inference request succeeded or that a model supports every request option.

### Other clients

| Client setting | Value |
| --- | --- |
| Connection/API type | OpenAI-compatible **Chat Completions** |
| Base URL | `http://127.0.0.1:43129/v1`, or the URL displayed on Routing |
| API key | The **client key** generated and saved in AI Usage |
| Model | A discovered model ID or saved pool name, such as `flash-models` |
| Streaming | Supported through server-sent events when the selected upstream supports it |
| Sticky caller header | `x-ai-usage-instance: writer-1`, with a different stable ID per app instance |

If a client asks for the complete chat endpoint instead of a base URL, use `http://127.0.0.1:43129/v1/chat/completions`. Do not append `/v1` twice. A Responses-only or Anthropic Messages-only client cannot use this API directly. Embeddings, image/music/audio generation and native agent execution are outside this API.

The listener is restricted to this PC's IPv4 loopback. Both endpoints require `Authorization: Bearer <client key>`. Direct browser JavaScript requests with an `Origin` header are rejected; use a native client, command-line tool, or backend running on this PC. A browser tab opened at the base URL is not a connection test.

### Try it from PowerShell

Create the `flash-models` pool and enable the router first. This example prompts for the client key without putting it in command history, lists configured model names, then sends one short chat request. The list describes configuration, not guaranteed current model availability.

```powershell
$usageBaseUrl = 'http://127.0.0.1:43129/v1'
$usageClientKey = Read-Host 'Paste the AI Usage client key' -AsSecureString
$usageCredential = [PSCredential]::new('client', $usageClientKey)
$usageHeaders = @{
    Authorization = 'Bearer ' + $usageCredential.GetNetworkCredential().Password
    'x-ai-usage-instance' = 'writer-1'
}

try {
    $usageModels = Invoke-RestMethod -Uri "$usageBaseUrl/models" -Headers $usageHeaders -ErrorAction Stop
    $usageModels.data | Select-Object id

    $usageBody = @{
        model = 'flash-models'
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

For a streaming client, use the same endpoint with `"stream": true` and consume the SSE stream. Send an optional `x-ai-usage-session` header containing a stable opaque conversation ID (1-200 letters, numbers, underscores or hyphens). `x-opencode-session` is also accepted. Without one, the router creates an ID for that request. For load-distribution pools, an explicit `x-ai-usage-instance` takes priority over this session ID for stickiness. Only Go receives session metadata; instance IDs and arbitrary caller headers are not forwarded. Successful responses include `x-ai-usage-account`, `x-ai-usage-requested-model`, `x-ai-usage-model` and `x-ai-usage-upstream-model` headers so a client can identify the selected account and model. The response body's model is the requested pool name, including in streaming chunks. `x-ai-usage-route-mode` is `failover` or `loadDistribution`; `x-ai-usage-selection` is `failover`, `distributed` or `sticky`. The model catalog also includes a `routing_mode` extension field.

The [HTML routing guide](docs/ROUTING.html) has the complete supported request fields, error contract, limits, selection rules and provider extension interface. GitHub displays HTML as source; download or clone it and open `docs/ROUTING.html` in a browser for the formatted guide.

## Local data

Configuration is stored at `%LOCALAPPDATA%\com.aiusagetray.desktop\settings.json`. Settings contain nonsecret options, not account passwords or copied access tokens. Website sessions are kept in separate WebView2 profiles under the same app directory. The providers handle website authentication directly.

Provider/server keys and the router client key are stored as generic credentials in **Windows Credential Manager**, private to the current Windows account, with target names starting `com.aiusagetray.desktop/`. Keys are sent from the local password field to native code for storage, never saved in settings JSON or returned to the frontend. A blank password field preserves the saved key; entering a replacement updates it. Native cloud usage reads use HTTPS. Local model connections may use HTTP on loopback or a private LAN. Requests reject redirects, honor cancellation, and have time and size limits.

The router forwards your prompt to the selected provider or local server. AI Usage does not log or persist prompts or responses; the selected server or provider has its own data handling. Ordinary settings contain account labels, nonsecret connection options, model pools, each pool's route type and its account/model entries.

Source and installer builds do not include those local profiles or existing Codex credentials. Each recipient connects their own accounts. Distribute the source or generated installer, never a copy of the app's runtime data directory.

Native app commands are restricted to the local main window. Remote sign-in pages have no app-command permissions. Readers run only on their declared HTTPS hostnames and return usage fields. The Codex connection uses the installed app's existing login and requests `account/rateLimits/read` over its local app server.

**Forget** clears this app's saved keys, website session and settings for the selected account. It does not revoke the key at the provider, cancel a subscription or sign out a separate Codex installation. Disabling an account stops its refreshes and routing, and closes its provider windows while preserving its connection for later use. Usage snapshots are held in memory, so a fresh app launch checks enabled accounts again. Use Forget on accounts you want removed before uninstalling; uninstalling the program preserves account data, including the saved router client key.

## Troubleshooting

| What you see | What to check |
| --- | --- |
| No accounts on Usage | Connect an account or turn on **Enable this account** and choose **Save settings**. Then use **Refresh all**. |
| Sign-in required or **Needs attention** | Open that account's **Connect account** window, finish sign-in or the provider's challenge, close it, and refresh. Old readings may remain visible with the error. |
| No reset time or **Percent unavailable** | The source did not provide a usable timestamp or denominator. Local servers have no subscription percentage. For Ollama's split reset layout, use 0.3.1 or later. |
| **Usage monitoring only** / **Routing off** | Check the supported-provider list above. A supported account needs both enable toggles, **Include this account in model pools** and **Save settings**. |
| **No eligible models found** | Save the URL/key before listing, check that the server is running and has a chat model installed, and use its exact model ID. Ollama cloud aliases and non-free OpenRouter models are excluded. |
| Router **Stopped** or connection refused | Start AI Usage, save a client key, enable the router and save. If the port is busy, choose another port and update the calling app's base URL. |
| HTTP 401 | Use the AI Usage client key, not a provider key. Check that a new key was saved. Use the displayed loopback URL and a native/backend client without a browser Origin header. |
| HTTP 400 | Check the model name, JSON body and supported fields. Use Chat Completions rather than Responses/Messages. Provider routing overrides and unsupported paid extensions are rejected. |
| HTTP 404 / model not found | Refresh models in AI Usage, check the exact case-sensitive name, and restart OpenCode to import new names. |
| HTTP 429 | Read the error's `attempts` and `retry_at` fields when present. No configured route may be eligible, an upstream may be rate-limited, or all eight request slots may be busy. Check both account toggles and pool entries, then wait for retry/reset or add an eligible model to the pool. |
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

- `src-tauri/target/release/bundle/nsis/AI Usage_0.7.0_x64-setup.exe`, the installer to distribute.
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

Subscription fixture tests exercise the concrete Go and Ollama Cloud adapters with real local HTTP transport, quota exhaustion, reset recovery, session headers, streaming tool calls, separate keys and plan-only defaults and unknown allowance rejection. See [Verification](docs/VERIFICATION.md) for completed checks and the exact scope of live account verification.

## Add providers and contribute

Provider metadata drives both Settings and Usage. A new provider implements `IUsageProvider` and registers once. Optional inference support goes through `IInferenceProvider`, which supplies model discovery and eligibility checks. See [Adding providers](docs/ADDING_PROVIDERS.md) and the [routing contract](docs/ROUTING.html). Core pool ordering and the HTML pages do not need provider-specific branches.

Follow [Contributing](CONTRIBUTING.md): submit changes through a pull request. Contributions remain outside `main` until the repository owner explicitly chooses to merge them. Automatic merging is disabled. Never commit provider keys, browser profiles, runtime settings or personal account data.

## Source references

- [Tauri tray events](https://v2.tauri.app/learn/system-tray/), [NSIS packaging](https://v2.tauri.app/distribute/windows-installer/) and [capability boundaries](https://v2.tauri.app/security/capabilities/).
- [OpenAI app server documentation](https://developers.openai.com/codex/app-server/) for the Codex account usage request.
- [Claude usage windows](https://code.claude.com/docs/en/statusline), [Suno account page](https://suno.com/account), [Higgsfield credit guidance](https://higgsfield.ai/creator-hub/help-center/credits/how-credits-work) and [Ollama Cloud documentation](https://docs.ollama.com/cloud).
- [OpenRouter account credits](https://openrouter.ai/docs/api/api-reference/credits/get-remaining-credits), [current-key allowances](https://openrouter.ai/docs/api/api-reference/api-keys/get-current-key), and [key reset semantics](https://openrouter.ai/docs/api/api-reference/api-keys/create-keys).
- [OpenCode Go](https://opencode.ai/docs/go/), [Zen](https://opencode.ai/docs/zen/), and the provider's [Go usage endpoint implementation](https://github.com/anomalyco/opencode/blob/dev/packages/console/app/src/routes/zen/go/v1/usage.ts). This endpoint is visible in the provider's public source; it is not a documented compatibility promise.

The website endpoints and DOM readers in this repository are integrations with those sites, not claims of supported public consumer APIs. No provider affiliation is implied.
