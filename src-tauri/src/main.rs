// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{
    CustomMenuItem, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu, SystemTrayMenuItem,
    SystemTraySubmenu,
};

use std::cell::RefCell;
use std::thread;
use std::time::{Duration, Instant};

use std::sync::{Arc, Mutex};

use serde::Deserialize;
use serde::Serialize;
use serde_json;

use arboard::Clipboard;

use chrono::*;

#[derive(Clone, Deserialize)]
struct Alias {
    address: String,
    open_url: String,
}

#[derive(Clone, Deserialize)]
struct Resource {
    address: String,
    admin_url: String,
    aliases: Option<Vec<Alias>>,
    auth_expires_at: i64,
    auth_flow_id: String,
    can_open_in_browser: bool,
    id: String,
    is_visible_in_client: bool,
    name: String,
    open_url: String,
    #[serde(rename = "type")]
    resource_type: String,
}

#[derive(Clone, Deserialize)]
struct User {
    avatar_url: String,
    email: String,
    first_name: String,
    id: String,
    is_admin: bool,
    last_name: String,
}

#[derive(Clone, Deserialize)]
struct Network {
    admin_url: String,
    resources: Vec<Resource>,
    user: User,
}

const USER_STATUS_ID: &str = "user_status";
const STOP_SERVICE_ID: &str = "stop_service";
const NUMBER_RESOURCES_ID: &str = "num_resources";
const RESOURCE_ADDRESS_ID: &str = "resource_address";
const COPY_ADDRESS_ID: &str = "copy_address";
const AUTHENTICATE_ID: &str = "authenticate";

fn start_resource_auth(auth_id: &str) {
    let resource_id = auth_id.split("-").last().unwrap();

    let n = get_network_data();

    let idx = n
        .resources
        .iter()
        .position(|x| x.id == resource_id)
        .unwrap();

    // TODO: not sure how to do sudo?
    std::process::Command::new("twingate")
        .args(["auth", &n.resources[idx].name])
        .spawn()
        .unwrap();
}

fn build_resource_menu(resource: &Resource) -> SystemTraySubmenu {
    let mut menu = SystemTrayMenu::new()
        .add_item(
            CustomMenuItem::new(
                format!("{}-{}", RESOURCE_ADDRESS_ID, &resource.id),
                &resource.address,
            )
            .disabled(),
        )
        .add_item(CustomMenuItem::new(
            format!("{}-{}", COPY_ADDRESS_ID, &resource.id),
            "Copy Address",
        ))
        .add_native_item(SystemTrayMenuItem::Separator);

    if resource.auth_expires_at == 0 {
        menu = menu
            .clone()
            .add_item(CustomMenuItem::new("auth_required", "Authentication Required").disabled());

        menu = menu.clone().add_item(CustomMenuItem::new(
            format!("{}-{}", AUTHENTICATE_ID, resource.id),
            "Authenticate...",
        ))
    } else {
        menu = menu.clone().add_item(
            CustomMenuItem::new(
                "auth_required",
                format!(
                    "Auth expires in {} days",
                    // TODO: needs to show hours for between 0 and 1 day left
                    chrono::Duration::milliseconds(resource.auth_expires_at.clone()).num_days()
                ),
            )
            .disabled(),
        );
    }

    SystemTraySubmenu::new(&resource.name, menu)
}

fn get_network_data() -> Network {
    let mut tg_notifier = std::process::Command::new("twingate-notifier");

    // TODO: need to handle when twingate isn't started

    serde_json::from_slice(&tg_notifier.arg("resources").output().unwrap().stdout).unwrap()
}

fn build_menu() -> SystemTrayMenu {
    let n: Network = get_network_data();

    let visible_resources: Vec<_> = n
        .resources
        .iter()
        .filter(|r| r.is_visible_in_client)
        .collect();

    let mut menu = SystemTrayMenu::new()
        .add_item(CustomMenuItem::new(USER_STATUS_ID, n.user.email))
        .add_item(CustomMenuItem::new(
            STOP_SERVICE_ID,
            "Stop Twingate Service",
        ))
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(
            CustomMenuItem::new(
                NUMBER_RESOURCES_ID,
                format!("{} Resources", visible_resources.len()),
            )
            .disabled(),
        );

    // probably better way to write this
    // we could also loop over all resources instead and divvy them up between the two menus
    for r in visible_resources.into_iter() {
        menu = menu.clone().add_submenu(build_resource_menu(r));
    }

    let background_resources: Vec<_> = n
        .resources
        .iter()
        .filter(|r| !r.is_visible_in_client)
        .collect();

    // library doesn't supprt nested submenu's, so we will just add separator and extra header
    menu = menu
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(
            CustomMenuItem::new(
                "background_resources_count",
                format!("{} Background Resources", background_resources.len()),
            )
            .disabled(),
        );

    for r in background_resources.into_iter() {
        menu = menu.clone().add_submenu(build_resource_menu(r));
    }

    menu
}

fn handle_copy_address(address_id: &str) {
    let resource_id = address_id.split("-").last().unwrap();

    let n = get_network_data();

    let idx = n
        .resources
        .iter()
        .position(|x| x.id == resource_id)
        .unwrap();

    let mut clipboard = Clipboard::new().unwrap();

    clipboard
        .set_text(n.resources[idx].address.clone())
        .unwrap()
}

fn main() {
    let tray_id = "tray_id";

    let builder = tauri::Builder::default()
        .system_tray(SystemTray::new().with_id(tray_id))
        .on_system_tray_event(|app, event| match event {
            SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
                "quit" => {
                    std::process::exit(0);
                }
                STOP_SERVICE_ID => {
                    std::process::Command::new("twingate")
                        .arg("stop")
                        .spawn()
                        .unwrap();
                }
                address_id if address_id.contains(COPY_ADDRESS_ID) => {
                    handle_copy_address(address_id);
                }
                auth_id if auth_id.contains(AUTHENTICATE_ID) => {
                    start_resource_auth(auth_id);
                }
                y => {
                    println!("y: {}", y);
                }
            },
            _ => {}
        })
        .setup(|app| {
            let window = app.get_window("main").unwrap();
            // // this is a workaround for the window to always show in current workspace.
            // // see https://github.com/tauri-apps/tauri/issues/2801
            window.set_always_on_top(true).unwrap();
            
            let tray_handle_original =
                Arc::new(app.app_handle().tray_handle_by_id(tray_id).unwrap());

            let tray_handle = tray_handle_original.clone();

            // left/right click events are not currently supported in linux, which could have been a way 
            // to update the menu before display.
            // since we can't do that, instead we will spawn an infinite thread that re-builds the menu every 3 seconds
            std::thread::spawn(move || loop {
                println!("update loop");

                let _ = tray_handle.set_menu(build_menu());

                thread::sleep(Duration::from_secs(3));
            });
            Ok(())
        });

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
