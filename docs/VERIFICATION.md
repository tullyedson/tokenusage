# Verification record

## Automated checks

Version 0.3.1 reset-time checks on 2026-09-06: 34 Rust tests and 20 frontend tests pass, with strict Clippy and production TypeScript/Vite compilation. Two new website-reader fixtures failed before the fix and pass afterward. They cover reset elements below separate usage headers/bars, monthly dollar balances, independent session/weekly dates, and a missing hourly timestamp that must not borrow the weekly reset. Two native parser tests confirm UTC-offset conversion, serialized reset timestamps, and preservation of valid usage when a reset is missing or invalid. The reader now continues within the same usage window after finding its amount, allowing the existing dashboard to display the reported reset in local time. This change has not been verified against a live Ollama session.

Version 0.3.0 routing checks on 2026-09-06: 32 Rust tests and 18 frontend tests pass, with strict Clippy and production TypeScript/Vite compilation. New tests exercise the actual loopback listener and HTTP transport with fictional upstream servers: ordered account failover on 429, reset recovery with an injected clock, exact-model precedence over substitutions, stopping on unavailable routes, separate account keys, JSON/SSE forwarding, non-replay after terminal errors, key rotation, Host/Origin checks, listener shutdown, real vLLM/Ollama adapter code, cloud-alias rejection, and cancellation while the caller stops reading a stream. Migration preserves version 1 account IDs and browser generations. The headless HTML app test adds a second account, discovers and maps a model, reorders accounts and saves fallback rules through mocked native commands.

These are fixture and headless DOM tests. This routing build has not been installed or exercised against a live model account or the user's running model servers. No real inference requests were sent. OpenAI, Anthropic, OpenCode Go, Ollama Cloud, Suno and Higgsfield remain usage-only in this build; see ROUTING.html for the compatibility limits and no-paid policy.

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

The native app opened successfully, and the dashboard layout was inspected. Desktop control stopped when the user took back the PC. Subsequent work uses background commands only.

## Packaged release

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
- OpenCode Go key authentication and live allowance values for a subscribed workspace. Zen wallet balances are outside the current OpenCode connection.
- Installer UI, uninstall UI and optional Windows startup after an actual install.
- Version 0.3 routing UI in the native window, live local model generation, OpenRouter free-model generation, caller streaming/disconnect behavior, and installed router startup/shutdown. All current routing verification uses fictional local HTTP fixtures and the headless HTML test.

The installer is provided for user-controlled installation. Creating the package does not install it or change Windows startup settings.
