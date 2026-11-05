# Zevu Desktop

Zevu's desktop process hosts the upstream autocomplete popup and local dashboard.
Autocomplete requires Accessibility permission and `autocomplete.disable = false`;
terminal-specific cursor and integration eligibility remain upstream defaults.
No account, updater, telemetry service, or remote catalogue is used.

The local desktop and PTY sockets, process generators, settings/history APIs, detected
IDE companions, and shell/input-method integrations remain part of the runtime.
Onboarding completes only after the applicable local installation checks pass.
See the [root README](../../README.md) for the macOS build and setup entrypoints.

## Developing

1. Follow the instructions under [the dashboard README](../../packages/dashboard-app/README.md) to run the development server.
1. Run `cargo run`.
1. Once the UI opens, right click anywhere to inspect element, go to the console tab, and set `window.location.href`
to the URL of the dashboard development server.
   - Alternatively, you can use the `DASHBOARD_URL` environment variable instead of manually setting `window.location.href`, e,g. `DASHBOARD_URL=http://localhost:3433 cargo run`.
