use std::time::Duration;
use eyre::Result;
use crate::util::desktop::{LaunchArgs, desktop_app_running, launch_fig_desktop};

pub async fn restart_fig() -> Result<()> {
    if desktop_app_running() {
        println!("Restarting Zevu");
        crate::util::quit_fig(false).await?;
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    launch_fig_desktop(LaunchArgs { wait_for_socket: true, open_dashboard: false, verbose: true })
}
