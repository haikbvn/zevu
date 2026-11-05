use std::process::ExitCode;
use clap::Subcommand;
use eyre::Result;
use fig_integrations::shell::ShellExt;
use fig_os_shim::Env;
use fig_util::Shell;
use super::OutputFormat;

#[derive(Debug, PartialEq, Eq, Subcommand)]
pub enum IntegrationsSubcommands {
    Install { #[command(subcommand)] integration: Integration, #[arg(long, short)] silent: bool },
    Uninstall { #[command(subcommand)] integration: Integration, #[arg(long, short)] silent: bool },
    Reinstall { #[command(subcommand)] integration: Integration, #[arg(long, short)] silent: bool },
    Status { #[command(subcommand)] integration: Integration, #[arg(long, short, value_enum, default_value_t)] format: OutputFormat },
}
#[derive(Debug, Subcommand, Clone, Copy, PartialEq, Eq)]
pub enum Integration {
    Dotfiles { #[arg(value_enum)] shell: Option<Shell> },
    InputMethod,
    #[command(name = "vscode")]
    VSCode,
    #[command(alias = "jetbrains-plugin")]
    IntellijPlugin,
    All,
}

impl IntegrationsSubcommands {
    pub async fn execute(self) -> Result<ExitCode> {
        let (integration, action, silent, format) = match self {
            Self::Install { integration, silent } => (integration, "install", silent, OutputFormat::Plain),
            Self::Uninstall { integration, silent } => (integration, "uninstall", silent, OutputFormat::Plain),
            Self::Reinstall { integration, silent } => (integration, "reinstall", silent, OutputFormat::Plain),
            Self::Status { integration, format } => (integration, "status", false, format),
        };
        let env = Env::new();
        let selections = if integration == Integration::All {
            vec![Integration::Dotfiles { shell: None }, Integration::InputMethod, Integration::VSCode, Integration::IntellijPlugin]
        } else { vec![integration] };
        let mut items: Vec<Box<dyn fig_integrations::Integration>> = Vec::new();
        for selection in selections {
            match selection {
                Integration::Dotfiles { shell } => {
                    let shells = shell.map_or_else(|| Shell::all().to_vec(), |shell| vec![shell]);
                    for shell in shells {
                        if action != "uninstall" && fig_integrations::shell::shell_executable(shell, &env).is_none() { continue; }
                        for item in shell.get_shell_integrations(&env)? { items.push(Box::new(ShellItem(item))); }
                    }
                },
                Integration::InputMethod => items.push(Box::new(fig_integrations::input_method::InputMethod::default())),
                Integration::VSCode => for variant in fig_integrations::vscode::variants_installed() { items.push(Box::new(fig_integrations::vscode::VSCodeIntegration { variant })); },
                Integration::IntellijPlugin => for item in fig_integrations::intellij::variants_installed().await? { items.push(Box::new(item)); },
                Integration::All => unreachable!(),
            }
        }
        let mut status = serde_json::Map::new();
        let mut failed = false;
        for item in items {
            if action == "uninstall" || action == "reinstall" { item.uninstall().await?; }
            if action == "install" || action == "reinstall" { item.install().await?; }
            if action == "status" {
                let result = item.is_installed().await;
                failed |= result.is_err();
                status.insert(item.describe(), match result { Ok(()) => serde_json::json!("installed"), Err(err) => serde_json::json!(err.to_string()) });
            } else if !silent { println!("{}: {action} complete", item.describe()); }
        }
        if action == "status" {
            format.print(|| status.iter().map(|(name, value)| format!("{name}: {value}")).collect::<Vec<_>>().join("\n"), || &status);
        }
        Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS })
    }
}

struct ShellItem(Box<dyn fig_integrations::shell::ShellIntegration>);
#[async_trait::async_trait]
impl fig_integrations::Integration for ShellItem {
    fn describe(&self) -> String { self.0.describe() }
    async fn install(&self) -> fig_integrations::Result<()> { self.0.install().await }
    async fn uninstall(&self) -> fig_integrations::Result<()> { self.0.uninstall().await }
    async fn is_installed(&self) -> fig_integrations::Result<()> { self.0.is_installed().await }
}
