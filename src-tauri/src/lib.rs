pub mod apparaat;
pub mod autostart;
pub mod ff400;
pub mod firewire;
pub mod klok;
pub mod mixer;
pub mod tray;

use tauri::{Manager, State, WindowEvent};

use apparaat::{Apparaat, Verbinding};
use ff400::Configuratie;
use mixer::{Toestand, BRONNEN, MAX_DB, PAREN, UIT_DB};

type Resultaat<T> = Result<T, String>;

fn binnen(index: usize, grens: usize, wat: &str) -> Resultaat<()> {
    if index < grens {
        Ok(())
    } else {
        Err(format!("{wat} {index} does not exist"))
    }
}

#[tauri::command]
fn haal_toestand(a: State<Apparaat>) -> Toestand {
    a.gedeeld.lock().unwrap().toestand.clone()
}

#[tauri::command]
fn haal_verbinding(a: State<Apparaat>) -> Verbinding {
    a.gedeeld.lock().unwrap().verbinding.clone()
}

/// Fader, pan of mute van één bron in één uitgangspaar. Bij een gekoppeld
/// bronpaar gaat het naar het linker kanaal, dat voor beide spreekt.
#[tauri::command]
fn zet_route(
    a: State<Apparaat>,
    paar: usize,
    bron: usize,
    db: Option<f64>,
    pan: Option<f64>,
    mute: Option<bool>,
) -> Resultaat<()> {
    binnen(paar, PAREN, "Output pair")?;
    binnen(bron, BRONNEN, "Source")?;
    a.wijzig(|t, vuil| {
        let bron = if t.bron_gekoppeld[bron / 2] { bron & !1 } else { bron };
        let k = &mut t.routes[paar][bron];
        if let Some(db) = db.filter(|v| v.is_finite()) {
            k.db = db.clamp(UIT_DB, MAX_DB);
        }
        if let Some(pan) = pan.filter(|v| v.is_finite()) {
            k.pan = pan.clamp(-1.0, 1.0);
        }
        if let Some(mute) = mute {
            k.mute = mute;
        }
        vuil.paar(paar);
    });
    Ok(())
}

#[tauri::command]
fn zet_koppeling(a: State<Apparaat>, bronpaar: usize, aan: bool) -> Resultaat<Toestand> {
    binnen(bronpaar, BRONNEN / 2, "Source pair")?;
    Ok(a.wijzig(|t, vuil| {
        t.bron_gekoppeld[bronpaar] = aan;
        let l = 2 * bronpaar;
        for rij in t.routes.iter_mut() {
            if aan {
                // Het linker kanaal gaat voor beide spreken; zet de balans in
                // het midden, anders klinkt de stereostrip ineens scheef.
                rij[l].pan = 0.0;
            } else {
                // Uit elkaar: elk kanaal houdt het niveau en gaat naar zijn kant.
                rij[l + 1] = rij[l];
                rij[l].pan = -1.0;
                rij[l + 1].pan = 1.0;
            }
        }
        t.solo[l + 1] = t.solo[l];
        vuil.mixer();
        t.clone()
    }))
}

#[tauri::command]
fn zet_solo(a: State<Apparaat>, bron: usize, aan: bool) -> Resultaat<Toestand> {
    binnen(bron, BRONNEN, "Source")?;
    Ok(a.wijzig(|t, vuil| {
        let bron = if t.bron_gekoppeld[bron / 2] { bron & !1 } else { bron };
        t.solo[bron] = aan;
        vuil.mixer();
        t.clone()
    }))
}

#[tauri::command]
fn wis_solo(a: State<Apparaat>) -> Toestand {
    a.wijzig(|t, vuil| {
        t.solo.iter_mut().for_each(|s| *s = false);
        vuil.mixer();
        t.clone()
    })
}

#[tauri::command]
fn zet_uitgang(
    a: State<Apparaat>,
    paar: usize,
    db: Option<f64>,
    mute: Option<bool>,
) -> Resultaat<()> {
    binnen(paar, PAREN, "Output pair")?;
    a.wijzig(|t, vuil| {
        let u = &mut t.uitgangen[paar];
        if let Some(db) = db.filter(|v| v.is_finite()) {
            u.db = db.clamp(UIT_DB, MAX_DB);
        }
        if let Some(mute) = mute {
            u.mute = mute;
        }
        vuil.uitgangen[2 * paar] = true;
        vuil.uitgangen[2 * paar + 1] = true;
    });
    Ok(())
}

#[tauri::command]
fn zet_gain(a: State<Apparaat>, kanaal: usize, niveau: u8) -> Resultaat<()> {
    binnen(kanaal, 4, "Preamp")?;
    let max = if kanaal < 2 { ff400::MIC_MAX } else { ff400::LINE_MAX };
    a.wijzig(|t, vuil| {
        t.gains[kanaal] = niveau.min(max);
        vuil.gains[kanaal] = true;
    });
    Ok(())
}

#[tauri::command]
fn zet_configuratie(a: State<Apparaat>, configuratie: Configuratie) -> Toestand {
    a.wijzig(|t, vuil| {
        t.configuratie = configuratie;
        vuil.configuratie = true;
        t.clone()
    })
}

#[tauri::command]
fn zet_naam(a: State<Apparaat>, soort: String, index: usize, naam: String) -> Resultaat<Toestand> {
    let naam: String = naam.trim().chars().take(24).collect();
    a.wijzig(|t, _| {
        let lijst = match soort.as_str() {
            "bron" => &mut t.namen.bronnen,
            "uitgang" => &mut t.namen.uitgangen,
            _ => return Err(format!("Unknown kind {soort}")),
        };
        if lijst.len() <= index {
            lijst.resize(index + 1, String::new());
        }
        lijst[index] = naam;
        Ok(t.clone())
    })
}

/// Een uitgangspaar leegmaken, of alles terug naar het begin.
#[tauri::command]
fn wis_paar(a: State<Apparaat>, paar: usize) -> Resultaat<Toestand> {
    binnen(paar, PAREN, "Output pair")?;
    Ok(a.wijzig(|t, vuil| {
        for k in t.routes[paar].iter_mut() {
            k.db = UIT_DB;
            k.mute = false;
        }
        vuil.paar(paar);
        t.clone()
    }))
}

#[tauri::command]
fn begin_opnieuw(a: State<Apparaat>) -> Toestand {
    a.wijzig(|t, vuil| {
        let configuratie = t.configuratie;
        let klok = t.klok;
        let namen = std::mem::take(&mut t.namen);
        *t = Toestand::default();
        t.configuratie = configuratie;
        t.klok = klok;
        t.namen = namen;
        *vuil = apparaat::Vuil::alles();
        t.clone()
    })
}

#[tauri::command]
fn haal_klok() -> Resultaat<klok::KlokNu> {
    klok::lees()
}

/// Samplerate en buffer via PipeWire. Pas na gelukt zetten komt de keuze in
/// de toestand, zodat die nooit iets belooft wat niet is gebeurd.
// Async: het zetten wacht even op PipeWire, en dat hoort niet op de UI-draad.
#[tauri::command(async)]
fn zet_klok(a: State<Apparaat>, keuze: klok::Klok) -> Resultaat<Toestand> {
    klok::zet(&keuze)?;
    Ok(a.wijzig(|t, _| {
        t.klok = keuze;
        t.clone()
    }))
}

#[tauri::command]
fn haal_autostart() -> bool {
    autostart::staat_aan()
}

#[tauri::command]
fn zet_autostart(app: tauri::AppHandle, aan: bool) -> Resultaat<bool> {
    autostart::zet(aan)?;
    let nu = autostart::staat_aan();
    if let Some(t) = app.try_state::<tray::Tray>() {
        let _ = t.autostart.set_checked(nu);
    }
    Ok(nu)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Een tweede start (uit het menu, terwijl hij al in de tray staat)
        // haalt alleen het venster van de eerste tevoorschijn.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            tray::toon_venster(app)
        }))
        .on_window_event(|venster, e| {
            // Sluiten is verbergen: de app blijft in de tray, zodat de
            // draaiknop en het opnieuw verbinden blijven werken.
            if let WindowEvent::CloseRequested { api, .. } = e {
                api.prevent_close();
                let _ = venster.hide();
            }
        })
        .setup(|app| {
            tray::maak(app.handle())?;
            autostart::werk_bij();
            // Met --hidden (zo start de autostart hem) blijft het venster dicht.
            if !std::env::args().any(|a| a == "--hidden") {
                tray::toon_venster(app.handle());
            }
            let bestand = app.path().app_config_dir()?.join("toestand.json");
            let toestand = apparaat::laad(&bestand);
            // PipeWire vergeet de geforceerde waarden bij een herstart; de
            // drop-in vangt dat op, maar zo is het ook meteen weer zo.
            klok::herstel(&toestand.klok);
            app.manage(Apparaat::start(app.handle().clone(), toestand, bestand));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            haal_toestand,
            haal_verbinding,
            zet_route,
            zet_koppeling,
            zet_solo,
            wis_solo,
            zet_uitgang,
            zet_gain,
            zet_configuratie,
            zet_naam,
            wis_paar,
            begin_opnieuw,
            haal_klok,
            zet_klok,
            haal_autostart,
            zet_autostart,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
