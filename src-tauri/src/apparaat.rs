//! De draad die met de Fireface praat.
//!
//! De Tauri-commando's veranderen alleen de [`Toestand`] en zetten vlaggetjes
//! voor wat er opnieuw naar het apparaat moet. Deze draad pakt die op, schrijft
//! ze weg en leest tussendoor de meters. Zo gaan tien faderbewegingen tussen
//! twee rondes als één schrijfactie de draad op, en wacht de UI nooit op de bus.

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::ff400::{self, Knop, Status, KANALEN, NUL_DB};
use crate::firewire::Node;
use crate::mixer::{lineair_naar_db, Toestand};

const METER_INTERVAL: Duration = Duration::from_millis(40);
const STATUS_INTERVAL: Duration = Duration::from_millis(1000);
const ZOEK_INTERVAL: Duration = Duration::from_millis(2000);
const OPSLAAN_NA: Duration = Duration::from_millis(800);

#[derive(Debug, Clone, Default)]
pub struct Vuil {
    pub bestemmingen: [bool; KANALEN],
    pub uitgangen: [bool; KANALEN],
    pub gains: [bool; 4],
    pub configuratie: bool,
}

impl Vuil {
    pub fn alles() -> Self {
        Self {
            bestemmingen: [true; KANALEN],
            uitgangen: [true; KANALEN],
            gains: [true; 4],
            configuratie: true,
        }
    }

    pub fn paar(&mut self, paar: usize) {
        self.bestemmingen[2 * paar] = true;
        self.bestemmingen[2 * paar + 1] = true;
    }

    pub fn mixer(&mut self) {
        self.bestemmingen = [true; KANALEN];
    }

    fn iets(&self) -> bool {
        self.configuratie
            || self.bestemmingen.iter().any(|&v| v)
            || self.uitgangen.iter().any(|&v| v)
            || self.gains.iter().any(|&v| v)
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Verbinding {
    pub verbonden: bool,
    pub status: Option<Status>,
    pub melding: Option<String>,
}

pub struct Gedeeld {
    pub toestand: Toestand,
    pub vuil: Vuil,
    pub verbinding: Verbinding,
    pub gewijzigd: Option<Instant>,
}

#[derive(Clone)]
pub struct Apparaat {
    pub gedeeld: Arc<Mutex<Gedeeld>>,
    wekker: Sender<()>,
    /// Er mag maar één draad de hwdep-node lezen, ook als een verbinding
    /// wegvalt terwijl de kaart er nog is.
    knop_luistert: Arc<AtomicBool>,
}

impl Apparaat {
    pub fn start(app: AppHandle, toestand: Toestand, bestand: PathBuf) -> Self {
        let gedeeld = Arc::new(Mutex::new(Gedeeld {
            toestand,
            vuil: Vuil::default(),
            verbinding: Verbinding::default(),
            // Ook bij de eerste start meteen wegschrijven, dan staat er
            // altijd een bestand om in te kijken.
            gewijzigd: (!bestand.exists()).then(Instant::now),
        }));
        let (wekker, wacht) = mpsc::channel();
        let a = Apparaat {
            gedeeld,
            wekker,
            knop_luistert: Arc::new(AtomicBool::new(false)),
        };
        let b = a.clone();
        thread::Builder::new()
            .name("fireface".into())
            .spawn(move || b.draai(app, wacht, bestand))
            .expect("draad starten");
        a
    }

    /// Voert `f` uit op de gedeelde toestand en maakt de draad wakker.
    pub fn wijzig<R>(&self, f: impl FnOnce(&mut Toestand, &mut Vuil) -> R) -> R {
        let mut g = self.gedeeld.lock().unwrap();
        let Gedeeld { toestand, vuil, .. } = &mut *g;
        let r = f(toestand, vuil);
        g.gewijzigd = Some(Instant::now());
        drop(g);
        let _ = self.wekker.send(());
        r
    }

    fn draai(self, app: AppHandle, wacht: Receiver<()>, bestand: PathBuf) {
        let mut node: Option<Node> = None;
        let mut laatst_gezocht: Option<Instant> = None;
        let mut volgende_meter = Instant::now();
        let mut volgende_status = Instant::now();

        loop {
            let nu = Instant::now();

            // ---- verbinden
            if node.is_none() && laatst_gezocht.is_none_or(|t| nu - t >= ZOEK_INTERVAL) {
                laatst_gezocht = Some(nu);
                match self.verbind(&app) {
                    Ok(n) => node = Some(n),
                    Err(melding) => self.meld(&app, false, None, Some(melding)),
                }
            }

            // ---- schrijven wat er veranderd is
            if let Some(n) = node.as_mut() {
                let (toestand, vuil) = {
                    let mut g = self.gedeeld.lock().unwrap();
                    if g.vuil.iets() {
                        (Some(g.toestand.clone()), std::mem::take(&mut g.vuil))
                    } else {
                        (None, Vuil::default())
                    }
                };
                if let Some(t) = toestand {
                    if let Err(e) = schrijf(n, &t, &vuil) {
                        // Wat niet aankwam moet bij de volgende verbinding
                        // alsnog; die zet sowieso alles opnieuw neer.
                        node = None;
                        self.meld(&app, false, None, Some(format!("Write failed: {e}")));
                        continue;
                    }
                }
            }

            // ---- meters en status
            if let Some(n) = node.as_mut() {
                if Instant::now() >= volgende_meter {
                    volgende_meter = Instant::now() + METER_INTERVAL;
                    match ff400::lees_meters(n) {
                        Ok(m) => {
                            let _ = app.emit("meters", m);
                        }
                        Err(e) => {
                            node = None;
                            self.meld(&app, false, None, Some(format!("Lost contact: {e}")));
                            continue;
                        }
                    }
                }
                if Instant::now() >= volgende_status {
                    volgende_status = Instant::now() + STATUS_INTERVAL;
                    if let Ok(s) = ff400::lees_status(n) {
                        let anders = self.gedeeld.lock().unwrap().verbinding.status != Some(s);
                        if anders {
                            self.meld(&app, true, Some(s), None);
                        }
                    }
                }
            }

            // ---- bewaren, even na de laatste wijziging
            let te_bewaren = {
                let mut g = self.gedeeld.lock().unwrap();
                match g.gewijzigd {
                    Some(t) if t.elapsed() >= OPSLAAN_NA => {
                        g.gewijzigd = None;
                        Some(g.toestand.clone())
                    }
                    _ => None,
                }
            };
            if let Some(t) = te_bewaren {
                bewaar(&bestand, &t);
            }

            // ---- wachten op een wijziging of de volgende meterronde
            let slaap = if node.is_some() {
                volgende_meter.saturating_duration_since(Instant::now())
            } else {
                Duration::from_millis(250)
            };
            match wacht.recv_timeout(slaap) {
                Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            while wacht.try_recv().is_ok() {}
        }
    }

    fn verbind(&self, app: &AppHandle) -> Result<Node, String> {
        let gevonden = ff400::zoek().ok_or_else(|| "No Fireface 400 found on the bus".to_string())?;
        let mut node = Node::open(&gevonden.fw)
            .map_err(|e| format!("Cannot open {}: {e}", gevonden.fw.display()))?;
        let status = ff400::lees_status(&mut node).map_err(|e| format!("No answer: {e}"))?;

        {
            let mut g = self.gedeeld.lock().unwrap();
            g.toestand.configuratie.neem_over(&status);
            // Het apparaat onthoudt niets dat we kunnen teruglezen, dus alles
            // gaat er opnieuw heen.
            g.vuil = Vuil::alles();
        }
        self.meld(app, true, Some(status), None);
        let _ = app.emit("toestand", self.gedeeld.lock().unwrap().toestand.clone());

        let al_bezig = self.knop_luistert.swap(true, Ordering::SeqCst);
        if let (Some(pad), false) = (gevonden.hwdep, al_bezig) {
            let a = self.clone();
            let app = app.clone();
            thread::Builder::new()
                .name("ff400-knop".into())
                .spawn(move || {
                    let vlag = a.knop_luistert.clone();
                    a.luister_naar_knop(app, pad);
                    vlag.store(false, Ordering::SeqCst);
                })
                .ok();
        } else if !al_bezig {
            self.knop_luistert.store(false, Ordering::SeqCst);
        }
        Ok(node)
    }

    fn meld(&self, app: &AppHandle, verbonden: bool, status: Option<Status>, melding: Option<String>) {
        let v = Verbinding {
            verbonden,
            status,
            melding,
        };
        {
            let mut g = self.gedeeld.lock().unwrap();
            if g.verbinding.verbonden == v.verbonden
                && g.verbinding.status == v.status
                && g.verbinding.melding == v.melding
            {
                return;
            }
            g.verbinding = v.clone();
        }
        crate::tray::werk_bij(app, &v);
        let _ = app.emit("verbinding", v);
    }

    /// De draaiknop op de voorkant. snd-fireface geeft zijn berichten door via
    /// de hwdep-node; de nieuwe waarde moet de computer zelf terugschrijven.
    fn luister_naar_knop(self, app: AppHandle, pad: PathBuf) {
        const FF400_BERICHT: u32 = 0x4f6c_6761;
        let Ok(mut f) = File::open(&pad) else { return };
        let mut buf = [0u8; 1024];
        loop {
            let n = match f.read(&mut buf) {
                Ok(0) | Err(_) => return, // kaart weg; de volgende verbinding start een nieuwe
                Ok(n) => n,
            };
            if n < 8 {
                continue;
            }
            let woord = |i: usize| u32::from_ne_bytes(buf[i..i + 4].try_into().unwrap());
            if woord(0) != FF400_BERICHT {
                continue;
            }
            let aantal = (woord(4) as usize).min((n - 8) / 8);
            let knoppen: Vec<Knop> = (0..aantal)
                .flat_map(|i| ff400::ontleed_knop(woord(8 + i * 8)))
                .collect();
            if knoppen.is_empty() {
                continue;
            }
            let t = self.wijzig(|t, vuil| {
                for k in knoppen {
                    match k {
                        Knop::Gain { kanaal, niveau } => {
                            let max = if kanaal < 2 { ff400::MIC_MAX } else { ff400::LINE_MAX };
                            t.gains[kanaal] = niveau.min(max);
                            vuil.gains[kanaal] = true;
                        }
                        Knop::Volume { kanaal, waarde } => {
                            let u = &mut t.uitgangen[kanaal / 2];
                            u.db = lineair_naar_db(waarde as f64 / NUL_DB);
                            vuil.uitgangen[kanaal & !1] = true;
                            vuil.uitgangen[kanaal | 1] = true;
                        }
                    }
                }
                t.clone()
            });
            let _ = app.emit("toestand", t);
        }
    }
}

fn schrijf(node: &mut Node, t: &Toestand, vuil: &Vuil) -> std::io::Result<()> {
    if vuil.configuratie {
        ff400::schrijf_configuratie(node, &t.configuratie)?;
    }
    for (kanaal, _) in vuil.gains.iter().enumerate().filter(|(_, &v)| v) {
        ff400::schrijf_ingangsgain(node, kanaal as u32, t.gains[kanaal])?;
    }
    for (bestemming, _) in vuil.bestemmingen.iter().enumerate().filter(|(_, &v)| v) {
        let (fysiek, stream) = t.mixerblok(bestemming);
        ff400::schrijf_mixer(node, bestemming, &fysiek, &stream)?;
    }
    for (kanaal, _) in vuil.uitgangen.iter().enumerate().filter(|(_, &v)| v) {
        ff400::schrijf_uitgang(node, kanaal, t.uitgangswaarde(kanaal))?;
    }
    Ok(())
}

pub fn laad(bestand: &PathBuf) -> Toestand {
    let mut t: Toestand = std::fs::read_to_string(bestand)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    t.herstel();
    t
}

pub fn bewaar(bestand: &PathBuf, t: &Toestand) {
    if let Some(map) = bestand.parent() {
        let _ = std::fs::create_dir_all(map);
    }
    // Eerst ernaast en dan erover, zodat een crash halverwege geen half
    // bestand achterlaat.
    let tijdelijk = bestand.with_extension("json.tmp");
    if let Ok(s) = serde_json::to_string_pretty(t) {
        if std::fs::write(&tijdelijk, s).is_ok() {
            let _ = std::fs::rename(&tijdelijk, bestand);
        }
    }
}
