// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{
    CustomMenuItem, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu, SystemTrayMenuItem,
    SystemTraySubmenu,
};

use std::thread;
use std::time::Duration;

use std::sync::Arc;

use serde::Deserialize;
use serde_json;
use std::process::Command;

use arboard::Clipboard;

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
const QUIT_ID: &str = "quit";

fn start_resource_auth(auth_id: &str) {
    let resource_id = auth_id.split("-").last().unwrap();

    let n = get_network_data();

    let idx = n
        .resources
        .iter()
        .position(|x| x.id == resource_id)
        .unwrap();

    // TODO: what do we do if pkexec isn't there?
    Command::new("pkexec")
        .args(["twingate", "auth", &n.resources[idx].name])
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
    let status_cmd = &Command::new("twingate").arg("status").output().unwrap();
    let status = std::str::from_utf8(&status_cmd.stdout).unwrap();

    // TODO: should check for other status. Just assuming only not-running and online
    if status == "not-running" {
        // TODO: should check for failure here
        let _ = Command::new("twingate").arg("start").output();
    }

    let mut tg_notifier = Command::new("twingate-notifier");

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
        .add_item(CustomMenuItem::new(QUIT_ID, "Close Tray"))
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

    // library doesn't supprt nested submenus =(, so we will just add separator and extra header
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
                    Command::new("pkexec")
                        .args(["twingate", "stop"])
                        .spawn()
                        .unwrap();
                }
                address_id if address_id.contains(COPY_ADDRESS_ID) => {
                    handle_copy_address(address_id);
                }
                auth_id if auth_id.contains(AUTHENTICATE_ID) => {
                    start_resource_auth(auth_id);
                }
                unfound => {
                    println!("unfound: {}", unfound);
                }
            },
            _ => {}
        })
        .setup(|app| {
            let tray_handle_original =
                Arc::new(app.app_handle().tray_handle_by_id(tray_id).unwrap());

            let tray_handle = tray_handle_original.clone();

            // left/right click events are not currently supported in linux, which could have been a way 
            // to update the menu before being display.
            // since we can't do that, instead we will spawn an infinite thread that re-builds the menu every 3 seconds
            std::thread::spawn(move || loop {
                let _ = tray_handle.set_menu(build_menu());

                thread::sleep(Duration::from_secs(3));
            });
            Ok(())
        });

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
