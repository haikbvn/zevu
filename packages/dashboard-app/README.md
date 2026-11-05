# Dashboard

This folder holds the React single-page app for the Zevu desktop UI. Its routes are getting started, autocomplete, keybindings, integrations, preferences, and licenses.

Setup reuses the local shell and Accessibility checks and requires the input method only for terminals that need it. Ready status requires successful current checks; the desktop independently validates prerequisites before persisting onboarding completion. Accounts, promotional feeds, telemetry, and update controls are not part of this app.

## Developing

1. Run `mise exec -- corepack pnpm build` in the project root.
2. Run `mise exec -- corepack pnpm dev:dashboard` for frontend development.
3. Use the local desktop API bridge when developing. The packaged dashboard is loaded from the app resources and does not require a development server.
