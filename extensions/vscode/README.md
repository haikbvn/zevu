# Zevu Terminal Companion

The IDE companion for the Zevu autocomplete desktop app. It keeps terminal focus
associated with the correct Zevu shell session; it does not provide editor
completions or require an AWS account.

Extension ID: `zevu.zevu-terminal-companion`  
Version: `0.2.0`

## Installation

Zevu setup detects supported installed VS Code-family IDEs and installs the
companion using their bundled CLI. The upstream variants remain supported:
VS Code, VS Code Insiders, VSCodium / Code OSS, Cursor, Cursor Nightly, Windsurf,
Windsurf Next, Positron, and Trae. Uninstalling the companion removes only Zevu's
extension directories, not Fig, Amazon Q, or other extensions.

To package the extension from this directory, run `pnpm run build`. The resulting
`zevu-terminal-companion-0.2.0.vsix` can also be installed using the IDE's
**Extensions: Install from VSIX** command. Zevu's macOS build copies that real
VSIX to `crates/fig_integrations/src/vscode/vscode-plugin.vsix` before compiling
the installer.

## Configuration and lifecycle

`zevu.cliPath` is the absolute path to the CLI. It defaults to
`/Applications/Zevu.app/Contents/MacOS/zevu`; change it in the IDE's user settings
if Zevu is installed elsewhere. The companion executes the CLI directly, without
a shell or a dependency on the GUI process's `PATH`.

New integrated terminals receive `Q_NEW_SESSION=1` through VS Code's ephemeral
environment variable collection. Open a new terminal after enabling the
extension. The collection is not persisted across reloads and is cleared when
the companion is disposed, together with its terminal/editor listeners. The
companion does not rewrite `settings.json`, `terminal.integrated.env.osx`, or
`editor.accessibilitySupport`.

For developer logging, set the internal user setting `"zevu.log": true`.