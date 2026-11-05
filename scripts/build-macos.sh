#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"
[[ "$(uname -s)" == Darwin ]] || { echo 'Zevu requires macOS.' >&2; exit 1; }
xcrun --find clang >/dev/null
export PATH="$HOME/.cargo/bin:$PATH:$(brew --prefix)/bin"
export MACOSX_DEPLOYMENT_TARGET=13.0
export AMAZON_Q_BUILD_VARIANT=full
export AMAZON_Q_BUILD_TARGET_TRIPLE="$(rustc -vV | while IFS=' ' read -r key value; do [[ "$key" != host: ]] || printf '%s' "$value"; done)"
export AMAZON_Q_BUILD_HASH=b654a1beb880951dd2343120bb867659622eecd9
export AMAZON_Q_BUILD_DATETIME="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
export JAVA_HOME="$(brew --prefix openjdk@17)/libexec/openjdk.jdk/Contents/Home"
[[ "$(node --version)" == v22.* ]] || { echo 'Run with mise exec -- bash scripts/build-macos.sh (Node 22 required).' >&2; exit 1; }
[[ "$(corepack pnpm --version)" == 10.0.0 ]] || { echo 'pnpm 10.0.0 required.' >&2; exit 1; }
corepack pnpm install --frozen-lockfile
corepack pnpm -C proto build
corepack pnpm build
(
  cd extensions/jetbrains
  ./gradlew buildPlugin
)
# Real Zevu companions must exist before Rust include_bytes! compilation.
cp extensions/vscode/zevu-terminal-companion-0.2.0.vsix crates/fig_integrations/src/vscode/vscode-plugin.vsix
python3 - <<'PY'
from pathlib import Path
import zipfile
archives = sorted(Path('extensions/jetbrains/build/distributions').glob('zevu-terminal-companion-0.2.0.zip'))
if len(archives) != 1:
    raise SystemExit('Expected the newly built Zevu JetBrains plugin ZIP')
with zipfile.ZipFile(archives[0]) as archive:
    jars = [n for n in archive.namelist() if '/lib/' in n and Path(n).name == 'instrumented-zevu-terminal-companion-0.2.0.jar']
    if len(jars) != 1:
        raise SystemExit(f'Expected one instrumented Zevu plugin JAR, found {jars}')
    Path('crates/fig_integrations/src/intellij/instrumented-zevu-terminal-companion-0.2.0.jar').write_bytes(archive.read(jars[0]))
PY
cargo build --release --locked -p q_cli -p figterm -p fig_desktop -p fig_input_method
mkdir -p dist
STAGE="$(mktemp -d "$ROOT/dist/.zevu-build.XXXXXX")"
trap 'rm -rf "$STAGE"' EXIT
export ZEVU_BUNDLE_STAGE="$STAGE"
python3 - <<'PY'
from pathlib import Path
import json, os, shutil
stage = Path(os.environ['ZEVU_BUNDLE_STAGE'])
app = stage/'Zevu.app'
macos = app/'Contents/MacOS'
resources = app/'Contents/Resources'
macos.mkdir(parents=True)
resources.mkdir(parents=True)
for source, dest in [('fig_desktop','zevu-desktop'),('q_cli','zevu'),('figterm','zevu-term')]:
    shutil.copy2(Path('target/release')/source, macos/dest)
shutil.copy2('bundle/macos/Info.plist', app/'Contents/Info.plist')
shutil.copy2('crates/fig_desktop/icons/icon.icns', resources/'Zevu.icns')
for package, dest in [('autocomplete-app','autocomplete'),('dashboard-app','dashboard')]:
    source = Path('packages')/package/'dist'
    if not (source/'index.html').is_file(): raise SystemExit(f'Missing frontend: {source}')
    shutil.copytree(source, resources/dest)
catalogue = Path('node_modules/@withfig/autocomplete').resolve()
metadata = json.loads((catalogue/'package.json').read_text())
if metadata['version'] != '2.692.3': raise SystemExit('Wrong catalogue version')
if not (catalogue/'build/index.json').is_file(): raise SystemExit('Missing catalogue index')
shutil.copytree(catalogue/'build', resources/'specs')
for name in ['icons', 'assets']:
    if (catalogue/name).is_dir(): shutil.copytree(catalogue/name, resources/'specs'/name, dirs_exist_ok=True)
for license_file in catalogue.glob('*LICENSE*'):
    if license_file.is_file(): shutil.copy2(license_file, resources/'specs'/license_file.name)
for name in ['LICENSE.MIT','LICENSE.APACHE','UPSTREAM.md']:
    shutil.copy2(name, resources/name)
shutil.copytree('bundle/macos/themes', resources/'themes')
ime = app/'Contents/Helpers/ZevuInputMethod.app/Contents'
(ime/'MacOS').mkdir(parents=True)
shutil.copy2('target/release/fig_input_method', ime/'MacOS/fig_input_method')
shutil.copy2('crates/fig_input_method/Info.plist', ime/'Info.plist')
shutil.copytree('crates/fig_input_method/resources', ime/'Resources')
companions = resources/'companions'
companions.mkdir()
for source in [Path('extensions/vscode/zevu-terminal-companion-0.2.0.vsix'), Path('extensions/jetbrains/build/distributions/zevu-terminal-companion-0.2.0.zip')]:
    shutil.copy2(source, companions/source.name)
    shutil.copy2(source, stage/source.name)
PY
APP="$STAGE/Zevu.app"
codesign --force --sign - "$APP/Contents/Helpers/ZevuInputMethod.app/Contents/MacOS/fig_input_method"
codesign --force --sign - "$APP/Contents/Helpers/ZevuInputMethod.app"
for executable in zevu zevu-term zevu-desktop; do
  test -x "$APP/Contents/MacOS/$executable"
  file "$APP/Contents/MacOS/$executable"
  codesign --force --sign - --entitlements crates/fig_desktop/entitlements.plist "$APP/Contents/MacOS/$executable"
done
codesign --force --sign - --entitlements crates/fig_desktop/entitlements.plist "$APP"
codesign --verify --deep --strict "$APP"
"$APP/Contents/MacOS/zevu" --version
ARCH="${AMAZON_Q_BUILD_TARGET_TRIPLE%%-*}"
ditto -c -k --sequesterRsrc --keepParent "$APP" "$STAGE/Zevu-macos-$ARCH.zip"
# Publish only a completely packaged and verified bundle. Keep the previous output on failure.
python3 - <<'PY'
from pathlib import Path
import os, shutil
stage = Path(os.environ['ZEVU_BUNDLE_STAGE'])
dist = Path('dist')
old = stage/'previous.app'
if (dist/'Zevu.app').exists(): os.rename(dist/'Zevu.app', old)
try:
    os.rename(stage/'Zevu.app', dist/'Zevu.app')
except BaseException:
    if old.exists(): os.rename(old, dist/'Zevu.app')
    raise
for source in stage.iterdir():
    if source.is_file(): os.replace(source, dist/source.name)
PY
printf 'Verified local build: %s/dist/Zevu.app (%s, macOS 13+, ad-hoc signed; not notarized)\n' "$ROOT" "$ARCH"
