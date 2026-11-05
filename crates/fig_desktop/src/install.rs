use std::sync::Arc;

use fig_integrations::Integration;
use fig_os_shim::Context;
use tracing::{error, info};

/// Install only local Zevu resources and companions for applications already present.
pub async fn run_install(_ctx: Arc<Context>) {
    #[cfg(target_os = "macos")]
    {
        if let Err(err) = initialize_fig_dir(&fig_os_shim::Env::new()).await {
            error!(%err, "Failed installing local Zevu resources");
        }

        let input_method = fig_integrations::input_method::InputMethod::default();
        if input_method.is_installed().await.is_err() {
            if let Err(err) = input_method.install().await {
                error!(%err, "Failed installing Zevu input method");
            }
        }

        for variant in fig_integrations::vscode::variants_installed() {
            let integration = fig_integrations::vscode::VSCodeIntegration { variant };
            if integration.is_installed().await.is_err() {
                info!("Installing Zevu companion for {}", integration.variant.application_name);
                if let Err(err) = integration.install().await {
                    error!(%err, "Failed installing companion for {}", integration.variant.application_name);
                }
            }
        }

        match fig_integrations::intellij::variants_installed().await {
            Ok(variants) => {
                for integration in variants {
                    if integration.is_installed().await.is_err() {
                        if let Err(err) = integration.install().await {
                            error!(%err, "Failed installing companion for {}", integration.variant.application_name());
                        }
                    }
                }
            },
            Err(err) => error!(%err, "Failed detecting JetBrains applications"),
        }
    }
}

#[cfg(target_os = "macos")]
pub async fn initialize_fig_dir(env: &fig_os_shim::Env) -> anyhow::Result<()> {
    use fig_integrations::shell::{ShellExt, shell_executable};
    use fig_util::{APP_BUNDLE_ID, APP_PROCESS_NAME, Shell};
    use fig_util::launchd_plist::{LaunchdPlist, create_launch_agent};
    use macos_utils::bundle::get_bundle_path;

    tokio::fs::create_dir_all(fig_util::directories::fig_data_dir()?.join("shell")).await?;

    fig_install::install_binary_links()?;

    if let Some(bundle_path) = get_bundle_path() {
        let executable = bundle_path.join("Contents/MacOS").join(APP_PROCESS_NAME);
        let launch_agent = LaunchdPlist::new("dev.zevu.autocomplete.launcher")
            .program_arguments([&executable.to_string_lossy(), "--is-startup", "--no-dashboard"])
            .associated_bundle_identifiers([APP_BUNDLE_ID])
            .run_at_load(true);
        create_launch_agent(&launch_agent)?;
        std::process::Command::new("launchctl")
            .arg("load").arg(launch_agent.get_file_path()?).status().ok();
    }

    for shell in Shell::all() {
        if shell_executable(*shell, env).is_none() {
            continue;
        }
        for integration in shell.get_script_integrations()? {
            integration.install().await?;
        }
        for integration in shell.get_shell_integrations(env)? {
            integration.install().await?;
        }
    }
    Ok(())
}

