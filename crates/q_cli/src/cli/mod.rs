//! Local Zevu command-line interface.

pub mod app;
mod completion;
mod debug;
mod diagnostics;
mod doctor;
mod hook;
mod init;
mod installation;
mod integrations;
pub mod internal;
mod settings;
mod theme;
mod uninstall;

use std::process::ExitCode;
use clap::{ArgAction, CommandFactory, Parser, Subcommand, ValueEnum};
use eyre::{Result, WrapErr};
use fig_ipc::local::open_ui_element;
use fig_log::{LogArgs, initialize_logging};
use fig_proto::local::UiElement;
use fig_util::{CLI_BINARY_NAME, PRODUCT_NAME, directories};
use serde::Serialize;
use tracing::{Level, debug};
use self::integrations::IntegrationsSubcommands;
use crate::util::CliContext;
use crate::util::desktop::{LaunchArgs, launch_fig_desktop};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Plain,
    Json,
    JsonPretty,
}

impl OutputFormat {
    pub fn print<T, TFn, J, JFn>(&self, text_fn: TFn, json_fn: JFn)
    where T: std::fmt::Display, TFn: FnOnce() -> T, J: Serialize, JFn: FnOnce() -> J {
        match self {
            Self::Plain => println!("{}", text_fn()),
            Self::Json => println!("{}", serde_json::to_string(&json_fn()).unwrap()),
            Self::JsonPretty => println!("{}", serde_json::to_string_pretty(&json_fn()).unwrap()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Processes { App }

#[derive(Debug, PartialEq, Subcommand)]
pub enum CliRootCommands {
    /// Hook commands used by shell and IDE integrations
    #[command(subcommand, hide = true)]
    Hook(hook::HookSubcommand),
    /// Debug the local application
    #[command(subcommand)]
    Debug(debug::DebugSubcommand),
    /// Customize appearance and behavior
    Settings(settings::SettingsArgs),
    /// Install local shell and IDE integrations
    Setup(internal::InstallArgs),
    /// Uninstall Zevu without removing settings, history, or backups
    Uninstall {
        #[arg(long, short = 'y')]
        no_confirm: bool,
    },
    /// Show local diagnostics
    Diagnostic(diagnostics::DiagnosticArgs),
    /// Generate shell integration code
    Init(init::InitArgs),
    /// Get or set the theme
    Theme(theme::ThemeArgs),
    /// Diagnose local autocomplete prerequisites
    Doctor(doctor::DoctorArgs),
    /// Generate CLI completions
    #[command(hide = true)]
    Completion(completion::CompletionArgs),
    /// Integration implementation commands
    #[command(subcommand, hide = true)]
    Internal(internal::InternalSubcommand),
    /// Launch Zevu
    Launch,
    /// Quit Zevu
    Quit,
    /// Restart Zevu
    Restart {
        #[arg(value_enum, default_value_t = Processes::App, hide = true)]
        process: Processes,
    },
    /// Manage local integrations
    #[command(subcommand)]
    Integrations(IntegrationsSubcommands),
    /// Print the product version
    Version,
    /// Open the dashboard
    Dashboard,
}

#[derive(Debug, Parser, PartialEq, Default)]
#[command(version, about = "Zevu local terminal autocomplete", name = CLI_BINARY_NAME)]
pub struct Cli {
    #[command(subcommand)]
    pub subcommand: Option<CliRootCommands>,
    /// Increase logging verbosity
    #[arg(long, short = 'v', action = ArgAction::Count, global = true)]
    pub verbose: u8,
    /// Print detailed help
    #[arg(long)]
    help_all: bool,
}

impl Cli {
    pub async fn execute(self) -> Result<ExitCode> {
        let _log_guard = initialize_logging(LogArgs {
            log_level: match self.verbose {
                0 => None, 1 => Some(Level::WARN.to_string()), 2 => Some(Level::INFO.to_string()),
                3 => Some(Level::DEBUG.to_string()), _ => Some(Level::TRACE.to_string()),
            },
            log_to_stdout: fig_os_shim::Env::new().q_log_stdout() || self.verbose > 0,
            log_file_path: if fig_log::get_log_level_max() >= Level::DEBUG {
                Some(directories::logs_dir()?.join("cli.log"))
            } else { None },
            delete_old_log_file: false,
        });
        debug!(command = ?std::env::args().collect::<Vec<_>>(), "Command ran");
        if self.help_all {
            Self::command().print_long_help()?;
            return Ok(ExitCode::SUCCESS);
        }
        let cli_context = CliContext::new();
        match self.subcommand {
            Some(CliRootCommands::Setup(args)) => {
                let no_confirm = args.no_confirm;
                installation::install_cli(args.into(), no_confirm).await
            },
            Some(CliRootCommands::Uninstall { no_confirm }) => uninstall::uninstall_command(no_confirm).await,
            Some(CliRootCommands::Diagnostic(args)) => args.execute().await,
            Some(CliRootCommands::Init(args)) => args.execute().await,
            Some(CliRootCommands::Doctor(args)) => args.execute().await,
            Some(CliRootCommands::Hook(args)) => args.execute().await,
            Some(CliRootCommands::Theme(args)) => args.execute().await,
            Some(CliRootCommands::Settings(args)) => args.execute(&cli_context).await,
            Some(CliRootCommands::Debug(args)) => args.execute().await,
            Some(CliRootCommands::Completion(args)) => args.execute(),
            Some(CliRootCommands::Internal(args)) => args.execute().await,
            Some(CliRootCommands::Launch) => {
                launch_fig_desktop(LaunchArgs { wait_for_socket: true, open_dashboard: false, verbose: true })?;
                Ok(ExitCode::SUCCESS)
            },
            Some(CliRootCommands::Quit) => crate::util::quit_fig(true).await,
            Some(CliRootCommands::Restart { .. }) => {
                app::restart_fig().await?;
                Ok(ExitCode::SUCCESS)
            },
            Some(CliRootCommands::Integrations(args)) => args.execute().await,
            Some(CliRootCommands::Version) => {
                print!("{}", Self::command().render_version());
                Ok(ExitCode::SUCCESS)
            },
            Some(CliRootCommands::Dashboard) => launch_dashboard().await,
            None => {
                Self::command().print_help()?;
                Ok(ExitCode::SUCCESS)
            },
        }
    }
}

async fn launch_dashboard() -> Result<ExitCode> {
    launch_fig_desktop(LaunchArgs { wait_for_socket: true, open_dashboard: true, verbose: true })?;
    println!("Opening {PRODUCT_NAME} dashboard");
    open_ui_element(UiElement::MissionControl, Some("/".into())).await.context("Failed to open dashboard")?;
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn command_definition() { Cli::command().debug_assert(); }

    #[test]
    fn local_setup_and_launch() {
        assert!(matches!(Cli::parse_from([CLI_BINARY_NAME, "setup", "--no-confirm"]).subcommand,
            Some(CliRootCommands::Setup(internal::InstallArgs { no_confirm: true, .. }))));
        assert_eq!(Cli::parse_from([CLI_BINARY_NAME, "launch"]).subcommand, Some(CliRootCommands::Launch));
        for removed in ["login", "logout", "chat", "mcp", "agent", "inline", "translate", "telemetry", "update", "start"] {
            assert!(Cli::try_parse_from([CLI_BINARY_NAME, removed]).is_err(), "{removed}");
        }
    }
}
