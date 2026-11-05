//! Install only local Zevu integrations.
use std::process::ExitCode;
use eyre::Result;
use fig_install::{InstallComponents, install};

pub async fn install_cli(components: InstallComponents, no_confirm: bool) -> Result<ExitCode> {
    if !no_confirm && !dialoguer::Confirm::new().with_prompt("Install Zevu shell and detected IDE integrations?").default(true).interact()? {
        return Ok(ExitCode::SUCCESS);
    }
    install(components, &fig_os_shim::Env::new()).await?;
    println!("Zevu local integrations installed. Restart existing terminal sessions to load them.");
    Ok(ExitCode::SUCCESS)
}
