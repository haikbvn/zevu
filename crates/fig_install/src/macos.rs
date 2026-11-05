use fig_integrations::Integration;
use crate::Error;

/// Remove only companions identified as Zevu by their upstream installer implementations.
pub async fn uninstall_terminal_integrations() -> Result<(), Error> {
    for variant in fig_integrations::vscode::variants_installed() {
        fig_integrations::vscode::VSCodeIntegration { variant }.uninstall().await?;
    }
    for integration in fig_integrations::intellij::variants_installed().await? { integration.uninstall().await?; }
    Ok(())
}

pub(crate) async fn uninstall_desktop(_ctx: &fig_os_shim::Context) -> Result<(), Error> {
    let agent = fig_util::directories::home_dir()?.join("Library/LaunchAgents/dev.zevu.autocomplete.plist");
    if agent.exists() {
        let output = tokio::process::Command::new("launchctl").arg("unload").arg(&agent).output().await?;
        if !output.status.success() { tracing::warn!("Could not unload Zevu launch agent: {}", String::from_utf8_lossy(&output.stderr)); }
        tokio::fs::remove_file(agent).await?;
    }
    // Settings, history, backups, caches, and all foreign app integrations remain untouched.
    let app = fig_util::app_bundle_path();
    if app.exists() { tokio::fs::remove_dir_all(app).await?; }
    Ok(())
}
