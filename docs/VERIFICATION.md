# Verification record

## Automated checks

- Nine Rust tests pass for quota parsing, multiple OpenAI buckets, missing denominators, credit top-ups, configuration replacement and cancellation.
- Twelve frontend tests pass. JavaScript fixtures exercise the actual website readers, deferred session initialization, usage-only return values, multiple Claude organizations, and Ollama monthly and legacy usage formats.
- The optional Codex integration test passed using the installed account on 2026-09-06. It performed a real usage read through the app server and validated the parsed remaining percentages. No quota values or credentials were printed.
- Strict TypeScript compilation, production Vite build and Rust Clippy with warnings denied.
- Production assets exclude the development-only sample accounts.

The Windows unit-test startup failure encountered during development was caused by constructing the entire dynamic provider registry in a parser integration test. This pulled UI imports into a test executable without the main app's Common Controls manifest. The test now calls the concrete parser, and normal tests pass without those imports. This was separate from the running tray app.

## Desktop observations

The native app opened successfully, and the dashboard layout was inspected. Desktop control stopped when the user took back the PC. Subsequent work uses background commands only.

## Packaged release

The final Windows x64 NSIS build completed successfully on 2026-09-06. The application contains its Common Controls version 6 manifest. Native-command permissions are limited to the local main window, and the production frontend contains no sample readings. The installer is unsigned. The source archive and SHA-256 hashes accompany it in `artifacts/`.

The temporary development instance and its frontend server were closed after verification. No provider settings had been saved in that instance. The installer was not executed.

## Hands-on checks still required

These require the user's account sign-ins or an uninterrupted desktop session:

- Single click, double click and all three tray menu actions.
- Settings category/provider folds, connect and Forget on a real provider session.
- Claude, Higgsfield, Suno and Ollama sign-in, live balances, session persistence and expired-session recovery.
- OpenAI website sign-in, if that connection is used instead of installed Codex.
- Installer UI, uninstall UI and optional Windows startup after an actual install.

The installer is provided for user-controlled installation. Creating the package does not install it or change Windows startup settings.
