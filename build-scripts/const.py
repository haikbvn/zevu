import pathlib


APP_NAME = "Zevu"
CLI_BINARY_NAME = "zevu"
PTY_BINARY_NAME = "zevu-term"
DESKTOP_BINARY_NAME = "zevu-desktop"
URL_SCHEMA = "zevu"
TAURI_PRODUCT_NAME = "zevu-desktop"

# macos specific
MACOS_BUNDLE_ID = "dev.zevu.autocomplete"

# cargo packages
CLI_PACKAGE_NAME = "q_cli"
PTY_PACKAGE_NAME = "figterm"
DESKTOP_PACKAGE_NAME = "fig_desktop"

DESKTOP_PACKAGE_PATH = pathlib.Path("crates", "fig_desktop")

