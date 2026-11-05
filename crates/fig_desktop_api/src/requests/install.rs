use std::fmt::Display;

use anstream::adapter::strip_str;
use fig_integrations::Integration;
use fig_integrations::shell::{ShellExt, shell_executable};
use fig_os_shim::EnvProvider;
use fig_proto::fig::install_response::{
    InstallationStatus,
    Response,
};
use fig_proto::fig::result::Result as ProtoResultEnum;
use fig_proto::fig::server_originated_message::Submessage as ServerOriginatedSubMessage;
use fig_proto::fig::{
    InstallAction,
    InstallComponent,
    InstallRequest,
    InstallResponse,
    Result as ProtoResult,
};
use fig_util::Shell;

use super::RequestResult;


#[cfg(not(target_os = "macos"))]
fn integration_unsupported() -> ServerOriginatedSubMessage {
    ServerOriginatedSubMessage::InstallResponse(InstallResponse {
        response: Some(Response::InstallationStatus(InstallationStatus::NotSupported.into())),
    })
}

fn integration_result(result: Result<(), impl Display>) -> ServerOriginatedSubMessage {
    ServerOriginatedSubMessage::InstallResponse(InstallResponse {
        response: Some(Response::Result(match result {
            Ok(()) => ProtoResult {
                result: ProtoResultEnum::Ok.into(),
                error: None,
            },
            Err(err) => ProtoResult {
                result: ProtoResultEnum::Error.into(),
                error: Some(err.to_string()),
            },
        })),
    })
}

pub async fn install<Ctx>(request: InstallRequest, ctx: &Ctx) -> RequestResult
where
    Ctx: EnvProvider + Send + Sync,
{
    let component = InstallComponent::try_from(request.component).map_err(super::Error::from_std)?;
    let action = InstallAction::try_from(request.action).map_err(super::Error::from_std)?;
    let response = match (component, action) {
        (InstallComponent::Dotfiles, action) => {
            let mut errs: Vec<String> = vec![];
            for shell in Shell::all() {
                if action != InstallAction::Uninstall && shell_executable(*shell, ctx.env()).is_none() {
                    continue;
                }
                match shell.get_script_integrations() {
                    Ok(integrations) => {
                        for integration in integrations {
                            let res = match action {
                                InstallAction::Install => integration.install().await,
                                InstallAction::Uninstall => integration.uninstall().await,
                                InstallAction::Status => integration.is_installed().await,
                            };
                            if let Err(err) = res {
                                errs.push(format!(
                                    "{}: {}",
                                    integration.path.display(),
                                    strip_str(&err.verbose_message().to_string())
                                ));
                            }
                        }
                    },
                    Err(err) => {
                        errs.push(format!("{shell}: {}", strip_str(&err.verbose_message().to_string())));
                    },
                }
                match shell.get_shell_integrations(ctx.env()) {
                    Ok(integrations) => {
                        for integration in integrations {
                            let res = match action {
                                InstallAction::Install => integration.install().await,
                                InstallAction::Uninstall => integration.uninstall().await,
                                InstallAction::Status => integration.is_installed().await,
                            };

                            if let Err(err) = res {
                                errs.push(format!(
                                    "{integration}: {}",
                                    strip_str(&err.verbose_message().to_string())
                                ));
                            }
                        }
                    },
                    Err(err) => {
                        errs.push(format!("{shell}: {}", strip_str(&err.verbose_message().to_string())));
                    },
                }
            }

            match action {
                InstallAction::Install | InstallAction::Uninstall => integration_result(match &errs[..] {
                    [] => Ok(()),
                    errs => Err(errs.join("\n\n")),
                }),
                InstallAction::Status => ServerOriginatedSubMessage::InstallResponse(InstallResponse {
                    response: Some(Response::InstallationStatus(
                        if errs.is_empty() {
                            InstallationStatus::Installed
                        } else {
                            InstallationStatus::NotInstalled
                        }
                        .into(),
                    )),
                }),
            }
        },
        (InstallComponent::Accessibility, InstallAction::Install) => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    use macos_utils::accessibility::{
                        open_accessibility,
                        accessibility_is_enabled
                    };

                    if !accessibility_is_enabled() {
                        open_accessibility();
                    }

                    integration_result(if accessibility_is_enabled() {
                        Ok(())
                    } else {
                        Err("Enable Zevu in System Settings > Privacy & Security > Accessibility")
                    })
                } else {
                    integration_result(Err("Accessibility permissions cannot be queried"))
                }
            }
        },
        (InstallComponent::Accessibility, InstallAction::Status) => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    use macos_utils::accessibility::accessibility_is_enabled;

                    ServerOriginatedSubMessage::InstallResponse(InstallResponse {
                        response: Some(Response::InstallationStatus(if accessibility_is_enabled() {
                            InstallationStatus::Installed.into()
                        } else {
                            InstallationStatus::NotInstalled.into()
                        })),
                    })
                } else {
                    integration_result(Err("Accessibility permissions cannot be queried"))
                }
            }
        },
        (InstallComponent::Accessibility, InstallAction::Uninstall) => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    integration_result(Err(
                        "Revoke Zevu's Accessibility permission in System Settings > Privacy & Security > Accessibility"
                    ))
                } else {
                    integration_result(Err("Accessibility permissions cannot be queried"))
                }
            }
        },
        (InstallComponent::InputMethod, InstallAction::Install) => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    use fig_integrations::input_method::{
                        InputMethod,
                    };
                    use fig_integrations::Integration;

                    integration_result(match InputMethod::default().install().await {
                        Ok(_) => Ok(()),
                        Err(err) => Err(format!("Could not install input method: {err}")),
                    })
                } else {
                    integration_result(Err("Input method install is only supported on macOS"))
                }
            }
        },
        (InstallComponent::InputMethod, InstallAction::Uninstall) => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    use fig_integrations::input_method::{
                        InputMethod,
                        InputMethodError,
                    };
                    use fig_integrations::Error;
                    use fig_integrations::Integration;

                    integration_result(match InputMethod::default().uninstall().await {
                        Ok(_) | Err(Error::InputMethod(InputMethodError::CouldNotListInputSources)) => {
                            Ok(())
                        },
                        Err(err) => Err(format!("Could not uninstall input method: {err}")),
                    })
                } else {
                    integration_result(Err("Input method uninstall is only supported on macOS"))
                }
            }
        },
        (InstallComponent::InputMethod, InstallAction::Status) => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    use fig_integrations::input_method::{
                        InputMethod,
                    };

                    let input_method = InputMethod::default();
                    let installed = input_method.is_installed().await.is_ok()
                        && input_method.is_enabled().unwrap_or(false);
                    ServerOriginatedSubMessage::InstallResponse(InstallResponse {
                        response: Some(Response::InstallationStatus(if installed {
                            InstallationStatus::Installed.into()
                        } else {
                            InstallationStatus::NotInstalled.into()
                        })),
                    })
                } else {
                    integration_unsupported()
                }
            }
        },
    };

    RequestResult::Ok(Box::new(response))
}

#[cfg(test)]
mod tests {
    use fig_os_shim::Env;

    use super::*;

    #[tokio::test]
    async fn removed_components_do_not_fall_back_to_dotfiles() {
        let env = Env::from_slice(&[]);
        for component in [1, 4, 5, 6, 7] {
            let request = InstallRequest {
                component,
                action: InstallAction::Install.into(),
            };
            assert!(install(request, &env).await.is_err());
        }
    }

    #[tokio::test]
    async fn unknown_action_does_not_fall_back_to_install() {
        let env = Env::from_slice(&[]);
        let request = InstallRequest {
            component: InstallComponent::Accessibility.into(),
            action: 100,
        };
        assert!(install(request, &env).await.is_err());
    }
}

