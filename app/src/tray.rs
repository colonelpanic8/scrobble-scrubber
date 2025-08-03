use crate::types::ScrubberStatus;
use dioxus::prelude::*;
use image::ImageReader;
use std::io::Cursor;

/// Format scrubber status for tray menu display
pub fn format_scrubber_status(status: &ScrubberStatus) -> String {
    match status {
        ScrubberStatus::Stopped => "Status: Stopped".to_string(),
        ScrubberStatus::Starting => "Status: Starting...".to_string(),
        ScrubberStatus::Running => "Status: Running".to_string(),
        ScrubberStatus::Sleeping { until_timestamp } => {
            let now = chrono::Utc::now();
            let remaining = (*until_timestamp - now).num_seconds().max(0);
            if remaining > 0 {
                let minutes = remaining / 60;
                let seconds = remaining % 60;
                if minutes > 0 {
                    format!("Status: 💤 Sleeping ({minutes}m {seconds}s)")
                } else {
                    format!("Status: 💤 Sleeping ({remaining}s)")
                }
            } else {
                "Status: 💤 Sleeping".to_string()
            }
        }
        ScrubberStatus::Stopping => "Status: Stopping...".to_string(),
        ScrubberStatus::Error(err) => format!("Status: Error - {err}"),
    }
}

/// Load an icon from raw bytes
pub fn load_icon_from_bytes(
    icon_data: &[u8],
) -> Result<(Vec<u8>, u32, u32), Box<dyn std::error::Error>> {
    let img = ImageReader::new(Cursor::new(icon_data))
        .with_guessed_format()?
        .decode()?
        .to_rgba8();

    let (width, height) = img.dimensions();
    let rgba = img.into_raw();

    Ok((rgba, width, height))
}

/// Create tray icon with menu
pub fn create_tray_icon(
    tray_icon_image: tray_icon::Icon,
    _window: Option<dioxus::desktop::DesktopContext>,
    status_text: &str,
) -> Result<
    (
        tray_icon::TrayIcon,
        crossbeam_channel::Receiver<tray_icon::menu::MenuEvent>,
    ),
    Box<dyn std::error::Error>,
> {
    use tray_icon::{
        menu::{Menu, MenuId, MenuItem, Submenu},
        TrayIconBuilder,
    };

    log::debug!("Creating tray menu items");

    // Create menu items with IDs for handling clicks
    let show_hide_item = MenuItem::with_id(MenuId::new("show_hide"), "Show Window", true, None);
    let separator1 = MenuItem::new("", false, None); // Separator

    // Status item (disabled, shows current state)
    let status_item = MenuItem::new(status_text, false, None);
    let separator_status = MenuItem::new("", false, None); // Separator

    // Scrubber submenu
    let start_scrubber_item =
        MenuItem::with_id(MenuId::new("start_scrubber"), "Start Scrubber", true, None);
    let stop_scrubber_item =
        MenuItem::with_id(MenuId::new("stop_scrubber"), "Stop Scrubber", true, None);
    let process_now_item = MenuItem::with_id(MenuId::new("process_now"), "Process Now", true, None);
    let scrubber_submenu = Submenu::new("Scrubber", true);
    scrubber_submenu.append_items(&[
        &start_scrubber_item,
        &stop_scrubber_item,
        &process_now_item,
    ])?;

    let separator2 = MenuItem::new("", false, None); // Separator
    let config_item = MenuItem::with_id(MenuId::new("config"), "Settings", true, None);
    let about_item = MenuItem::with_id(MenuId::new("about"), "About", true, None);
    let separator3 = MenuItem::new("", false, None); // Separator
    let quit_item = MenuItem::with_id(MenuId::new("quit"), "Exit", true, None);

    let menu = Menu::new();

    menu.append_items(&[
        &show_hide_item,
        &separator1,
        &status_item,
        &separator_status,
        &scrubber_submenu,
        &separator2,
        &config_item,
        &about_item,
        &separator3,
        &quit_item,
    ])?;
    log::debug!("Menu items appended successfully");

    log::debug!("Building tray icon with TrayIconBuilder");

    // Build tray icon with menu and status in tooltip
    let tooltip = if status_text.contains("💤 Sleeping") {
        format!("Scrobble Scrubber - {status_text}\nRight-click for options")
    } else {
        format!("Scrobble Scrubber - {status_text} - Right-click for options")
    };
    let tray_icon = TrayIconBuilder::new()
        .with_tooltip(tooltip)
        .with_icon(tray_icon_image)
        .with_menu(Box::new(menu))
        .build()?;

    log::info!("🔧 TrayIconBuilder.build() completed successfully");

    // Get menu event receiver for handling clicks
    let menu_channel = tray_icon::menu::MenuEvent::receiver().clone();

    Ok((tray_icon, menu_channel))
}

/// Initialize tray icon and handle menu events
pub fn initialize_tray(state: Signal<crate::types::AppState>) {
    let icon_data = include_bytes!("../assets/icons/256x256.png");

    // Load and decode PNG to RGBA for tray icon
    let (rgba, width, height) = match load_icon_from_bytes(icon_data) {
        Ok(data) => data,
        Err(e) => {
            log::warn!("Failed to load tray icon image: {e}");
            return;
        }
    };

    // Create tray icon
    let tray_icon_image = match tray_icon::Icon::from_rgba(rgba, width, height) {
        Ok(icon) => icon,
        Err(e) => {
            log::warn!("Failed to create tray icon image: {e}");
            return;
        }
    };

    // Get current status from app state
    let status_text = format_scrubber_status(&state.read().scrubber_state.status);

    match create_tray_icon(tray_icon_image, None, &status_text) {
        Ok((tray_icon, menu_channel)) => {
            log::info!("System tray icon initialized successfully");

            // Handle menu events
            spawn(async move {
                loop {
                    if let Ok(event) = menu_channel.try_recv() {
                        handle_tray_menu_event(&event.id.0);
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            });

            // Periodically update tray status
            spawn(async move {
                let mut last_status = format_scrubber_status(&state.read().scrubber_state.status);

                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

                    let current_status =
                        format_scrubber_status(&state.read().scrubber_state.status);
                    if current_status != last_status {
                        last_status = current_status.clone();
                        log::debug!("Tray status updated: {current_status}");

                        // For now we just log the status change
                        // In the future we could recreate the tray menu or update the tooltip
                        // The tray-icon crate doesn't seem to support updating individual menu items
                    }
                }
            });

            // Keep tray icon alive
            std::mem::forget(tray_icon);
        }
        Err(e) => {
            log::warn!("Failed to create system tray icon: {e}");
        }
    }
}

/// Handle tray menu events
fn handle_tray_menu_event(menu_id: &str) {
    match menu_id {
        "show_hide" => {
            log::info!("Show/Hide window clicked from tray menu");
            toggle_window_visibility();
        }
        "start_scrubber" => {
            log::info!("Start scrubber clicked from tray menu");
            show_window_if_hidden();
        }
        "stop_scrubber" => {
            log::info!("Stop scrubber clicked from tray menu");
            show_window_if_hidden();
        }
        "process_now" => {
            log::info!("Process now clicked from tray menu");
            show_window_if_hidden();
        }
        "config" => {
            log::info!("Settings clicked from tray menu");
            show_window_if_hidden();
        }
        "about" => {
            log::info!("About clicked from tray menu");
            show_window_if_hidden();
        }
        "quit" => {
            log::info!("Exit clicked from tray menu - shutting down application");
            std::process::exit(0);
        }
        _ => {
            log::warn!("Unknown tray menu item clicked: {menu_id}");
        }
    }
}

/// Toggle window visibility (show if hidden, hide if shown)
fn toggle_window_visibility() {
    use dioxus::desktop::window;

    let win = window();
    let is_visible = win.is_visible();
    if is_visible {
        win.set_visible(false);
        log::info!("Window hidden via tray menu");
    } else {
        win.set_visible(true);
        win.set_focus();
        log::info!("Window shown and focused via tray menu");
    }
}

/// Show window if it's currently hidden
fn show_window_if_hidden() {
    use dioxus::desktop::window;

    let win = window();
    if !win.is_visible() {
        win.set_visible(true);
        win.set_focus();
        log::info!("Window shown via tray menu action");
    }
}
