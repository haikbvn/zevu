use std::process::ExitCode;
use eyre::Result;

pub async fn uninstall_command(no_confirm: bool) -> Result<ExitCode> {
    if !no_confirm && !dialoguer::Confirm::new().with_prompt("Uninstall Zevu? Settings, history, and backups will be preserved.").default(false).interact()? {
        return Ok(ExitCode::SUCCESS);
    }
    crate::util::quit_fig(false).await?;
    fig_install::uninstall(fig_install::InstallComponents::all(), fig_os_shim::Context::new()).await?;
    Ok(ExitCode::SUCCESS)
}
