pub mod local_state;
pub mod should_figterm_launch;

use std::io::{
    Read,
    Write,
    stdout,
};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anstream::println;
use bytes::{
    Buf,
    BytesMut,
};
use clap::{
    ArgGroup,
    Args,
    Subcommand,
    ValueEnum,
};
use crossterm::style::Stylize;
use eyre::{
    Context,
    ContextCompat,
    Result,
    bail,
};
use fig_install::InstallComponents;
#[cfg(target_os = "macos")]
use fig_integrations::input_method::InputMethod;
use fig_ipc::local::send_hook_to_socket;
use fig_ipc::{
    BufferedUnixStream,
    SendMessage,
    SendRecvMessage,
};
use fig_os_shim::{
    Context as OsContext,
};
use fig_proto::ReflectMessage;
use fig_proto::figterm::figterm_request_message::Request as FigtermRequest;
use fig_proto::figterm::{
    FigtermRequestMessage,
    UpdateShellContextRequest,
};
use fig_proto::hooks::{
    new_callback_hook,
    new_event_hook,
};
use fig_proto::local::EnvironmentVariable;
use fig_proto::util::get_shell;
use fig_util::directories::{
    figterm_socket_path,
};
use fig_util::env_var::QTERM_SESSION_ID;
use fig_util::{
    directories,
};
use rand::distr::{
    Alphanumeric,
    SampleString,
};
use sysinfo::System;
use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};
use tokio::select;
use tracing::{
    debug,
    error,
    info,
    trace,
};

use crate::cli::installation::install_cli;

#[derive(Debug, Args, PartialEq, Eq)]
#[command(group(
        ArgGroup::new("output")
            .args(&["filename", "exit_code"])
            .multiple(true)
            .requires_all(&["filename", "exit_code"])
            ))]
pub struct CallbackArgs {
    handler_id: String,
    #[arg(group = "output")]
    filename: Option<String>,
    #[arg(group = "output")]
    exit_code: Option<i64>,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct InstallArgs {
    /// Install only the shell integrations
    #[arg(long)]
    pub dotfiles: bool,
    /// Prompt input method installation
    #[arg(long)]
    pub input_method: bool,
    /// Don't confirm automatic installation.
    #[arg(long)]
    pub no_confirm: bool,
}

impl From<InstallArgs> for InstallComponents {
    fn from(args: InstallArgs) -> Self {
        let InstallArgs {
            dotfiles, input_method, ..
        } = args;
        if dotfiles || input_method {
            let mut install_components = InstallComponents::empty();
            install_components.set(InstallComponents::SHELL_INTEGRATIONS, dotfiles);
            install_components.set(InstallComponents::INPUT_METHOD, input_method);
            install_components
        } else {
            InstallComponents::all()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum StateComponent {
    Figterm,
    WebNotifications,
    Platform,
}

#[derive(Debug, PartialEq, Eq, Subcommand)]
#[command(hide = true, alias = "_")]
pub enum InternalSubcommand {
    /// Command that is run during the PreCmd section of
    /// the Amazon Q integrations.
    PreCmd {
        #[arg(long, allow_hyphen_values = true)]
        alias: Option<String>,
    },
    /// Change the local-state file
    LocalState(local_state::LocalStateArgs),
    /// Callback used for the internal pseudoterminal
    Callback(CallbackArgs),
    /// Install the Amazon Q cli
    Install(InstallArgs),
    /// Uninstall the Amazon Q cli
    Uninstall {
        /// Uninstall only the shell integrations
        #[arg(long)]
        dotfiles: bool,
        /// Uninstall only the input method
        #[arg(long)]
        input_method: bool,
        /// Uninstall only the binary
        #[arg(long)]
        binary: bool,
    },
    GetShell,
    Hostname,
    /// Detects if Figterm should be launched
    ///
    /// Exit code:
    /// - 0 execute figterm
    /// - 1 dont execute figterm
    /// - 2 fallback to Q_TERM env
    ShouldFigtermLaunch,
    Event {
        /// Name of the event.
        #[arg(long)]
        name: String,
        /// Payload of the event as a JSON string.
        #[arg(long)]
        payload: Option<String>,
        /// Apps to send the event to.
        #[arg(long)]
        apps: Vec<String>,
    },
    SocketsDir,
    StreamFromSocket,
    FigtermSocketPath {
        session_id: String,
    },
    #[command(group(
        ArgGroup::new("target")
            .multiple(false)
            .required(true)
    ))]
    Ipc {
        #[arg(long, group = "target")]
        app: bool,
        #[arg(long, group = "target")]
        figterm: Option<String>,
        #[arg(long)]
        json: String,
        #[arg(long)]
        recv: bool,
    },
    Uuidgen,
    #[cfg(target_os = "macos")]
    AttemptToFinishInputMethodInstallation {
        bundle_path: Option<PathBuf>,
    },
    DumpState {
        component: StateComponent,
    },
}

const BUFFER_SIZE: usize = 1024;

impl InternalSubcommand {
    pub async fn execute(self) -> Result<ExitCode> {
        let ctx = OsContext::new();
        match self {
            InternalSubcommand::Install(args) => {
                let no_confirm = args.no_confirm;
                install_cli(args.into(), no_confirm).await
            },
            InternalSubcommand::Uninstall {
                dotfiles,
                input_method,
                binary,
            } => {
                let components = if dotfiles || binary || input_method {
                    let mut uninstall_components = InstallComponents::empty();
                    uninstall_components.set(InstallComponents::SHELL_INTEGRATIONS, dotfiles);
                    uninstall_components.set(InstallComponents::INPUT_METHOD, input_method);
                    uninstall_components.set(InstallComponents::BINARY, binary);
                    uninstall_components
                } else {
                    InstallComponents::all()
                };
                if components.contains(InstallComponents::BINARY) {
                    if option_env!("Q_IS_PACKAGE_MANAGED").is_some() {
                        println!("Please uninstall using your package manager");
                    } else {
                        fig_install::uninstall(InstallComponents::BINARY, Arc::clone(&ctx)).await?;
                        println!("\n{}\n", "The binary was successfully uninstalled".bold());
                    }
                }

                let mut components = components;
                components.set(InstallComponents::BINARY, false);
                fig_install::uninstall(components, Arc::clone(&ctx)).await?;
                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::PreCmd { alias } => Ok(pre_cmd(alias).await),
            InternalSubcommand::LocalState(local_state) => {
                local_state.execute().await?;
                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::Callback(CallbackArgs {
                handler_id,
                filename,
                exit_code,
            }) => {
                trace!("handlerId: {handler_id}");

                let (filename, exit_code) = match (filename, exit_code) {
                    (Some(filename), Some(exit_code)) => {
                        trace!("callback specified filepath ({filename}) and exitCode ({exit_code}) to output!");
                        (filename, exit_code)
                    },
                    _ => {
                        let file_id = Alphanumeric.sample_string(&mut rand::rng(), 9);
                        let tmp_filename = format!("fig-callback-{file_id}");
                        let tmp_path = PathBuf::from("/tmp").join(tmp_filename);
                        let mut tmp_file = std::fs::File::create(&tmp_path)?;
                        let mut buffer = [0u8; BUFFER_SIZE];
                        let mut stdin = std::io::stdin();
                        trace!("Created tmp file: {}", tmp_path.display());

                        loop {
                            let size = stdin.read(&mut buffer)?;
                            if size == 0 {
                                break;
                            }
                            tmp_file.write_all(&buffer[..size])?;
                            trace!("Read {size} bytes\n{}", std::str::from_utf8(&buffer[..size])?);
                        }

                        let filename: String = tmp_path.to_str().context("invalid file path")?.into();
                        trace!("Done reading from stdin!");
                        (filename, -1)
                    },
                };
                let hook = new_callback_hook(&handler_id, &filename, exit_code);

                info!(
                    "Sending 'handlerId: {handler_id}, filename: {filename}, exitcode: {exit_code}' over unix socket!\n"
                );

                match send_hook_to_socket(hook).await {
                    Ok(()) => debug!("Successfully sent hook"),
                    Err(e) => debug!("Couldn't send hook {e}"),
                }

                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::GetShell => match get_shell() {
                Ok(shell) => {
                    if write!(stdout(), "{shell}").is_ok() {
                        return Ok(ExitCode::SUCCESS);
                    }
                    Ok(ExitCode::FAILURE)
                },
                Err(_) => Ok(ExitCode::FAILURE),
            },
            InternalSubcommand::Hostname => {
                if let Some(hostname) = System::host_name() {
                    if write!(stdout(), "{hostname}").is_ok() {
                        return Ok(ExitCode::SUCCESS);
                    }
                }
                Ok(ExitCode::FAILURE)
            },
            InternalSubcommand::ShouldFigtermLaunch => {
                Ok(should_figterm_launch::should_figterm_launch(&OsContext::new()))
            },
            InternalSubcommand::Event { payload, apps, name } => {
                let hook = new_event_hook(name, payload, apps);
                send_hook_to_socket(hook).await?;
                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::Ipc {
                app,
                figterm,
                json,
                recv,
            } => {
                let message = fig_proto::FigMessage::json(serde_json::from_str::<serde_json::Value>(&json)?)?;

                let socket = if app {
                    directories::desktop_socket_path().expect("Failed to get socket path")
                } else if let Some(ref figterm) = figterm {
                    directories::figterm_socket_path(figterm).expect("Failed to get socket path")
                } else {
                    bail!("No destination for message");
                };

                let mut conn = BufferedUnixStream::connect(socket).await?;

                if recv {
                    macro_rules! recv {
                        ($abc:path) => {{
                            let response: Option<$abc> = conn.send_recv_message(message).await?;
                            match response {
                                Some(response) => {
                                    let message = response.transcode_to_dynamic();
                                    println!("{}", serde_json::to_string(&message)?)
                                },
                                None => bail!("Received EOF while waiting for response"),
                            }
                        }};
                    }

                    if app {
                        recv!(fig_proto::local::CommandResponse);
                    } else if figterm.is_some() {
                        recv!(fig_proto::figterm::FigtermResponseMessage);
                    }
                } else {
                    conn.send_message(message).await?;
                }

                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::SocketsDir => {
                writeln!(stdout(), "{}", directories::sockets_dir_utf8()?).ok();
                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::FigtermSocketPath { session_id } => {
                writeln!(
                    stdout(),
                    "{}",
                    directories::figterm_socket_path(session_id)?.to_string_lossy()
                )
                .ok();
                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::StreamFromSocket => {
                let mut stdout = tokio::io::stdout();
                let mut stdin = tokio::io::stdin();

                let mut stdout_buf = BytesMut::with_capacity(1024);
                let mut stream_buf = BytesMut::with_capacity(1024);

                let socket = directories::remote_socket_path()?;
                while let Ok(mut stream) = BufferedUnixStream::connect_timeout(&socket, Duration::from_secs(5)).await {
                    loop {
                        select! {
                            n = stream.read_buf(&mut stdout_buf) => {
                                match n {
                                    Ok(0) | Err(_) => {
                                        break;
                                    }
                                    Ok(mut n) => {
                                        while !stdout_buf.is_empty() {
                                            let m = stdout.write(&stdout_buf[..n]).await?;
                                            stdout.flush().await?;
                                            stdout_buf.advance(m);
                                            n -= m;
                                        }
                                        stdout_buf.clear();
                                    }
                                }
                            }
                            n = stdin.read_buf(&mut stream_buf) => {
                                match n {
                                    Ok(0) | Err(_) => {
                                        break;
                                    }
                                    Ok(mut n) => {
                                        while !stream_buf.is_empty() {
                                            let m = stream.write(&stream_buf[..n]).await?;
                                            stream.flush().await?;
                                            stream_buf.advance(m);
                                            n -= m;
                                        }
                                        stream_buf.clear();
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::Uuidgen => {
                let _ = writeln!(stdout(), "{}", uuid::Uuid::new_v4());
                Ok(ExitCode::SUCCESS)
            },
            #[cfg(target_os = "macos")]
            InternalSubcommand::AttemptToFinishInputMethodInstallation { bundle_path } => {
                match InputMethod::finish_input_method_installation(bundle_path) {
                    Ok(_) => Ok(ExitCode::SUCCESS),
                    Err(err) => {
                        println!(
                            "{}",
                            serde_json::to_string(&err).expect("InputMethodError should be serializable")
                        );
                        Ok(ExitCode::FAILURE)
                    },
                }
            },
            InternalSubcommand::DumpState { component } => {
                use fig_proto::local::dump_state_command::Type as StateCommandType;

                let state = fig_ipc::local::dump_state_command(match component {
                    StateComponent::Figterm => StateCommandType::DumpStateFigterm,
                    StateComponent::WebNotifications => StateCommandType::DumpStateWebNotifications,
                    StateComponent::Platform => StateCommandType::DumpStatePlatform,
                })
                .await
                .context("Failed to send dump state command")?;

                println!("{}", state.json);
                Ok(ExitCode::SUCCESS)
            },
        }
    }
}

pub async fn pre_cmd(alias: Option<String>) -> ExitCode {
    let Ok(session_id) = std::env::var(QTERM_SESSION_ID) else {
        return ExitCode::FAILURE;
    };

    match figterm_socket_path(&session_id) {
        Ok(figterm_path) => match fig_ipc::socket_connect(figterm_path).await {
            Ok(mut figterm_stream) => {
                let message = FigtermRequestMessage {
                    request: Some(FigtermRequest::UpdateShellContext(UpdateShellContextRequest {
                        update_environment_variables: true,
                        environment_variables: std::env::vars()
                            .map(|(key, value)| EnvironmentVariable {
                                key,
                                value: Some(value),
                            })
                            .collect(),
                        update_alias: true,
                        alias,
                    })),
                };
                if let Err(err) = figterm_stream.send_message(message).await {
                    error!(%err, %session_id, "Failed to send UpdateShellContext to Figterm");
                    ExitCode::FAILURE
                } else {
                    ExitCode::SUCCESS
                }
            },
            Err(err) => {
                error!(%err, %session_id, "Failed to connect to Figterm socket");
                ExitCode::FAILURE
            },
        },
        Err(err) => {
            error!(%err, %session_id, "Failed to get Figterm socket path");
            ExitCode::FAILURE
        },
    }
}


#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Debug, Parser, PartialEq, Eq)]
    pub struct MockCli {
        #[command(subcommand)]
        pub subcommand: InternalSubcommand,
    }

    #[test]
    fn parse_pre_cmd() {
        assert_eq!(MockCli::parse_from(["_", "pre-cmd"]), MockCli {
            subcommand: InternalSubcommand::PreCmd { alias: None }
        });


        let hyphen_alias = "-='cd -'\n...=../..\nga='git add'";
        assert_eq!(
            MockCli::parse_from(["_", "pre-cmd", "--alias", hyphen_alias]),
            MockCli {
                subcommand: InternalSubcommand::PreCmd {
                    alias: Some(hyphen_alias.to_owned())
                }
            }
        );
    }

}
