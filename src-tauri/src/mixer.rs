//! De mixer zoals de gebruiker hem ziet, en de vertaling naar registers.
//!
//! Het model volgt TotalMix: je kiest een uitgangspaar en ziet daarvoor alle
//! ingangen en playback-kanalen als kanaalstrips met fader, pan en mute. Per
//! (uitgangspaar, bron) staat er dus één instelling; de mixer van het apparaat
//! wil per (uitgangskanaal, bron) een gain, en pan is precies het verschil
//! tussen links en rechts.

use serde::{Deserialize, Serialize};

use crate::ff400::{self, Configuratie, KANALEN, MAX_WAARDE, NUL_DB};
use crate::klok::Klok;

/// Bronnen: 0..18 de fysieke ingangen, 18..36 de playback-kanalen.
pub const BRONNEN: usize = 2 * KANALEN;
pub const PAREN: usize = KANALEN / 2;
/// Uitgangspaar 7/8: de koptelefoon op de voorkant.
pub const KOPTELEFOON: usize = 3;

/// Onder deze waarde staat de fader op stil.
pub const UIT_DB: f64 = -90.0;
pub const MAX_DB: f64 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Kruispunt {
    pub db: f64,
    /// -1 is helemaal links, +1 helemaal rechts. Bij een gekoppeld bronpaar
    /// is het de balans en geldt de waarde van het linker kanaal.
    pub pan: f64,
    pub mute: bool,
}

impl Kruispunt {
    const UIT: Self = Self {
        db: UIT_DB,
        pan: 0.0,
        mute: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Uitgang {
    pub db: f64,
    pub mute: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Toestand {
    /// `routes[paar][bron]`
    pub routes: Vec<Vec<Kruispunt>>,
    /// Per bronpaar (0..18): bron 2k en 2k+1 als één stereostrip.
    pub bron_gekoppeld: Vec<bool>,
    /// Per uitgangspaar: volume voor beide kanalen.
    pub uitgangen: Vec<Uitgang>,
    /// Solo per bron; zodra er één aan staat zijn de rest stil.
    pub solo: Vec<bool>,
    /// Ruwe gains voor ingang 1..4.
    pub gains: [u8; 4],
    pub configuratie: Configuratie,
    /// Samplerate en buffergrootte, die bij PipeWire horen en niet bij het
    /// apparaat; zie `klok.rs`.
    pub klok: Klok,
    /// Namen die de gebruiker zelf aan bronnen en uitgangen gaf.
    pub namen: Namen,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Namen {
    pub bronnen: Vec<String>,
    pub uitgangen: Vec<String>,
}

impl Default for Toestand {
    /// Zoals snd-firewire-ctl-services begint: elk playback-kanaal op 0 dB
    /// naar de uitgang met hetzelfde nummer, de ingangen dicht. Daarbij ook
    /// playback 1/2 op de koptelefoon, want daar luistert iedereen eerst.
    ///
    /// De analoge uitgangen beginnen zacht. Het apparaat vertelt niet hoe
    /// ze stonden, en een eerste start mag geen speakers opblazen.
    fn default() -> Self {
        let mut routes = vec![vec![Kruispunt::UIT; BRONNEN]; PAREN];
        let aan = Kruispunt {
            db: 0.0,
            pan: 0.0,
            mute: false,
        };
        for (paar, rij) in routes.iter_mut().enumerate() {
            rij[KANALEN + 2 * paar] = aan;
        }
        routes[KOPTELEFOON][KANALEN] = aan;
        let uitgangen = (0..PAREN)
            .map(|paar| Uitgang {
                db: match paar {
                    0..=2 => -20.0,
                    KOPTELEFOON => -12.0,
                    _ => 0.0,
                },
                mute: false,
            })
            .collect();
        Self {
            routes,
            bron_gekoppeld: vec![true; BRONNEN / 2],
            uitgangen,
            solo: vec![false; BRONNEN],
            gains: [0; 4],
            configuratie: Configuratie::default(),
            klok: Klok::default(),
            namen: Namen::default(),
        }
    }
}

pub fn db_naar_lineair(db: f64) -> f64 {
    if db <= UIT_DB {
        0.0
    } else {
        10f64.powf(db.min(MAX_DB) / 20.0)
    }
}

pub fn lineair_naar_db(v: f64) -> f64 {
    if v <= 0.0 {
        UIT_DB
    } else {
        (20.0 * v.log10()).clamp(UIT_DB, MAX_DB)
    }
}

fn ruw(lineair: f64) -> u32 {
    ((lineair * NUL_DB).round() as u32).min(MAX_WAARDE)
}

impl Toestand {
    /// Maakt een ingelezen (misschien oudere of beschadigde) toestand weer
    /// heel, zodat de rest van de code op de lengtes kan rekenen.
    pub fn herstel(&mut self) {
        let standaard = Toestand::default();
        if self.routes.len() != PAREN || self.routes.iter().any(|r| r.len() != BRONNEN) {
            self.routes = standaard.routes;
        }
        self.bron_gekoppeld.resize(BRONNEN / 2, true);
        if self.uitgangen.len() != PAREN {
            self.uitgangen = standaard.uitgangen;
        }
        self.solo.resize(BRONNEN, false);
        self.gains[0] = self.gains[0].min(ff400::MIC_MAX);
        self.gains[1] = self.gains[1].min(ff400::MIC_MAX);
        self.gains[2] = self.gains[2].min(ff400::LINE_MAX);
        self.gains[3] = self.gains[3].min(ff400::LINE_MAX);
        for rij in &mut self.routes {
            for k in rij.iter_mut() {
                if !k.db.is_finite() {
                    k.db = UIT_DB;
                }
                k.db = k.db.clamp(UIT_DB, MAX_DB);
                k.pan = if k.pan.is_finite() { k.pan.clamp(-1.0, 1.0) } else { 0.0 };
            }
        }
        for u in &mut self.uitgangen {
            u.db = if u.db.is_finite() { u.db.clamp(UIT_DB, MAX_DB) } else { 0.0 };
        }
    }

    /// Links/rechts-versterking van `bron` in uitgangspaar `paar`.
    fn links_rechts(&self, paar: usize, bron: usize) -> (f64, f64) {
        let gekoppeld = self.bron_gekoppeld[bron / 2];
        // Bij een stereostrip bepaalt het linker kanaal fader, balans en mute.
        let leidend = if gekoppeld { bron & !1 } else { bron };
        let k = self.routes[paar][leidend];

        let solo_actief = self.solo.iter().any(|&s| s);
        if k.mute || (solo_actief && !self.solo[leidend]) {
            return (0.0, 0.0);
        }
        let g = db_naar_lineair(k.db);
        // Balans, geen constant-power: in het midden 0 dB naar beide kanten,
        // net als de pan in TotalMix bij 0 dB.
        let l = g * (1.0 - k.pan).min(1.0);
        let r = g * (1.0 + k.pan).min(1.0);
        if gekoppeld {
            if bron % 2 == 0 {
                (l, 0.0)
            } else {
                (0.0, r)
            }
        } else {
            (l, r)
        }
    }

    /// De registerwaarden voor uitgangskanaal `bestemming` (0..18).
    pub fn mixerblok(&self, bestemming: usize) -> ([u32; KANALEN], [u32; KANALEN]) {
        let paar = bestemming / 2;
        let rechts = bestemming % 2 == 1;
        let mut fysiek = [0; KANALEN];
        let mut stream = [0; KANALEN];
        for bron in 0..BRONNEN {
            let (l, r) = self.links_rechts(paar, bron);
            let w = ruw(if rechts { r } else { l });
            if bron < KANALEN {
                fysiek[bron] = w;
            } else {
                stream[bron - KANALEN] = w;
            }
        }
        (fysiek, stream)
    }

    pub fn uitgangswaarde(&self, kanaal: usize) -> u32 {
        let u = self.uitgangen[kanaal / 2];
        if u.mute {
            0
        } else {
            ruw(db_naar_lineair(u.db))
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn standaard_routeert_playback_een_op_een() {
        let t = Toestand::default();
        for bestemming in 0..KANALEN {
            let (fysiek, stream) = t.mixerblok(bestemming);
            assert!(fysiek.iter().all(|&w| w == 0));
            for (j, &w) in stream.iter().enumerate() {
                let koptelefoon = bestemming / 2 == KOPTELEFOON && j == bestemming % 2;
                let verwacht = j == bestemming || koptelefoon;
                assert_eq!(w, if verwacht { 0x8000 } else { 0 }, "{bestemming}/{j}");
            }
        }
    }

    #[test]
    fn mono_pan_links() {
        let mut t = Toestand::default();
        t.bron_gekoppeld[0] = false;
        t.routes[0][0] = Kruispunt {
            db: 0.0,
            pan: -1.0,
            mute: false,
        };
        assert_eq!(t.mixerblok(0).0[0], 0x8000);
        assert_eq!(t.mixerblok(1).0[0], 0);
    }

    #[test]
    fn solo_dempt_de_rest() {
        let mut t = Toestand::default();
        t.solo[KANALEN + 2] = true; // playback 3/4
        assert_eq!(t.mixerblok(0).1[0], 0);
        assert_eq!(t.mixerblok(1).1[1], 0);
    }

    #[test]
    fn plus_zes_is_het_maximum() {
        assert_eq!(ruw(db_naar_lineair(6.0)), 0xff65);
        assert_eq!(ruw(db_naar_lineair(12.0)), 0xff65);
        assert_eq!(ruw(db_naar_lineair(UIT_DB)), 0);
    }
}
