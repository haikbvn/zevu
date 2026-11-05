use std::collections::HashMap;
use std::path::{
    Path,
    PathBuf,
};

use async_trait::async_trait;
use fig_util::macos::BUNDLE_CONTENTS_INFO_PLIST_PATH;
use fig_util::terminal::IntelliJVariant;
use macos_utils::url::path_for_application;
use serde::Deserialize;

use crate::error::Result;
use crate::{
    Error,
    Integration,
};

const PLUGIN_PREFIX: &str = "zevu-terminal-companion";

/// Version of the Zevu IntelliJ plugin. This should match the version in
/// [build.gradle](../../../../extensions/jetbrains/build.gradle).
const PLUGIN_VERSION: &str = "0.2.0";

static PLUGIN_CONTENTS: &[u8] = include_bytes!("instrumented-zevu-terminal-companion-0.2.0.jar");

/// Returns the directory name of the Zevu plugin under IntelliJ's `plugins/` directory.
fn plugin_slug() -> String {
    format!("{PLUGIN_PREFIX}-{PLUGIN_VERSION}")
}

fn is_zevu_plugin(name: &str) -> bool {
    let Some(version) = name.strip_prefix(PLUGIN_PREFIX).and_then(|suffix| suffix.strip_prefix('-')) else {
        return false;
    };
    let mut components = version.split('.');
    (0..3).all(|_| {
        components
            .next()
            .is_some_and(|component| !component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit()))
    }) && components.next().is_none()
}

async fn uninstall_plugins(plugins_folder: &Path) -> Result<()> {
    let mut entries = match tokio::fs::read_dir(plugins_folder).await {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(Error::Custom(
                format!("Failed reading plugins folder dir {plugins_folder:?}: {err:?}").into(),
            ));
        },
    };
    while let Some(entry) = entries.next_entry().await.map_err(|err| {
        Error::Custom(format!("Failed reading next entry in plugins folder dir {plugins_folder:?}: {err:?}").into())
    })? {
        if entry.file_name().to_str().is_some_and(is_zevu_plugin) {
            tokio::fs::remove_dir_all(entry.path()).await.map_err(|err| {
                Error::Custom(
                    format!(
                        "Failed removing entry {:?} from plugins folder dir {plugins_folder:?}: {err:?}",
                        entry.path()
                    )
                    .into(),
                )
            })?;
        }
    }
    Ok(())
}

async fn install_plugin(application_folder: &Path) -> Result<()> {
    let plugins_folder = application_folder.join("plugins");
    let slug = plugin_slug();
    if plugins_folder.join(&slug).exists() {
        return Ok(());
    }

    uninstall_plugins(&plugins_folder).await?;
    let lib_dir = plugins_folder.join(&slug).join("lib");
    tokio::fs::create_dir_all(&lib_dir)
        .await
        .map_err(|err| Error::Custom(format!("Failed creating plugin lib folder: {err:?}").into()))?;

    let jar_path = lib_dir.join(format!("{slug}.jar"));
    tokio::fs::write(&jar_path, PLUGIN_CONTENTS)
        .await
        .map_err(|err| Error::Custom(format!("Failed writing plugin jar to {jar_path:?}: {err:?}").into()))?;
    Ok(())
}

pub async fn variants_installed() -> Result<Vec<IntelliJIntegration>> {
    Ok(IntelliJVariant::all()
        .iter()
        .filter(|variant| path_for_application(variant.bundle_identifier()).is_some())
        .cloned()
        .map(|variant| IntelliJIntegration { variant })
        .collect())
}

pub struct IntelliJIntegration {
    pub variant: IntelliJVariant,
}

#[derive(Deserialize)]
struct InfoPList {
    #[serde(rename = "JVMOptions")]
    jvm_options: JVMOptions,
}

#[derive(Deserialize)]
struct JVMOptions {
    #[serde(rename = "Properties")]
    properties: HashMap<String, String>,
}

impl IntelliJIntegration {
    fn get_jvm_properties(&self) -> Result<HashMap<String, String>> {
        let plist_path = path_for_application(self.variant.bundle_identifier())
            .ok_or_else(|| Error::ApplicationNotInstalled(self.variant.application_name().into()))?
            .join(BUNDLE_CONTENTS_INFO_PLIST_PATH);

        let contents: InfoPList = plist::from_file(plist_path)
            .map_err(|err| Error::Custom(format!("Could not read plist file: {err:?}").into()))?;

        Ok(contents.jvm_options.properties)
    }

    pub fn application_folder(&self) -> Result<PathBuf> {
        let mut props = self.get_jvm_properties().map_err(|err| {
            Error::Custom(
                format!(
                    "Couldn't get JVM properties for {}: {err:?}",
                    self.variant.application_name()
                )
                .into(),
            )
        })?;

        let selector = props.remove("idea.paths.selector").ok_or_else(|| {
            Error::Custom(
                format!(
                    "Could not read `idea.paths.selector` from jvm options for {}",
                    self.variant.application_name()
                )
                .into(),
            )
        })?;

        Ok(dirs::data_local_dir()
            .ok_or_else(|| Error::Custom("Could not read application support directory".into()))?
            .join(self.variant.organization())
            .join(selector))
    }
}

#[async_trait]
impl Integration for IntelliJIntegration {
    fn describe(&self) -> String {
        format!("{} Integration", self.variant.application_name())
    }

    async fn install(&self) -> Result<()> {
        if self.is_installed().await.is_ok() {
            return Ok(());
        }

        let application_folder = self.application_folder()?;
        if !application_folder.exists() {
            return Err(Error::Custom(
                format!(
                    "Application folder does not exist for {}: {application_folder:?}",
                    self.variant.application_name()
                )
                .into(),
            ));
        }

        install_plugin(&application_folder).await
    }

    async fn uninstall(&self) -> Result<()> {
        uninstall_plugins(&self.application_folder()?.join("plugins")).await
    }

    async fn is_installed(&self) -> Result<()> {
        let plugin_folder = self.application_folder()?.join("plugins").join(plugin_slug());
        if !plugin_folder.exists() {
            return Err(Error::Custom("Plugin not installed".into()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOREIGN_PLUGINS: &[&str] = &[
        "q-for-command-line-companion-1.0.4",
        "codewhisperer-for-command-line-companion-1.0.4",
        "jetbrains-extension-2.0.0",
        "fig",
        "another-plugin",
        "zevu-terminal-companion-tools",
        "zevu-terminal-companion-extra-0.2.0",
    ];

    fn seed_plugin(plugins_folder: &Path, name: &str) {
        let folder = plugins_folder.join(name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("user-content"), name).unwrap();
    }

    fn assert_foreign_plugins_unchanged(plugins_folder: &Path) {
        for name in FOREIGN_PLUGINS {
            assert_eq!(
                std::fs::read(plugins_folder.join(name).join("user-content")).unwrap(),
                name.as_bytes()
            );
        }
    }

    #[tokio::test]
    async fn install_reinstall_uninstall_preserves_foreign_plugins() {
        let application_folder = tempfile::tempdir().unwrap();
        let plugins_folder = application_folder.path().join("plugins");
        for name in FOREIGN_PLUGINS {
            seed_plugin(&plugins_folder, name);
        }
        let old_plugin = format!("{PLUGIN_PREFIX}-0.1.0");
        seed_plugin(&plugins_folder, &old_plugin);

        install_plugin(application_folder.path()).await.unwrap();
        assert!(!plugins_folder.join(&old_plugin).exists());
        assert_foreign_plugins_unchanged(&plugins_folder);

        let plugin_folder = plugins_folder.join(plugin_slug());
        let jar = plugin_folder.join("lib").join(format!("{}.jar", plugin_slug()));
        assert_eq!(std::fs::read(&jar).unwrap(), PLUGIN_CONTENTS);
        std::fs::write(plugin_folder.join("installed-marker"), b"keep").unwrap();

        install_plugin(application_folder.path()).await.unwrap();
        assert_eq!(std::fs::read(&jar).unwrap(), PLUGIN_CONTENTS);
        assert_eq!(std::fs::read(plugin_folder.join("installed-marker")).unwrap(), b"keep");
        assert_foreign_plugins_unchanged(&plugins_folder);

        uninstall_plugins(&plugins_folder).await.unwrap();
        assert!(!plugin_folder.exists());
        assert_foreign_plugins_unchanged(&plugins_folder);

        uninstall_plugins(&plugins_folder).await.unwrap();
        assert_foreign_plugins_unchanged(&plugins_folder);
    }

    #[tokio::test]
    async fn uninstall_removes_only_zevu_versions() {
        let application_folder = tempfile::tempdir().unwrap();
        let plugins_folder = application_folder.path().join("plugins");
        for name in FOREIGN_PLUGINS {
            seed_plugin(&plugins_folder, name);
        }
        let zevu_versions = [format!("{PLUGIN_PREFIX}-0.1.0"), plugin_slug()];
        for name in &zevu_versions {
            seed_plugin(&plugins_folder, name);
        }

        uninstall_plugins(&plugins_folder).await.unwrap();

        for name in zevu_versions {
            assert!(!plugins_folder.join(name).exists());
        }
        assert_foreign_plugins_unchanged(&plugins_folder);
    }

    #[tokio::test]
    async fn install_creates_missing_plugins_folder_and_uninstall_is_idempotent() {
        let application_folder = tempfile::tempdir().unwrap();
        let plugins_folder = application_folder.path().join("plugins");
        uninstall_plugins(&plugins_folder).await.unwrap();
        assert!(!plugins_folder.exists());

        install_plugin(application_folder.path()).await.unwrap();
        let jar = plugins_folder
            .join(plugin_slug())
            .join("lib")
            .join(format!("{}.jar", plugin_slug()));
        assert_eq!(std::fs::read(jar).unwrap(), PLUGIN_CONTENTS);
        uninstall_plugins(&plugins_folder).await.unwrap();
        uninstall_plugins(&plugins_folder).await.unwrap();
        assert_eq!(std::fs::read_dir(plugins_folder).unwrap().count(), 0);
    }
}
