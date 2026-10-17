//! Het icoon in "Status and Notifications". Het venster sluiten verbergt de
//! app alleen; de mixer en de draaiknop blijven het doen zolang hij hier staat.
//!
//! Niet via de tray van Tauri: die loopt op Linux over libayatana-appindicator,
//! en daar bestaat alleen een menu. Een klik op het icoon toont dan dat menu in
//! plaats van de app te openen. `ksni` spreekt StatusNotifierItem zelf, dus
//! komt de linkerklik binnen als `activate` en het menu op de rechterklik.

use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, StandardItem};
use ksni::{Icon, MenuItem, ToolTip};
use tauri::image::Image;
use tauri::{AppHandle, Emitter, Manager};

use crate::apparaat::Verbinding;
use crate::autostart;

pub struct Tray(Handle<Icoon>);

pub struct Icoon {
    app: AppHandle,
    verbonden: bool,
    status: String,
    autostart: bool,
}

/// PNG naar wat StatusNotifierItem wil: ARGB32 in netwerkvolgorde.
fn pixmap(verbonden: bool) -> Vec<Icon> {
    let bytes: &[u8] = if verbonden {
        include_bytes!("../icons/tray-aan.png")
    } else {
        include_bytes!("../icons/tray-uit.png")
    };
    let Ok(beeld) = Image::from_bytes(bytes) else {
        return Vec::new();
    };
    let data = beeld
        .rgba()
        .chunks_exact(4)
        .flat_map(|p| [p[3], p[0], p[1], p[2]])
        .collect();
    vec![Icon {
        width: beeld.width() as i32,
        height: beeld.height() as i32,
        data,
    }]
}

pub fn toon_venster(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Een klik op het icoon: open als hij weg is, weg als hij open is — zoals
/// de andere iconen in de tray van KDE het doen.
fn wissel_venster(app: &AppHandle) {
    match app.get_webview_window("main") {
        Some(w) if w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false) => {
            let _ = w.hide();
        }
        _ => toon_venster(app),
    }
}

fn zet_autostart(icoon: &mut Icoon, aan: bool) {
    if autostart::zet(aan).is_ok() {
        let _ = icoon.app.emit("autostart", aan);
    }
    icoon.autostart = autostart::staat_aan();
}

impl ksni::Tray for Icoon {
    fn id(&self) -> String {
        "cachymix".into()
    }

    fn title(&self) -> String {
        "CachyMix".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::Hardware
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        pixmap(self.verbonden)
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "CachyMix".into(),
            description: self.status.clone(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        wissel_venster(&self.app);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: self.status.clone(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Show CachyMix".into(),
                activate: Box::new(|t: &mut Self| toon_venster(&t.app)),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Start at login".into(),
                checked: self.autostart,
                activate: Box::new(|t: &mut Self| {
                    let aan = !t.autostart;
                    zet_autostart(t, aan);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|t: &mut Self| t.app.exit(0)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

pub fn maak(app: &AppHandle) {
    let icoon = Icoon {
        app: app.clone(),
        verbonden: false,
        status: "Fireface 400: searching…".into(),
        autostart: autostart::staat_aan(),
    };
    // Zonder tray (geen StatusNotifierWatcher) draait de app gewoon door,
    // alleen zonder icoon.
    if let Ok(handle) = icoon.spawn() {
        app.manage(Tray(handle));
    }
}

pub fn werk_bij(app: &AppHandle, v: &Verbinding) {
    let Some(t) = app.try_state::<Tray>() else { return };
    let status = match (&v.status, v.verbonden) {
        (Some(s), true) => format!("Fireface 400: connected, {:.1} kHz", s.samplerate as f64 / 1000.0),
        (_, true) => "Fireface 400: connected".to_string(),
        _ => "Fireface 400: not found".to_string(),
    };
    let verbonden = v.verbonden;
    t.0.update(move |i| {
        i.verbonden = verbonden;
        i.status = status;
    });
}

/// Na een wijziging vanuit het Settings-paneel het vinkje gelijk trekken.
pub fn autostart_gewijzigd(app: &AppHandle) {
    if let Some(t) = app.try_state::<Tray>() {
        t.0.update(|i| i.autostart = autostart::staat_aan());
    }
}
