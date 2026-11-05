//! Local macOS installation of Zevu and its integrations.
//!
//! `InstallComponents::all()` installs local executable links, available-shell profiles,
//! the bundled input method, and companions for detected IDEs. Existing foreign links
//! are conflicts, never replacement candidates. Uninstall removes only Zevu-owned
//! integrations and app files; settings, history, and profile backups are preserved.
//! No component downloads artifacts, installs SSH hooks, or performs an update.
mod common;
#[cfg(target_os = "macos")]
pub mod macos;
pub use common::{InstallComponents, install, uninstall, install_binary_links};
#[cfg(target_os = "macos")]
pub use macos::uninstall_terminal_integrations;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Util(#[from] fig_util::Error),
    #[error(transparent)]
    Integration(#[from] fig_integrations::Error),
    #[error("installation conflict at {0}")]
    Conflict(std::path::PathBuf),
}
impl From<fig_util::directories::DirectoryError> for Error {
    fn from(err: fig_util::directories::DirectoryError) -> Self { fig_util::Error::Directory(err).into() }
}
