# Verification record

## Version 0.6.0 routing reports

On September 6, 2026, 51 Rust tests and 30 frontend tests passed with strict Clippy and TypeScript/Vite production compilation. The Reports tab shows concurrent active pool requests, selected account/provider/model, lifecycle stages, elapsed times and recent expandable fallback steps. History is capped at 100 finished requests and 64 steps per request; clearing history preserves active requests.

Native HTTP fixtures verify live streaming destination metadata, quota fallback and restoration of the preferred entry after reset, caller disconnect, cancellation of a queued future, successful streams, terminal error streams and truncation. Forwarded SSE bytes remain unchanged apart from existing pool aliasing. Report serialization excludes fictional prompts, completions, keys, session IDs, URLs and raw upstream error bodies. A streaming HTTP 200 is not counted as successful completion by itself.

Frontend tests cover simultaneous pipelines, provider/account/model display, search and result filters, expansion across refreshes, clear-history failures, stale reads after clearing or remounting, metadata escaping and polling cleanup on navigation. The main app integration test exercises the real Reports tab and native command name. The populated page at 980 pixels and empty page at 680 pixels were inspected in isolated headless Edge profiles without controlling the owner's desktop. Reports commands are restricted to the local main window, with no HTTP report endpoint or provider-specific UI branches.

The final 0.6.0 NSIS build passed archive integrity, product-version and full payload comparison, differing from the optimized build only in Tauri's three-byte UNK-to-NSS marker. It was installed silently while the router had no open client connections. The installed executable exactly matches that payload; settings were preserved byte-for-byte, startup preferences were unchanged, and one tray process restarted hidden. A neutral live request to the existing local vLLM Qwen model succeeded with the requested model name, correct account header and new report request ID. The router and OpenCode each expose 46 models after the upgrade; OpenCode configuration and auth hashes remain unchanged. No cloud generation or real quota depletion was induced for this reporting update. Native report controls remain covered by the command/UI tests rather than desktop interaction.

## Version 0.5.1 model pools

On September 6, 2026, 46 Rust tests and 25 frontend tests passed, with strict Clippy, production TypeScript/Vite compilation and NSIS packaging. Pool tests cover exact order across different models/providers, return after reset, automatic catalogs, cache expiry and stale/error handling, disabled accounts, custom overrides, version 1/2 migration, UTF-8 SSE chunk boundaries, tool-call preservation and stable response model names. Existing no-paid-request, local-model verification, credential isolation, cancellation and no-replay regressions pass.

DOM integration tests exercise creation with spaces converted to hyphens, drag-and-drop and arrow ordering, mixed providers, duplicate prevention, draft preservation across navigation/refresh, failed saves, resetting to automatic groups, and connection saves preserving pools. OpenCode plugin tests cover automatic name import, explicit limit preservation, offline behavior, loopback-only authentication and stable session headers. The Models page was rendered in an isolated headless browser at 980 pixels wide, including a populated three-entry pool; its controls and layout were inspected without controlling the user's desktop.

The account owner completed the provider billing setup and enabled Go and Ollama Cloud before this update. Version 0.5.0 defaults to plan-only routing without a confirmation dropdown. The app cannot inspect or change the provider-side overage switches, and supported Ollama billing remains the legacy capped plan without extra credits. Live quota exhaustion/reset recovery is simulated in fixtures, not by consuming real allowances.

Version 0.5.1 explicitly disables Tauri's native file-drop interception on the main window, which is required for HTML5 drag-and-drop on Windows. The pool integration test guards this host setting as well as the DOM behavior. All checks and NSIS packaging above were rerun for 0.5.1. Native pointer interactions were not exercised while the owner used the desktop.

After the owner's restart approval, the 0.5.0 and final 0.5.1 installers were applied silently and the app restarted hidden. Both upgrades preserved settings byte-for-byte and the Windows startup preference. The running 0.5.1 executable matches the extracted installer payload exactly, with one tray process. Its authenticated router catalog and Harborlight's OpenCode 1.18.29 each expose 46 models, including GLM and the existing local Qwen route. The OpenCode plugin matches the distributed example, and its project configuration and authentication file remain unchanged.

Neutral live requests through the same routing engine in 0.5.0 succeeded for Go GLM and Ollama Cloud DeepSeek. A local Qwen streaming request returned a valid tool call across 27 events and a terminal marker, without executing a tool. These requests preserved the requested model name. Real account depletion and reset were not induced; those transitions remain fixture-tested. The final Windows-only patch was followed by fresh catalog/client checks without repeating cloud generation. Existing settings version 2 is migrated in memory; the next successful settings save persists version 3.

## Automated checks

Version 0.4.0 subscription-routing checks on 2026-09-06: 41 Rust tests and 21 frontend tests pass, including strict Clippy and production TypeScript/Vite compilation. Five new native tests exercise all three Go quota windows, failover to the same GLM model on Ollama Cloud, restoration of Go priority after a fresh post-reset read, quota races at submission, both accounts exhausted, separate account keys, stream/tool-call preservation and no generation for unknown usage, missing models or unconfirmed billing. A client-plugin test verifies stable session IDs and preservation of other providers' headers. These fixtures use the actual subscription adapters with local HTTP servers; they do not exhaust real accounts.

Read-only live checks confirmed that the existing Go and Ollama keys authenticate, both catalogs include `glm-5.3-flash`, Go returns all three current usage windows, and Ollama exposes its OpenAI-compatible model endpoint. These checks did not send a generation request, verify provider billing switches, or prove live failover. At that build, routing was left disabled pending owner setup. The owner subsequently completed it before the 0.5.0 work recorded above.

Version 0.3.2 Spark-meter checks on 2026-09-06: 36 Rust tests and 20 frontend tests pass, with strict Clippy and production TypeScript/Vite compilation. Two new parser regressions reproduced three meters before the change and pass with only the normal weekly allowance afterward. Both signed-in Codex and website payloads are covered, including identification by the provider's Spark bucket ID or display name, with the regular percentage, reset and plan preserved. The optional signed-in Codex integration test also passes against the installed account without printing account data. The OpenAI website path remains fixture-tested.

Version 0.3.1 reset-time checks on 2026-09-06: 34 Rust tests and 20 frontend tests pass, with strict Clippy and production TypeScript/Vite compilation. Two new website-reader fixtures failed before the fix and pass afterward. They cover reset elements below separate usage headers/bars, monthly dollar balances, independent session/weekly dates, and a missing hourly timestamp that must not borrow the weekly reset. Two native parser tests confirm UTC-offset conversion, serialized reset timestamps, and preservation of valid usage when a reset is missing or invalid. The reader now continues within the same usage window after finding its amount, allowing the existing dashboard to display the reported reset in local time. This change has not been verified against a live Ollama session.

Version 0.3.0 routing checks on 2026-09-06: 32 Rust tests and 18 frontend tests pass, with strict Clippy and production TypeScript/Vite compilation. New tests exercise the actual loopback listener and HTTP transport with fictional upstream servers: ordered account failover on 429, reset recovery with an injected clock, exact-model precedence over substitutions, stopping on unavailable routes, separate account keys, JSON/SSE forwarding, non-replay after terminal errors, key rotation, Host/Origin checks, listener shutdown, real vLLM/Ollama adapter code, cloud-alias rejection, and cancellation while the caller stops reading a stream. Migration preserves version 1 account IDs and browser generations. The headless HTML app test adds a second account, discovers and maps a model, reorders accounts and saves fallback rules through mocked native commands.

The 0.3.0 checks above were fixture and headless DOM tests, with no real inference requests during that build. OpenAI, Anthropic, OpenCode Go, Ollama Cloud, Suno and Higgsfield were usage-only in 0.3.0. The 0.4.0 subscription support and its separate verification scope are recorded at the top of this file.

The following checks describe the preceding 0.2.0 usage-reader release:

- Twenty Rust tests pass for quota parsing, multiple OpenAI buckets, missing denominators, credit top-ups, configuration replacement, secret-field separation, credential rollback/Forget, native HTTP restrictions and cancellation. OpenRouter fixtures cover account credits and resetting/uncapped key allowances; OpenCode fixtures cover all three Go windows and malformed responses.
- Fourteen frontend tests pass. JavaScript fixtures exercise the actual website readers, deferred session initialization, usage-only return values, multiple Claude organizations, and Ollama monthly and legacy usage formats. Password-field tests verify separate secret payloads, blank saved-key inputs and clearing entered keys after saving.
- The isolated Windows Credential Manager test passed on 2026-09-06. It created, read, replaced and deleted a uniquely named fictional credential, then verified its removal. It did not access a provider account. Run it explicitly with `cargo test --manifest-path src-tauri/Cargo.toml windows_vault_round_trip_with_isolated_temporary_key -- --ignored`.
- The optional Codex integration test passed using the installed account on 2026-09-06. It performed a real usage read through the app server and validated the parsed remaining percentages. No quota values or credentials were printed.
- Strict TypeScript compilation, production Vite build and Rust Clippy with warnings denied.
- Production assets exclude the development-only sample accounts.
- Anonymous requests to the OpenRouter credits/current-key endpoints and OpenCode Go usage endpoint returned HTTP 401 on 2026-09-06. This confirms the routes require authentication; it is not a live account-usage test.

The Windows unit-test startup failure encountered during development was caused by constructing the entire dynamic provider registry in a parser integration test. This pulled UI imports into a test executable without the main app's Common Controls manifest. The test now calls the concrete parser, and normal tests pass without those imports. This was separate from the running tray app.

After the project folder moved, cached Tauri permission manifests still referenced the old absolute build path. Rebuilding the Tauri package caches resolved the Clippy build failure. When moving a checkout, regenerate these build outputs if they reference its old directory.

## Desktop observations

The version 0.4.0 NSIS installer was applied in silent update mode with no open router connections. The installed executable matches the verified installer payload, settings were byte-identical across the upgrade, and the Windows startup preference was preserved. The app restarted hidden. A subsequent scoped setup saved GLM mappings for Go and Ollama Cloud, with Go first, while leaving both subscription routes disabled pending billing confirmation. The existing Go key matches the OpenCode Go connection, and the existing Ollama key was saved in Windows Credential Manager. No secret values were printed or written to project configuration.

Harborlight's OpenCode project configuration now lists `ai-usage/glm-5.3-flash` and retains its prior Qwen route. The session plugin is installed. `opencode models ai-usage` successfully lists both choices. OpenCode's existing client key still authenticates to the live router, whose active catalog retains the original local route. Harborlight's tracked source diff and the OpenCode auth file were unchanged by setup. The cloud model is prepared in the client but cannot route until its billing prerequisites are confirmed and its account routing toggles enabled. No cloud inference requests were sent, and no desktop controls were used.

The version 0.3.2 NSIS installer was applied in silent update mode while the router had no open client connections. The installed executable matches the verified installer payload, settings remain byte-identical, and the Windows startup preference is unchanged. The app restarted hidden in the tray. OpenCode's existing client key successfully read the router model catalog and its previous local model route remained available. No desktop interaction or screenshot verification was performed for this update.

The native app opened successfully, and the dashboard layout was inspected. Desktop control stopped when the user took back the PC. Subsequent work uses background commands only.

## Packaged release

The version 0.5.1 Windows x64 NSIS installer passed archive integrity and product-version checks. Full executable comparison found only Tauri's three-byte UNK-to-NSS bundle marker; the installed executable matches the extracted payload exactly. The package contains the application and standard NSIS helper assets. Account data remains outside the source and release archives. The installer is unsigned.

The version 0.4.0 Windows x64 NSIS installer passed archive integrity and product-version checks on 2026-09-06. The seven-file payload contains the application and standard installer helpers. A complete byte comparison matched the optimized executable apart from Tauri's three-byte UNK-to-NSS bundle marker. The installed executable exactly matches that payload.

The version 0.3.2 Windows x64 NSIS installer passed archive integrity and product-version checks on 2026-09-06. The packaged executable differs from the optimized build only in Tauri's three-byte UNK-to-NSS bundle marker; a complete byte comparison verified no other differences. The silent installed executable exactly matches that packaged payload.

The version 0.3.1 Windows x64 NSIS installer was built on 2026-09-06 with the Ollama reset fix. Archive integrity, the current-user installer configuration, product/file version 0.3.1, Common Controls version 6, and production fixture exclusion pass. Its extracted application matches the final build after the same in-memory Tauri bundle-marker normalization described below. The 94-file source audit found no credentials or account data. It has not been installed or launched; the running app and its sessions were left untouched.

The version 0.3.0 Windows x64 feature installer was built on 2026-09-06 after final routing/cancellation checks. It packages one application executable plus standard NSIS helper assets. The extracted executable matches the final build apart from Tauri's documented three-byte bundle marker change from UNK to NSS. A full byte comparison after normalizing that marker found no other differences. It reports product version 0.3.0 and contains the Common Controls version 6 dependency without an elevation request. NSIS is configured for the current user. The staged source audit covers 94 files and reports no credential/account-data findings; the production fixture scan also passes. This build is unsigned and was not installed or launched; an existing tray process was left running. Source and installer archives with SHA-256 hashes are provided in artifacts, while the code remains on the feature branch for owner review.

The version 0.2.0 Windows x64 NSIS build completed successfully on 2026-09-06. The application contains its Common Controls version 6 manifest and both native provider readers. Native-command permissions are limited to the local main window, and the production frontend contains no sample readings. The installer packages the application executable without account data. It is unsigned. The source archive and SHA-256 hashes accompany it in `artifacts/`.

The temporary development instance and its frontend server were closed after verification. No provider settings had been saved in that instance. The installer was not executed.

## Hands-on checks still required

These require the user's account sign-ins or an uninterrupted desktop session:

- Single click, double click and all three tray menu actions.
- Settings category/provider folds, connect and Forget on a real provider session.
- Claude, Higgsfield, Suno and Ollama sign-in, live balances, session persistence and expired-session recovery.
- OpenAI website sign-in, if that connection is used instead of installed Codex.
- OpenRouter management-key account credits and standard-key allowances against a real account, including key replacement and Forget in the installed app.
- OpenCode Go website session persistence and expired-session recovery. Native key authentication, all three allowance windows and included-plan generation were exercised as recorded above. Zen wallet balances are outside the current OpenCode connection.
- Installer UI, uninstall UI and optional Windows startup after an actual install.
- Pool drag/drop in the native window, OpenRouter free-model generation, local Ollama generation and real caller disconnect behavior. Local vLLM generation, streaming tool calls, silent installation, router restart and OpenCode model import passed as recorded above. Quota exhaustion/reset and disconnect/cancellation paths use local HTTP fixtures.

Building an installer alone does not install it or change Windows startup settings. The separate silent upgrades and their verified scope are recorded above; interactive installation and uninstall UI remain user-controlled checks.
