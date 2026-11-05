use std::{path::{Path, PathBuf}, sync::Arc};
use fig_integrations::{Integration, shell::ShellExt};
use fig_os_shim::{Context, Env};
use fig_util::{CLI_BINARY_NAME, PTY_BINARY_NAME, Shell, directories};
use crate::Error;

bitflags::bitflags! {
    /// Local components only. User data is never an uninstall component.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct InstallComponents: u64 {
        const SHELL_INTEGRATIONS = 1;
        const BINARY = 2;
        const DESKTOP_APP = 8;
        const INPUT_METHOD = 16;
        const IDE_EXTENSIONS = 32;
    }
}

fn binary_target(name: &str) -> PathBuf {
    fig_util::app_bundle_path().join("Contents/MacOS").join(name)
}

fn owns_link(path: &Path, target: &Path) -> Result<bool, Error> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let existing = std::fs::read_link(path)?;
            let suffix = Path::new(fig_util::APP_BUNDLE_NAME).join("Contents/MacOS").join(target.file_name().unwrap());
            Ok(existing == target || (existing.is_absolute() && existing.ends_with(suffix)))
        },
        Ok(_) => Ok(false),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err.into()),
    }
}

/// Install only Zevu links, rejecting foreign files and links before changing anything.
pub fn install_binary_links() -> Result<(), Error> {
    let bin = directories::home_local_bin()?;
    let pairs = binary_links(&bin);
    install_links(&bin, &pairs)
}

fn binary_links(bin: &Path) -> Vec<(PathBuf, PathBuf)> {
    let mut pairs = vec![
        (bin.join(CLI_BINARY_NAME), binary_target(CLI_BINARY_NAME)),
        (bin.join(PTY_BINARY_NAME), binary_target(PTY_BINARY_NAME)),
    ];
    pairs.extend(Shell::all().iter().map(|shell| (
        bin.join(format!("{shell} ({PTY_BINARY_NAME})")),
        binary_target(PTY_BINARY_NAME),
    )));
    pairs
}

fn install_links(bin: &Path, pairs: &[(PathBuf, PathBuf)]) -> Result<(), Error> {
    for (path, target) in pairs {
        if !target.is_file() {
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("missing bundled executable: {}", target.display())).into());
        }
        if std::fs::symlink_metadata(path).is_ok() && !owns_link(path, target)? {
            return Err(Error::Conflict(path.clone()));
        }
    }
    std::fs::create_dir_all(bin)?;
    for (path, target) in pairs {
        if std::fs::read_link(path).ok().as_deref() != Some(target.as_path()) {
            if owns_link(path, target)? { std::fs::remove_file(path)?; }
            std::os::unix::fs::symlink(target, path)?;
        }
    }
    Ok(())
}

pub async fn install(components: InstallComponents, env: &Env) -> Result<(), Error> {
    if components.contains(InstallComponents::BINARY) { install_binary_links()?; }
    if components.contains(InstallComponents::SHELL_INTEGRATIONS) {
        for shell in Shell::all() {
            if fig_integrations::shell::shell_executable(*shell, env).is_none() {
                tracing::info!(%shell, "Skipping shell: executable not installed");
                continue;
            }
            for integration in shell.get_shell_integrations(env)? { integration.install().await?; }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if components.contains(InstallComponents::INPUT_METHOD) {
            fig_integrations::input_method::InputMethod::default().install().await?;
        }
        if components.contains(InstallComponents::IDE_EXTENSIONS) {
            for variant in fig_integrations::vscode::variants_installed() {
                fig_integrations::vscode::VSCodeIntegration { variant }.install().await?;
            }
            for integration in fig_integrations::intellij::variants_installed().await? { integration.install().await?; }
        }
    }
    Ok(())
}

pub async fn uninstall(components: InstallComponents, ctx: Arc<Context>) -> Result<(), Error> {
    if components.contains(InstallComponents::SHELL_INTEGRATIONS) {
        for shell in Shell::all() {
            for integration in shell.get_shell_integrations(ctx.env())? { integration.uninstall().await?; }
        }
    }
    if components.contains(InstallComponents::BINARY) {
        let bin = directories::home_local_bin()?;
        for (path, target) in binary_links(&bin) {
            if owns_link(&path, &target)? { std::fs::remove_file(path)?; }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if components.contains(InstallComponents::IDE_EXTENSIONS) { crate::macos::uninstall_terminal_integrations().await?; }
        if components.contains(InstallComponents::INPUT_METHOD) { fig_integrations::input_method::InputMethod::default().uninstall().await?; }
        if components.contains(InstallComponents::DESKTOP_APP) { crate::macos::uninstall_desktop(&ctx).await?; }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn link_ownership_preserves_foreign_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("zevu");
        let target = dir.path().join("Zevu.app/Contents/MacOS/zevu");
        std::fs::write(&path, "user file").unwrap();
        assert!(!owns_link(&path, &target).unwrap());
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "binary").unwrap();
        assert!(matches!(install_links(dir.path(), &[(path.clone(), target.clone())]), Err(Error::Conflict(_))));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "user file");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "user file");
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink("/foreign/zevu", &path).unwrap();
        assert!(!owns_link(&path, &target).unwrap());
        assert!(matches!(install_links(dir.path(), &[(path.clone(), target.clone())]), Err(Error::Conflict(_))));
        assert_eq!(std::fs::read_link(&path).unwrap(), Path::new("/foreign/zevu"));
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(owns_link(&path, &target).unwrap());
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "binary").unwrap();
        install_links(dir.path(), &[(path.clone(), target.clone())]).unwrap();
        install_links(dir.path(), &[(path.clone(), target.clone())]).unwrap();
        assert_eq!(std::fs::read_link(&path).unwrap(), target);
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink("/old/Zevu.app/Contents/MacOS/zevu", &path).unwrap();
        install_links(dir.path(), &[(path.clone(), target.clone())]).unwrap();
        assert_eq!(std::fs::read_link(&path).unwrap(), target);
    }
}
