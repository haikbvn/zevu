#!/usr/bin/env bash
# Local macOS dependency setup, build, verified installation and launch.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"
[[ "$(uname -s)" == Darwin ]] || { echo 'Zevu setup supports macOS only.' >&2; exit 1; }
xcrun --find clang >/dev/null || { echo 'Install Xcode Command Line Tools before running setup.' >&2; exit 1; }
command -v brew >/dev/null || { echo 'Homebrew is required: https://brew.sh' >&2; exit 1; }
export HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_INSTALL_UPGRADE=1
export HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK=1 HOMEBREW_NO_INSTALL_CLEANUP=1
MACOS_MAJOR="$(sw_vers -productVersion)"
MACOS_MAJOR="${MACOS_MAJOR%%.*}"
if (( MACOS_MAJOR <= 14 )); then
  bash scripts/setup-brew-bottles.sh
fi
for dependency in mise protobuf bash fish shellcheck jq cmake openjdk@17; do
  brew list --versions "$dependency" >/dev/null 2>&1 || brew install "$dependency"
done
export PATH="$HOME/.cargo/bin:$(brew --prefix)/bin:$PATH"
command -v rustup >/dev/null || { echo 'Install rustup without changing your default shell, then run setup again: https://rustup.rs' >&2; exit 1; }
rustup toolchain install 1.87.0 --profile minimal --component rustfmt --component clippy
mise trust .mise.toml
mise install
export JAVA_HOME="$(brew --prefix openjdk@17)/libexec/openjdk.jdk/Contents/Home"
export MACOSX_DEPLOYMENT_TARGET=13.0
mise exec -- corepack pnpm install --frozen-lockfile
# Dependency-only mode is used to prepare the checkout before companion/core cutover.
[[ "${ZEVU_DEPS_ONLY:-0}" != 1 ]] || exit 0
mise exec -- bash scripts/build-macos.sh
codesign --verify --deep --strict dist/Zevu.app
# Staging and replacement stay on the /Applications volume; never expose a half-copy.
STAGE="$(mktemp -d /Applications/.zevu-install.XXXXXX)"
trap 'rm -rf "$STAGE"' EXIT
ditto dist/Zevu.app "$STAGE/Zevu.app"
codesign --verify --deep --strict "$STAGE/Zevu.app"
if [[ -d /Applications/Zevu.app ]]; then
  if [[ -x /Applications/Zevu.app/Contents/MacOS/zevu ]]; then
    /Applications/Zevu.app/Contents/MacOS/zevu quit || true
  fi
  mv /Applications/Zevu.app "$STAGE/previous.app"
fi
if ! mv "$STAGE/Zevu.app" /Applications/Zevu.app; then
  [[ ! -d "$STAGE/previous.app" ]] || mv "$STAGE/previous.app" /Applications/Zevu.app
  exit 1
fi
/Applications/Zevu.app/Contents/MacOS/zevu setup --no-confirm
/Applications/Zevu.app/Contents/MacOS/zevu launch
printf 'Installed and launched /Applications/Zevu.app\n'
