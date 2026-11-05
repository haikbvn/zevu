#!/usr/bin/env bash
# Official Homebrew bottles from the upstream-era snapshot retain macOS 13/14 support.
# Newer Homebrew core no longer publishes bottles for those build hosts.
set -euo pipefail
export HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_INSTALL_UPGRADE=1
export HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK=1 HOMEBREW_NO_INSTALL_CLEANUP=1
SNAPSHOT=20e78e4af544a6faf31dded51010e9bf074efe22
TAP="$(brew --repository)/Library/Taps/zevu/homebrew-build-prereqs"
mkdir -p "$TAP/Formula"
for name in usage abseil ncurses mise protobuf bash fish openjdk@17; do
  if brew list --versions "$name" >/dev/null 2>&1; then continue; fi
  formula="$TAP/Formula/$name.rb"
  curl --fail --location --silent --show-error \
    "https://raw.githubusercontent.com/Homebrew/homebrew-core/$SNAPSHOT/Formula/${name:0:1}/$name.rb" \
    --output "$formula"
  # Preserve the original recipe and checksums; only its bottle registry remains core.
  python3 - "$formula" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
s = s.replace('  no_autobump! because: :bumped_by_upstream\n', '')
needle = '  bottle do\n'
if s.count(needle) != 1:
    raise SystemExit(f'Unexpected pinned Homebrew formula: {p}')
p.write_text(s.replace(needle, needle + '    root_url "https://ghcr.io/v2/homebrew/core"\n', 1))
PY
  brew trust --formula "zevu/build-prereqs/$name"
  # Every selected recipe has checked macOS 13/14 host bottles. Runtime libraries
  # are checked below rather than upgrading unrelated installed dependencies.
  brew install --force-bottle --ignore-dependencies "zevu/build-prereqs/$name"
  if [[ "$name" == mise ]]; then
    # The official bottle enables mise in Fish globally. Zevu setup must not
    # activate mise in user shells; remove only this newly installed vendor hook.
    hook="$(brew --prefix mise)/share/fish/vendor_conf.d/mise-activate.fish"
    if [[ -f "$hook" ]]; then
      link="$(brew --prefix)/share/fish/vendor_conf.d/mise-activate.fish"
      if [[ -L "$link" && "$(python3 -c 'import os,sys;print(os.path.realpath(sys.argv[1]) == os.path.realpath(sys.argv[2]))' "$link" "$hook")" == True ]]; then rm "$link"; fi
      rm "$hook"
    fi
  fi
done
# jq is architecture-independent at the OS level and still has compatible bottles.
brew list --versions jq >/dev/null 2>&1 || brew install jq
export PATH="$(brew --prefix)/bin:$PATH"
mise --version
protoc --version
"$(brew --prefix)/bin/bash" --version
"$(brew --prefix)/bin/fish" --version
"$(brew --prefix openjdk@17)/libexec/openjdk.jdk/Contents/Home/bin/java" -version
