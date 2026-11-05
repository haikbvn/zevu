# Zevu 0.2.0

Local terminal autocomplete for macOS 13+, rebuilt from the complete Amazon Q autocomplete source tree at `b654a1beb880951dd2343120bb867659622eecd9`. Upstream terminal/PTY/parser integrations are retained; the previous Zevu Swift/ZLE implementation is not used.

Source, catalogue, theme and build-prerequisite provenance: [UPSTREAM.md](UPSTREAM.md). Upstream authors and the [MIT](LICENSE.MIT) / [Apache-2.0](LICENSE.APACHE) licenses are retained. The bundled `@withfig/autocomplete` catalogue is ISC-licensed; its license is copied into the application resources.

## Build and install

Prerequisites: macOS, Xcode Command Line Tools, Homebrew and rustup. Setup installs missing dependencies without changing the Rust default, system Java or shell profiles to activate mise.

```sh
bash scripts/setup.sh
```

Setup prepares Rust 1.87.0, Node 22, pnpm 10.0.0, Python 3.11, protobuf, Bash, Fish and Java 17; builds both IDE companions and the four runtime binaries; verifies the signed bundle; then replaces `/Applications/Zevu.app`, installs local integrations and launches the application. The existing installed application is not replaced until the new bundle passes signing verification.

On macOS 13/14, `scripts/setup-brew-bottles.sh` uses the pinned official Homebrew bottle snapshot documented in `UPSTREAM.md`, avoiding a newer Homebrew source-build chain that upgrades unrelated LLVM/Rust dependencies. It does not change the default shell or install other IDE applications.

Build-only:

```sh
mise exec -- bash scripts/build-macos.sh
```

The build uses the host architecture and `MACOSX_DEPLOYMENT_TARGET=13.0`. Local artifacts use ad-hoc signing; they are **not notarized**. Intel builds are not evidence of Apple Silicon verification.

Outputs:

- `dist/Zevu.app`
- `dist/Zevu-macos-<host-arch>.zip`
- `dist/zevu-terminal-companion-0.2.0.vsix`
- `dist/zevu-terminal-companion-0.2.0.zip` — JetBrains plugin

The app contains separate desktop, CLI and PTY executables, the generated `ZevuInputMethod.app`, production frontend assets, the full compiled completion catalogue, its icons/license, and the upstream appearance themes. No Node/pnpm, development server, AWS credentials or chat binary is part of the application runtime. Individual completion generators can require their corresponding installed CLI or network service.

The macOS GitHub Actions job enables Corepack's package-manager shims before building so Turbo can discover the pinned pnpm binary on clean runners.

## Local use

```sh
/Applications/Zevu.app/Contents/MacOS/zevu launch
/Applications/Zevu.app/Contents/MacOS/zevu doctor --all
/Applications/Zevu.app/Contents/MacOS/zevu dashboard
/Applications/Zevu.app/Contents/MacOS/zevu quit
```

Setup automatically detects installed shell and IDE integrations. The dashboard retains local status and repair controls, completion appearance, keybindings and preferences; it has no login or AI setup. macOS may require the user to grant Accessibility to the final installed bundle. Screen Recording/Automation permission for the verification tool is separate from Zevu's completion permission.

Default lifecycle and keybindings remain upstream: Enter accepts the selected completion, Tab inserts the common prefix, and Escape dismisses the popup. When the popup is hidden, the shell handles native editing and command execution. `app.launchOnStartup` retains the upstream default.

Shell integration preserves custom ZDOTDIR, Fish XDG configuration and profile symlinks. Setup removes only the complete legacy Zevu marker block, with a backup; malformed markers and foreign executable links are errors. Uninstall preserves history, settings, backups and other products' integrations. No `q`, `cw` or `cwterm` aliases are installed.

## Catalogue and integrations

The bundled catalogue is `@withfig/autocomplete@2.692.3`: 1,484 completion entries plus its nested/versioned files. Global specs are served only through `spec://localhost/` from bundle resources. Existing development, project and personal spec precedence remains upstream. Static completion and local data generators do not require AWS or the spec CDN; generators such as package search may themselves access the network.

All upstream macOS shell, terminal and IDE variants remain in source, including Terminal.app, iTerm2, Ghostty, WezTerm, VS Code-family applications such as Cursor, and JetBrains IDEs. Availability in source is not a claim that an absent application was live-verified. Bash, Zsh and Fish have isolated real-PTY regression scenarios; Nu support is retained without making it a required installed shell.

The VS Code companion ID is `zevu.zevu-terminal-companion`; its `zevu.cliPath` defaults to the installed bundle CLI. The JetBrains plugin ID is `dev.zevu.jetbrains`. Neither installer removes Fig/Amazon extensions or changes unrelated IDE settings.

## Verification commands

```sh
mise exec -- corepack pnpm install --frozen-lockfile
mise exec -- bash scripts/build-macos.sh
mise exec -- corepack pnpm exec vitest run
cargo build --locked -p q_cli -p figterm
cargo test --locked --workspace -- --test-threads=1
codesign --verify --deep --strict dist/Zevu.app
dist/Zevu.app/Contents/MacOS/zevu --help
```

Tests are not a substitute for exercising the installed application, its popup, shell editing and focus transitions. Runtime results must distinguish exercised applications/displays from unavailable ones.

## Verified host results

The Intel macOS 14.8.9 delivery was exercised in Terminal.app, iTerm2, Ghostty and VS Code with Zsh, Bash and Fish: all 12 combinations passed nine completion/Doctor groups plus native tab/window focus isolation. VS Code also passed editor-focus dismissal and a machine-scoped CLI path containing spaces.

Cold startup used a fresh HOME and private WebKit storage with Wi-Fi powered off and a system-only PATH containing neither Node nor pnpm. Static completion, local branches, escaped directories and Unicode file execution passed. The real npm network generator returned curl exit code 6 without blocking subsequent completion. A separate online static-popup observation found no TCP/UDP sockets in the app and its attributed WebKit network process during eight samples.

Live disable/re-enable, quitting with a selected popup, first-Enter delivery after quit, relaunch, light/dark popup themes, window movement/resize and a built-in display change from 2× to 1× density were exercised. Original theme, disable state and display mode were restored.

Evidence: `dist/verification/release/`. Rust: 291 passed, 11 ignored; the frontend suite run passed 194 with 107 upstream TODO cases. Cursor, WezTerm and JetBrains were not installed for live verification; their source support and real companions are retained. Only one attached display and the Intel build were verified.

Each evidence record retains its candidate's binary hashes. The final installed candidate passed all 12 combinations × nine completion/Doctor groups (`observer-fix/final-live-matrix.json`), all 12 native focus cases (`observer-fix/focus-matrix.json`), live disable/quit/relaunch (`observer-fix/final-lifecycle.json`), and popup movement without keyboard input (`observer-fix/final-native-surface.json`). Its dashboard displays “Zevu is ready”. Earlier cold-offline, theme and display-density evidence retains its original candidate hashes; `dist/verification/release/candidate.json` distinguishes those runs.
