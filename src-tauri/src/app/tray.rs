//! The system tray icon that keeps the app running after its window is closed.

use tauri::menu::{Menu, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use super::{shutdown, MAIN_LABEL};

const TRAY_ID: &str = "anki-mining-tray";
const SHOW_ID: &str = "show";
const QUIT_ID: &str = "quit";

/// Shows the tray icon when `enabled` and removes it otherwise. Turning it off also brings the
/// window back, so the app never runs with neither a window nor an icon.
pub fn sync_tray(app: &AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        if app.tray_by_id(TRAY_ID).is_some() {
            return Ok(());
        }
        return build_tray(app).map_err(|error| format!("Could not create the tray icon: {error}"));
    }
    app.remove_tray_by_id(TRAY_ID);
    show_main_window(app);
    Ok(())
}

/// Hides the main window instead of closing the app.
pub fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.hide();
    }
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, SHOW_ID, "Show AnkiMining", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("AnkiMining")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            handle_menu(app, &event);
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

fn handle_menu(app: &AppHandle, event: &MenuEvent) {
    match event.id().as_ref() {
        SHOW_ID => {
            show_main_window(app);
        }
        QUIT_ID => {
            shutdown(app);
            app.exit(0);
        }
        _ => {}
    }
}
