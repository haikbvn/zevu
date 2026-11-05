use fig_remote_ipc::figterm::FigtermState;
use fig_util::consts::PRODUCT_NAME;
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tao::event_loop::ControlFlow;
use tracing::trace;
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::event::{Event, WindowEvent};
use crate::{AUTOCOMPLETE_ID, DASHBOARD_ID, EventLoopProxy, EventLoopWindowTarget};

pub fn handle_event(menu_event: &MenuEvent, proxy: &EventLoopProxy) {
    let path = match &*menu_event.id().0 {
        "dashboard" => Some("/"),
        "settings" => Some("/autocomplete"),
        "integrations" => Some("/integrations"),
        "licenses" => Some("/licenses"),
        "dashboard-devtools" => {
            proxy.send_event(Event::WindowEvent {
                window_id: DASHBOARD_ID,
                window_event: WindowEvent::Devtools,
            }).unwrap();
            None
        },
        "autocomplete-devtools" => {
            proxy.send_event(Event::WindowEvent {
                window_id: AUTOCOMPLETE_ID,
                window_event: WindowEvent::Devtools,
            }).unwrap();
            None
        },
        "quit" => {
            proxy.send_event(Event::ControlFlow(ControlFlow::Exit)).unwrap();
            None
        },
        id => {
            trace!(?id, "Unhandled tray event");
            None
        },
    };
    if let Some(path) = path {
        proxy.send_event(Event::WindowEvent {
            window_id: DASHBOARD_ID,
            window_event: WindowEvent::Batch(vec![
                WindowEvent::NavigateRelative { path: path.into() },
                WindowEvent::Show,
            ]),
        }).unwrap();
    }
}

pub async fn build_tray(
    _event_loop_window_target: &EventLoopWindowTarget,
    _figterm_state: &FigtermState,
) -> tray_icon::Result<TrayIcon> {
    TrayIconBuilder::new()
        .with_icon(get_icon())
        .with_icon_as_template(true)
        .with_menu(Box::new(get_context_menu()))
        .build()
}

fn get_icon() -> Icon {
    let image = image::load_from_memory(include_bytes!("../icons/icon-monochrome.png"))
        .expect("Failed to load tray icon")
        .into_rgba8();
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height).expect("Invalid tray icon")
}

fn get_context_menu() -> Menu {
    let menu = Menu::new();
    for (id, label) in [
        ("dashboard", format!("Open {PRODUCT_NAME}")),
        ("settings", "Autocomplete Settings".into()),
        ("integrations", "Integrations and Permissions".into()),
        ("licenses", "Open Source Licenses".into()),
    ] {
        menu.append(&MenuItem::with_id(id, label, true, None)).unwrap();
    }
    menu.append(&PredefinedMenuItem::separator()).unwrap();
    menu.append(&MenuItem::new(format!("Version {}", env!("CARGO_PKG_VERSION")), false, None)).unwrap();
    menu.append(&PredefinedMenuItem::separator()).unwrap();
    menu.append(&MenuItem::with_id("quit", format!("Quit {PRODUCT_NAME}"), true, None)).unwrap();
    menu
}
