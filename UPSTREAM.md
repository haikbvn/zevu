# Upstream provenance

Zevu product version: **0.2.0**.

- Source: https://github.com/aws/amazon-q-developer-cli-autocomplete
- Pinned commit: `b654a1beb880951dd2343120bb867659622eecd9`
- Upstream workspace version: `1.19.7`
- Import: complete clean upstream tree, including upstream Git history; no source, build output, cache, helper, plist or scripts retained from the previous Zevu implementation.
- Upstream authors and `LICENSE.MIT` / `LICENSE.APACHE` are preserved. Internal crate/package identifiers retain upstream provenance.
- Completion catalogue: `@withfig/autocomplete@2.692.3`, gitHead `aef52acff84c45edde61ae610cc2c964802b9a38`.
- Catalogue integrity: `sha512-zxN7K8W+qWGc2ejWksd92xwAyCZ0faHvnk3c7jUoWkkk5e5iQf3/4mOLheFvYLoUWR5a36571VJ1DdyxmAWJZA==`.
- Appearance themes: https://github.com/withfig/themes at `ab71dc82974f4750736d2583867bf6c7219515cb`; all 28 upstream JSON themes retained under `bundle/macos/themes`.
- macOS 13/14 build-host prerequisites use official Homebrew recipes/bottle checksums from `Homebrew/homebrew-core@20e78e4af544a6faf31dded51010e9bf074efe22`. Newer Homebrew core no longer publishes bottles for these hosts. These are build prerequisites, not application runtime dependencies; shell activation and system Rust/Java defaults are not changed.

Target: host-architecture macOS 13+ local autocomplete. Local builds are ad-hoc signed unless a valid Developer ID is explicitly selected; they are not notarized.

## Local runtime corrections

- Bash preexec installation is idempotent: scheduling it twice no longer clears the DEBUG trap and loses command lifecycle hooks.
- Input-method operations execute inline when already on the macOS main thread, avoiding a synchronous dispatch back onto the same queue.
- The dashboard loads bundled license notices from the Vite public resource path, without a network dependency.
- Cursor-only movement is not classified as a paste or history replacement; completion can reopen at a valid middle-of-line token boundary without changing the buffer.
- CSI-u Escape, Enter, Tab, and Backspace decode to their functional keys while retaining modifiers and original bytes; Fish 4's enhanced keyboard protocol can use the existing keybindings.
- Reinstalling an already registered, running, enabled input method leaves existing terminal processes eligible instead of marking them for an unnecessary restart.
- Doctor identifies the terminal application in its own process ancestry, rather than another running instance with the same bundle identifier.
- Fish startup exports the resolved controlling TTY in the same assignment; a bare `set --export TTY` erased the value. TTY discovery uses stderr because the init script is piped into `source`.
- The VS Code companion hides terminal completion when the active editor changes, including opening a blank editor before its first selection change. Native Zsh, Bash, and Fish sessions verify immediate dismissal, editor input, and returning to the original terminal without executing its completion.
- Disabling autocomplete immediately uses the existing native hide path and releases visibility-based interception; re-enabling allows subsequent completion without restarting.
- Figterm releases interception when the desktop socket reader reaches EOF, rather than waiting for the next intercepted key to fail its socket write. Native quit-with-popup verification confirms the first Enter reaches the shell without a Ctrl-C reset.
- macOS AX notification registration checks its status return directly instead of passing a status-only API through an uninitialized output-parameter wrapper. An optimized native reproduction returned `Err(0)` with the old wrapper despite successful registration, and `Ok(())` with status-only handling. Observers are released before their callback data; window-created and focus notifications resolve the application's actual focused window and ignore background applications.
- Cold macOS AX registration uses one bounded 15-second native IPC handshake on a blocking worker instead of the upstream 250 ms activation sleep. A real first request needed 9.15 seconds; short requests returned `kAXErrorCannotComplete`. Ordinary UI queries retain their 250 ms deadline. Observers are reused and removed by process identity, activation immediately disables stale interception, and delayed focus results are checked against the actual foreground process. Native observer/callback owners are released on the main runloop. A source-linked native smoke received the new Ghostty window's focus event without an app-focus cycle.
  Installed-window verification waits for the actual native shell prompt before typing. Input queued during shell startup remains subject to the original paste/history suppression; no replay mechanism or keybinding policy change is added.
- Focused-UI-element notifications use the existing foreground-window focus/hide path. iTerm can switch existing tabs without changing its AX window; a native reproduction showed the old tab's popup above an empty new tab and emitted `AXFocusedUIElementChanged` for the window and text area. Native subscription succeeded in Terminal, iTerm, Ghostty and VS Code. The final installed candidate passed all 12 application/shell focus cases, including existing-tab dismissal and return, external-app focus, new windows, and VS Code blank-editor keyboard isolation. Before/after native screenshots and popup movement without keyboard input are retained with the candidate hashes.

Verification on the Intel macOS host: 291 Rust tests passed (47 suites, 11 ignored), including real isolated Bash, Zsh, and Fish PTYs exercising inherited-empty TTY recovery, Unicode editing, middle-of-line edits, command execution, and resize propagation.
