use std::env::temp_dir;
use std::io::BufRead;
use std::path::{
    Path,
    PathBuf,
};

use async_trait::async_trait;
use fig_util::macos::BUNDLE_CONTENTS_RESOURCE_PATH;
use macos_utils::url::path_for_application;
use tokio::process::Command;

use crate::error::Result;
use crate::{
    Error,
    Integration,
};

const EXTENSION_PREFIX: &str = "zevu.zevu-terminal-companion";

const EXTENSION_VERSION: &str = "0.2.0";
const EXTENSION: &[u8] = include_bytes!("vscode-plugin.vsix");

#[derive(Clone)]
pub struct VSCodeVariant {
    bundle_identifier: &'static str,
    config_folder_name: &'static str,
    pub application_name: &'static str,
    cli_executable_name: &'static str,
}

pub static VARIANTS: &[VSCodeVariant] = &[
    VSCodeVariant {
        bundle_identifier: "com.microsoft.VSCode",
        config_folder_name: ".vscode",
        application_name: "VSCode",
        cli_executable_name: "code",
    },
    VSCodeVariant {
        bundle_identifier: "com.microsoft.VSCodeInsiders",
        config_folder_name: ".vscode-insiders",
        application_name: "VSCode Insiders",
        cli_executable_name: "code",
    },
    VSCodeVariant {
        bundle_identifier: "com.vscodium",
        config_folder_name: ".vscode-oss",
        application_name: "VSCodium",
        cli_executable_name: "codium",
    },
    VSCodeVariant {
        bundle_identifier: "com.visualstudio.code.oss",
        config_folder_name: ".vscode-oss",
        application_name: "VSCodium",
        cli_executable_name: "codium",
    },
    VSCodeVariant {
        bundle_identifier: "com.todesktop.230313mzl4w4u92",
        config_folder_name: ".cursor",
        application_name: "Cursor",
        cli_executable_name: "cursor",
    },
    VSCodeVariant {
        bundle_identifier: "com.todesktop.230313mzl4w4u92",
        config_folder_name: ".cursor-nightly",
        application_name: "Cursor Nightly",
        cli_executable_name: "cursor-nightly",
    },
    VSCodeVariant {
        bundle_identifier: "com.exafunction.windsurf",
        config_folder_name: ".windsurf",
        application_name: "Windsurf",
        cli_executable_name: "windsurf",
    },
    VSCodeVariant {
        bundle_identifier: "com.exafunction.windsurf-next",
        config_folder_name: ".windsurf-next",
        application_name: "Windsurf Next",
        cli_executable_name: "windsurf-next",
    },
    VSCodeVariant {
        bundle_identifier: "co.posit.positron",
        config_folder_name: ".positron",
        application_name: "Positron",
        cli_executable_name: "positron",
    },
    VSCodeVariant {
        bundle_identifier: "com.trae.app",
        config_folder_name: ".trae",
        application_name: "Trae",
        cli_executable_name: "trae",
    },
];

pub fn variants_installed() -> Vec<VSCodeVariant> {
    VARIANTS
        .iter()
        .filter(|variant| path_for_application(variant.bundle_identifier).is_some())
        .cloned()
        .collect()
}

/// Determines if the VSCode extension is installed by checking for the Zevu extension name in
/// the stdout of `code --list-extensions --show-versions`.
fn is_installed_from_stdout(stdout: impl AsRef<[u8]>) -> bool {
    let extension_name = format!("{EXTENSION_PREFIX}@{EXTENSION_VERSION}");
    stdout
        .as_ref()
        .lines()
        .any(|line| if let Ok(l) = line { l == extension_name } else { false })
}

pub struct VSCodeIntegration {
    pub variant: VSCodeVariant,
}

impl VSCodeIntegration {
    fn extensions_dir(&self) -> Result<PathBuf> {
        Ok(fig_util::directories::home_dir()?
            .join(self.variant.config_folder_name)
            .join("extensions"))
    }

    fn cli_path(&self) -> Result<PathBuf> {
        let bundle_path = path_for_application(self.variant.bundle_identifier)
            .ok_or_else(|| Error::ApplicationNotInstalled(self.variant.application_name.into()))?;

        Ok(bundle_path
            .join(BUNDLE_CONTENTS_RESOURCE_PATH)
            .join("app/bin")
            .join(self.variant.cli_executable_name))
    }

    async fn remove_extension(extensions_dir: &Path) -> Result<()> {
        let mut entries = match tokio::fs::read_dir(extensions_dir).await {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let Some(version) = name
                .to_str()
                .and_then(|name| name.strip_prefix(EXTENSION_PREFIX))
                .and_then(|suffix| suffix.strip_prefix('-'))
            else {
                continue;
            };
            // VS Code stores extensions as publisher.name-version[-platform]. Require a
            // complete release version so similarly named foreign extensions stay untouched.
            let mut components = version.split(['-', '+']).next().unwrap_or_default().split('.');
            if !(0..3).all(|_| {
                components.next().is_some_and(|component| {
                    !component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit())
                })
            }) || components.next().is_some()
            {
                continue;
            }
            let file_type = entry.file_type().await?;
            if file_type.is_symlink() {
                tokio::fs::remove_file(entry.path()).await?;
            } else if file_type.is_dir() {
                tokio::fs::remove_dir_all(entry.path()).await?;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl Integration for VSCodeIntegration {
    fn describe(&self) -> String {
        format!("{} Integration", self.variant.application_name)
    }

    async fn install(&self) -> Result<()> {
        if self.is_installed().await.is_ok() {
            return Ok(());
        }

        let extension_path = temp_dir().join("zevu-terminal-companion-0.2.0.vsix");
        tokio::fs::write(&extension_path, &EXTENSION).await?;

        let output = Command::new(self.cli_path()?)
            .arg("--install-extension")
            .arg(extension_path.as_os_str())
            .output()
            .await?;

        if !output.status.success() {
            return Err(Error::Custom(
                format!(
                    "error installing extension. stdout: {:?}",
                    String::from_utf8_lossy(&output.stdout)
                )
                .into(),
            ));
        }

        Ok(())
    }

    async fn uninstall(&self) -> Result<()> {
        Self::remove_extension(&self.extensions_dir()?).await
    }

    async fn is_installed(&self) -> Result<()> {
        let output = Command::new(self.cli_path()?)
            .arg("--list-extensions")
            .arg("--show-versions")
            .output()
            .await?;
        if !is_installed_from_stdout(output.stdout) {
            return Err(Error::Custom("Extension not installed".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_is_installed_from_stdout() {
        assert!(is_installed_from_stdout(indoc::formatdoc! {"
            [0423/155310.304438:ERROR:codesign_util.cc(108)] SecCodeCheckValidity: Error Domain=NSOSStatusErrorDomain Code=-67062 \"(null)\" (-67062)
            zevu.zevu-terminal-companion@{EXTENSION_VERSION}
            eamodio.gitlens@14.9.0
            mathiasfrohlich.kotlin@1.7.1
        "}));

        assert!(!is_installed_from_stdout(indoc::indoc! {r#"
            [0423/155310.304438:ERROR:codesign_util.cc(108)] SecCodeCheckValidity: Error Domain=NSOSStatusErrorDomain Code=-67062 "(null)" (-67062)
            eamodio.gitlens@14.9.0
            mathiasfrohlich.kotlin@1.7.1
        "#}));
        assert!(!is_installed_from_stdout(
            b"withfig.fig@1.0.0\namazonwebservices.codewhisperer-for-command-line-companion@0.2.0\n"
        ));
        assert!(!is_installed_from_stdout(b"zevu.zevu-terminal-companion@0.1.0\n"));
        assert!(!is_installed_from_stdout(b"zevu.zevu-terminal-companion-other@0.2.0\n"));
    }

    #[tokio::test]
    async fn uninstall_removes_only_zevu_extensions() {
        let home = tempfile::tempdir().unwrap();
        let extensions_dir = home.path().join(".vscode/extensions");
        let owned = [
            "zevu.zevu-terminal-companion-0.1.0",
            "zevu.zevu-terminal-companion-0.2.0",
            "zevu.zevu-terminal-companion-0.2.0-darwin-x64",
        ];
        let foreign = [
            "withfig.fig-1.0.0",
            "amazonwebservices.codewhisperer-for-command-line-companion-1.0.0",
            "amazonwebservices.amazon-q-vscode-1.0.0",
            "eamodio.gitlens-14.9.0",
            "zevu.zevu-terminal-companion-other-0.2.0",
            "zevu.zevu-terminal-companion2-0.2.0",
            "other.zevu-terminal-companion-0.2.0",
        ];
        for name in owned.iter().chain(foreign.iter()) {
            let extension_dir = extensions_dir.join(name);
            tokio::fs::create_dir_all(&extension_dir).await.unwrap();
            tokio::fs::write(extension_dir.join("user-data"), name).await.unwrap();
        }
        let extension_index = extensions_dir.join("extensions.json");
        tokio::fs::write(&extension_index, b"unrelated extension registry").await.unwrap();
        let settings_path = home.path().join("Library/Application Support/Code/User/settings.json");
        tokio::fs::create_dir_all(settings_path.parent().unwrap()).await.unwrap();
        let settings = b"// User settings with comments\n{\"editor.accessibilitySupport\": \"on\"}\n";
        tokio::fs::write(&settings_path, settings).await.unwrap();

        // The second uninstall must succeed without touching the remaining extensions.
        for _ in 0..2 {
            VSCodeIntegration::remove_extension(&extensions_dir).await.unwrap();
            for name in owned {
                assert!(!extensions_dir.join(name).exists(), "{name} was not removed");
            }
            for name in foreign {
                assert_eq!(
                    tokio::fs::read(extensions_dir.join(name).join("user-data")).await.unwrap(),
                    name.as_bytes(),
                    "{name} was modified"
                );
            }
            assert_eq!(
                tokio::fs::read(&extension_index).await.unwrap(),
                b"unrelated extension registry"
            );
            assert_eq!(tokio::fs::read(&settings_path).await.unwrap(), settings);
        }
    }

    #[tokio::test]
    async fn uninstall_missing_extensions_directory_is_idempotent() {
        let home = tempfile::tempdir().unwrap();
        let extensions_dir = home.path().join(".cursor/extensions");
        for _ in 0..2 {
            VSCodeIntegration::remove_extension(&extensions_dir).await.unwrap();
        }
        assert!(!extensions_dir.exists());
    }

    #[tokio::test]
    async fn uninstall_zevu_symlink_preserves_foreign_target() {
        let home = tempfile::tempdir().unwrap();
        let extensions_dir = home.path().join(".vscode/extensions");
        let foreign_dir = home.path().join("foreign-extension");
        tokio::fs::create_dir_all(&extensions_dir).await.unwrap();
        tokio::fs::create_dir_all(&foreign_dir).await.unwrap();
        tokio::fs::write(foreign_dir.join("user-data"), b"preserve me").await.unwrap();
        let extension_link = extensions_dir.join("zevu.zevu-terminal-companion-0.2.0");
        std::os::unix::fs::symlink(&foreign_dir, &extension_link).unwrap();

        VSCodeIntegration::remove_extension(&extensions_dir).await.unwrap();

        assert!(!extension_link.exists());
        assert_eq!(
            tokio::fs::read(foreign_dir.join("user-data")).await.unwrap(),
            b"preserve me"
        );
    }
}
