# AI Usage development

Read `README.md` and `docs/ADDING_PROVIDERS.md` before extending the app.

- Keep this a small Rust Windows tray utility with local HTML usage and settings pages.
- The tray menu is exactly Show usage, Settings and Exit. Single and double left clicks show Usage. Closing the main window hides it.
- Providers implement `IUsageProvider` and register in `providers::registry()`. Provider metadata drives the frontend. Do not add provider-ID branches to the service or UI.
- Consumer subscription allowances take priority. API credits and consumer subscriptions are different sources; label each reading accurately.
- Configuration DTOs carry nonsecret values only. Website authentication belongs to isolated WebView2 sessions. A future direct-key transport needs a native credential-store boundary.
- Retain stale readings with an error; never fabricate percentages, balances, reset times or successful connections.
- Disabled providers must stop refreshes. Honor cancellation and configuration revisions, including during process and browser work.
- Remote provider pages have no native-command permissions. Readers return only quota fields and run on exact allowed HTTPS hosts.
- Keep the browser preview and its sample readings out of production assets.
- TypeScript interfaces begin with `I`, contain methods only, and use precise DTOs. Do not use `any`.
- Test concrete parsers without constructing the native dynamic registry. The Windows app manifest is not attached to ordinary unit-test executables.
- Run the README's frontend tests, strict production build, Rust tests and Clippy. Build the NSIS bundle with `npm.cmd run installer`.
- Fixture tests are not live account verification. Record which real sign-ins were exercised. Never control the user's desktop while they have taken it back for other work.

The app's account data lives outside the source tree. Never copy browser profiles, Codex authentication files or credentials into source, test fixtures or release archives.
