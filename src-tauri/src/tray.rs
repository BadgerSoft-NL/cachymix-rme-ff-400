//! Het icoon in "Status and Notifications". Het venster sluiten verbergt de
//! app alleen; de mixer en de draaiknop blijven het doen zolang hij hier staat.
//!
//! Op Linux loopt dit via StatusNotifierItem (libayatana-appindicator), en daar
//! komen geen klikken op het icoon zelf binnen: alles gaat via het menu.

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::apparaat::Verbinding;
use crate::autostart;

pub struct Tray {
    icoon: TrayIcon<Wry>,
    status: MenuItem<Wry>,
    pub autostart: CheckMenuItem<Wry>,
}

fn icoon(verbonden: bool) -> Option<Image<'static>> {
    let bytes: &'static [u8] = if verbonden {
        include_bytes!("../icons/tray-aan.png")
    } else {
        include_bytes!("../icons/tray-uit.png")
    };
    Image::from_bytes(bytes).ok()
}

pub fn toon_venster(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn maak(app: &AppHandle) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "status", "Fireface 400: searching…", false, None::<&str>)?;
    let toon = MenuItem::with_id(app, "toon", "Show CachyMix", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start at login",
        true,
        autostart::staat_aan(),
        None::<&str>,
    )?;
    let stop = MenuItem::with_id(app, "stop", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &PredefinedMenuItem::separator(app)?,
            &toon,
            &autostart,
            &PredefinedMenuItem::separator(app)?,
            &stop,
        ],
    )?;

    let mut bouwer = TrayIconBuilder::with_id("hoofd")
        .tooltip("CachyMix")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "toon" => toon_venster(app),
            "autostart" => {
                let aan = !autostart::staat_aan();
                let gelukt = autostart::zet(aan).is_ok();
                if let Some(t) = app.try_state::<Tray>() {
                    let _ = t.autostart.set_checked(autostart::staat_aan());
                }
                if gelukt {
                    let _ = app.emit("autostart", aan);
                }
            }
            "stop" => app.exit(0),
            _ => {}
        });
    if let Some(i) = icoon(false) {
        bouwer = bouwer.icon(i);
    }
    let icoon = bouwer.build(app)?;

    app.manage(Tray {
        icoon,
        status,
        autostart,
    });
    Ok(())
}

pub fn werk_bij(app: &AppHandle, v: &Verbinding) {
    let Some(t) = app.try_state::<Tray>() else { return };
    let tekst = match (&v.status, v.verbonden) {
        (Some(s), true) => format!("Fireface 400: connected, {:.1} kHz", s.samplerate as f64 / 1000.0),
        (_, true) => "Fireface 400: connected".to_string(),
        _ => "Fireface 400: not found".to_string(),
    };
    let _ = t.status.set_text(tekst);
    let _ = t.icoon.set_icon(icoon(v.verbonden));
}
