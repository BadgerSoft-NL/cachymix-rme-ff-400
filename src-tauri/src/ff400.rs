//! Het registerprotocol van de Fireface 400.
//!
//! Overgenomen uit `protocols/fireface/src/former{,/ff400}.rs` van
//! snd-firewire-ctl-services (Takashi Sakamoto, LGPL-3.0). Daar staat de
//! reverse engineering; hier staan alleen de adressen en bitjes die deze app
//! gebruikt.
//!
//! Het belangrijkste om te weten: mixer, uitgangsvolumes, gains en de
//! configuratie zijn **alleen te schrijven**. Wat het apparaat nu doet is niet
//! terug te lezen, dus de app houdt zelf de waarheid bij en zet hem bij elke
//! verbinding opnieuw neer. Alleen de status en de meters zijn leesbaar.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::firewire::Node;

const MIXER_OFFSET: u64 = 0x0000_8008_0000;
const OUTPUT_OFFSET: u64 = 0x0000_8008_0f80;
const METER_OFFSET: u64 = 0x0000_8010_0000;
const CFG_OFFSET: u64 = 0x0000_8010_0514;
const STATUS_OFFSET: u64 = 0x0000_801c_0000;
const AMP_OFFSET: u64 = 0x0000_801c_0180;

/// Fysieke ingangen (8 analoog, 2 S/PDIF, 8 ADAT), streams vanaf de computer
/// en fysieke uitgangen: alle drie achttien.
pub const KANALEN: usize = 18;

/// Elke bestemming van de mixer heeft een blok van 2 × 18 quadlets: eerst de
/// fysieke ingangen, dan de streams.
const MIXER_BLOK: usize = 2 * KANALEN;

/// 0 dB in de mixer en bij de uitgangsvolumes; het bereik loopt lineair van
/// 0 (stil) tot 0x10000 (+6 dB).
pub const NUL_DB: f64 = 0x8000 as f64;
pub const MAX_WAARDE: u32 = 0x10000;

// ------------------------------------------------------------------ vinden

pub struct Gevonden {
    pub fw: PathBuf,
    pub hwdep: Option<PathBuf>,
}

/// Zoekt de Fireface 400 op de bus via sysfs: de node met RME als vendor,
/// waarvan de unit een ALSA-kaart met id `Fireface400` heeft.
pub fn zoek() -> Option<Gevonden> {
    let basis = PathBuf::from("/sys/bus/firewire/devices");
    for entry in fs::read_dir(&basis).ok()?.flatten() {
        let naam = entry.file_name().to_string_lossy().into_owned();
        if !naam.starts_with("fw") || naam.contains('.') {
            continue;
        }
        let vendor = fs::read_to_string(entry.path().join("vendor")).unwrap_or_default();
        if vendor.trim() != "0x000a35" {
            continue;
        }
        let geluid = basis.join(format!("{naam}.0")).join("sound");
        let mut hwdep = None;
        let mut is_ff400 = false;
        if let Ok(kaarten) = fs::read_dir(&geluid) {
            for kaart in kaarten.flatten() {
                let id = fs::read_to_string(kaart.path().join("id")).unwrap_or_default();
                if id.trim() == "Fireface400" {
                    is_ff400 = true;
                    let k = kaart.file_name().to_string_lossy().into_owned();
                    if let Some(nr) = k.strip_prefix("card") {
                        hwdep = Some(PathBuf::from(format!("/dev/snd/hwC{nr}D0")));
                    }
                }
            }
        }
        // Zonder snd-fireface geladen is er geen kaart om naar te kijken; ga
        // dan af op het model van de unit.
        if !is_ff400 {
            let model = fs::read_to_string(basis.join(format!("{naam}.0")).join("model"))
                .unwrap_or_default();
            is_ff400 = model.trim() == "0x101800";
        }
        if is_ff400 {
            return Some(Gevonden {
                fw: PathBuf::from(format!("/dev/{naam}")),
                hwdep,
            });
        }
    }
    None
}

// ------------------------------------------------------------------ mixer

/// Schrijft de bronnen voor één bestemming (0..18). `fysiek` en `stream`
/// zijn ruwe waarden (0..=0x10000).
pub fn schrijf_mixer(
    node: &mut Node,
    bestemming: usize,
    fysiek: &[u32; KANALEN],
    stream: &[u32; KANALEN],
) -> std::io::Result<()> {
    let mut blok = [0u32; MIXER_BLOK];
    blok[..KANALEN].copy_from_slice(fysiek);
    blok[KANALEN..].copy_from_slice(stream);
    let offset = MIXER_OFFSET + (bestemming * MIXER_BLOK * 4) as u64;
    node.schrijf_blok(offset, &blok)
}

// ------------------------------------------------------------------ uitgangen

const OUTPUT_AMP_MAX: u32 = 0x3f;
const AMP_UIT_KANAAL: u32 = 4;

fn amp_commando(node: &mut Node, kanaal: u32, niveau: u8) -> std::io::Result<()> {
    node.schrijf_quadlet(AMP_OFFSET, (kanaal << 16) | niveau as u32)
}

/// Het volume van één fysieke uitgang, ruw (0..=0x10000). Het register zelf
/// en de analoge versterker krijgen elk hun eigen schrijfactie, net als bij
/// snd-firewire-ctl-services.
pub fn schrijf_uitgang(node: &mut Node, kanaal: usize, waarde: u32) -> std::io::Result<()> {
    let waarde = waarde.min(MAX_WAARDE);
    node.schrijf_blok(OUTPUT_OFFSET + (kanaal * 4) as u64, &[waarde])?;
    // 0x00 is +6 dB, 0x3f is stil.
    let amp = (OUTPUT_AMP_MAX * (MAX_WAARDE - waarde) / MAX_WAARDE) as u8;
    amp_commando(node, AMP_UIT_KANAAL + kanaal as u32, amp)
}

/// Omgekeerd: wat de draaiknop op de voorkant meldt (0..=0x3f) als ruwe
/// volumewaarde.
pub fn amp_naar_waarde(amp: u32) -> u32 {
    MAX_WAARDE * (OUTPUT_AMP_MAX - amp.min(OUTPUT_AMP_MAX)) / OUTPUT_AMP_MAX
}

// ------------------------------------------------------------------ voorversterkers

/// Gain van ingang 1..4: kanaal 0 en 1 zijn de mic-ingangen (0..=65 dB per
/// stap van 1), 2 en 3 de line/instrument-ingangen (0..=36, stappen van 0,5 dB).
pub fn schrijf_ingangsgain(node: &mut Node, kanaal: u32, niveau: u8) -> std::io::Result<()> {
    amp_commando(node, kanaal, niveau)
}

pub const MIC_MAX: u8 = 65;
pub const LINE_MAX: u8 = 36;

// ------------------------------------------------------------------ configuratie

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Niveau {
    /// Ingangen: "Lo Gain". Uitgangen: "Hi Gain".
    #[default]
    Hoog,
    /// -10 dBV
    Consumer,
    /// +4 dBu
    Pro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Klokbron {
    #[default]
    Intern,
    Adat,
    Spdif,
    WordClock,
    Ltc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OptischUit {
    #[default]
    Adat,
    Spdif,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SpdifIn {
    #[default]
    Coaxiaal,
    Optisch,
}

/// Alles wat in de drie configuratiequadlets staat en de gebruiker mag kiezen.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Configuratie {
    pub fantoom: [bool; 2],
    pub instrument: [bool; 2],
    pub pad: [bool; 2],
    pub line_in: Niveau,
    pub line_uit: Niveau,
    pub koptelefoon: Niveau,
    pub klokbron: Klokbron,
    pub spdif_in: SpdifIn,
    pub optisch_uit: OptischUit,
    pub spdif_pro: bool,
    pub spdif_emphasis: bool,
    pub word_single_speed: bool,
}

impl Default for Configuratie {
    fn default() -> Self {
        Self {
            fantoom: [false; 2],
            instrument: [false; 2],
            pad: [false; 2],
            line_in: Niveau::Hoog,
            line_uit: Niveau::Hoog,
            koptelefoon: Niveau::Hoog,
            klokbron: Klokbron::Intern,
            spdif_in: SpdifIn::Coaxiaal,
            optisch_uit: OptischUit::Adat,
            spdif_pro: false,
            spdif_emphasis: false,
            word_single_speed: false,
        }
    }
}

impl Configuratie {
    /// De drie quadlets, bitje voor bitje zoals `Ff400Config` ze serialiseert.
    /// Wat de gebruiker niet kiest staat op de standaardwaarden van
    /// snd-firewire-ctl-services: MIDI aan op lage offset 0x0000, alle
    /// samplerates beschikbaar en doorgaan bij synchronisatiefouten.
    pub fn quadlets(&self) -> [u32; 3] {
        let mut q = [0u32; 3];

        match self.line_in {
            Niveau::Hoog => q[0] |= 0x0000_0010,
            Niveau::Consumer => {
                q[0] |= 0x0000_0020;
                q[1] |= 0x0000_0003;
            }
            Niveau::Pro => {
                q[0] |= 0x0000_0008;
                q[1] |= 0x0000_0002;
            }
        }
        if self.fantoom[0] {
            q[0] |= 0x0000_0001;
        }
        if self.fantoom[1] {
            q[0] |= 0x0000_0080;
        }
        if self.instrument[0] {
            q[0] |= 0x0000_0200;
        }
        if self.instrument[1] {
            q[0] |= 0x0000_0004;
        }
        if self.pad[0] {
            q[0] |= 0x0000_0100;
        }
        if self.pad[1] {
            q[0] |= 0x0000_0002;
        }

        match self.line_uit {
            Niveau::Hoog => {
                q[0] |= 0x0000_0400;
                q[1] |= 0x0000_0010;
            }
            Niveau::Consumer => {
                q[0] |= 0x0000_1000;
                q[1] |= 0x0000_0008;
            }
            Niveau::Pro => {
                q[0] |= 0x0000_0800;
                q[1] |= 0x0000_0018;
            }
        }
        q[0] |= match self.koptelefoon {
            Niveau::Hoog => 0x0004_0000,
            Niveau::Consumer => 0x0002_0000,
            Niveau::Pro => 0x0000_0000,
        };

        // MIDI-berichten naar lage offset 0x0000, niet onderdrukt.
        q[2] |= 0x0400_0000;
        q[2] |= match self.klokbron {
            Klokbron::Intern => 0x0000_0001,
            Klokbron::Ltc => 0x0000_1400,
            Klokbron::WordClock => 0x0000_1000,
            Klokbron::Spdif => 0x0000_0c00,
            Klokbron::Adat => 0x0000_0000,
        };
        // 44,1 en 48 kHz, dubbel en viervoudig: allemaal toegestaan.
        q[2] |= 0x0000_0002 | 0x0000_0004 | 0x0000_0008 | 0x0000_0010;
        if self.spdif_in == SpdifIn::Optisch {
            q[2] |= 0x0000_0200;
        }
        if self.optisch_uit == OptischUit::Spdif {
            q[2] |= 0x0000_0100;
        }
        if self.spdif_pro {
            q[2] |= 0x0000_0020;
        }
        if self.spdif_emphasis {
            q[2] |= 0x0000_0040;
        }
        if self.word_single_speed {
            q[2] |= 0x0000_2000;
        }
        q[2] |= 0x8000_0000; // doorgaan bij fouten
        q
    }

    /// Wat ook in het statusregister staat, neemt de app over van het
    /// apparaat: dat is de enige configuratie die terug te lezen is.
    pub fn neem_over(&mut self, s: &Status) {
        self.klokbron = s.ingestelde_klok;
        self.spdif_in = if s.spdif_in_optisch {
            SpdifIn::Optisch
        } else {
            SpdifIn::Coaxiaal
        };
        self.optisch_uit = if s.optisch_uit_spdif {
            OptischUit::Spdif
        } else {
            OptischUit::Adat
        };
        self.spdif_pro = s.spdif_pro;
        self.spdif_emphasis = s.spdif_emphasis;
        self.word_single_speed = s.word_single_speed;
    }
}

pub fn schrijf_configuratie(node: &mut Node, c: &Configuratie) -> std::io::Result<()> {
    node.schrijf_blok(CFG_OFFSET, &c.quadlets())
}

// ------------------------------------------------------------------ status

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Default)]
pub struct Status {
    pub samplerate: u32,
    pub actieve_klok: Klokbron,
    pub ingestelde_klok: Klokbron,
    pub externe_rate: Option<u32>,
    pub spdif_rate: Option<u32>,
    pub adat_lock: bool,
    pub adat_sync: bool,
    pub spdif_lock: bool,
    pub spdif_sync: bool,
    pub word_lock: bool,
    pub word_sync: bool,
    pub spdif_in_optisch: bool,
    pub optisch_uit_spdif: bool,
    pub spdif_pro: bool,
    pub spdif_emphasis: bool,
    pub word_single_speed: bool,
}

fn rate_q0(v: u32) -> Option<u32> {
    Some(match v {
        0x0200_0000 => 32000,
        0x0400_0000 => 44100,
        0x0600_0000 => 48000,
        0x0800_0000 => 64000,
        0x0a00_0000 => 88200,
        0x0e00_0000 => 96000,
        0x0c00_0000 => 128000,
        0x1000_0000 => 176400,
        0x1200_0000 => 192000,
        _ => return None,
    })
}

fn spdif_rate(v: u32) -> Option<u32> {
    Some(match v {
        0x0000_4000 => 32000,
        0x0000_8000 => 44100,
        0x0000_c000 => 48000,
        0x0001_0000 => 64000,
        0x0001_4000 => 88200,
        0x0001_8000 => 96000,
        0x0001_c000 => 128000,
        0x0002_0000 => 176400,
        0x0002_4000 => 192000,
        _ => return None,
    })
}

pub fn lees_status(node: &mut Node) -> std::io::Result<Status> {
    let q = node.lees_blok(STATUS_OFFSET, 2)?;
    Ok(decodeer_status(q[0], q[1]))
}

fn decodeer_status(q0: u32, q1: u32) -> Status {
    let actieve_klok = match q0 & 0x01c0_0000 {
        0x01c0_0000 => Klokbron::Intern,
        0x0140_0000 => Klokbron::Ltc,
        0x0100_0000 => Klokbron::WordClock,
        0x00c0_0000 => Klokbron::Spdif,
        _ => Klokbron::Adat,
    };
    let ingestelde_klok = match q1 & 0x0000_1c01 {
        0x0000_0001 => Klokbron::Intern,
        0x0000_1400 => Klokbron::Ltc,
        0x0000_1000 => Klokbron::WordClock,
        0x0000_0c00 => Klokbron::Spdif,
        _ => Klokbron::Adat,
    };
    let samplerate = match q1 & 0x0000_001e {
        0x0000_0002 => 32000,
        0x0000_0006 => 48000,
        0x0000_000a => 64000,
        0x0000_0008 => 88200,
        0x0000_000e => 96000,
        0x0000_0012 => 128000,
        0x0000_0010 => 176400,
        0x0000_0016 => 192000,
        _ => 44100,
    };
    Status {
        samplerate,
        actieve_klok,
        ingestelde_klok,
        externe_rate: rate_q0(q0 & 0x1e00_0000),
        spdif_rate: spdif_rate(q0 & 0x0003_c000),
        adat_lock: q0 & 0x0000_1000 != 0,
        adat_sync: q0 & 0x0000_0400 != 0,
        spdif_lock: q0 & 0x0004_0000 != 0,
        spdif_sync: q0 & 0x0010_0000 != 0,
        word_lock: q0 & 0x2000_0000 != 0,
        word_sync: q0 & 0x4000_0000 != 0,
        spdif_in_optisch: q1 & 0x0000_0200 != 0,
        optisch_uit_spdif: q1 & 0x0000_0100 != 0,
        spdif_pro: q1 & 0x0000_0020 != 0,
        spdif_emphasis: q1 & 0x0000_0040 != 0,
        word_single_speed: q1 & 0x0000_2000 != 0,
    }
}

// ------------------------------------------------------------------ meters

/// Niveaus als fractie van full scale (0..=1), lineair.
#[derive(Debug, Clone, Serialize)]
pub struct Meters {
    pub ingangen: Vec<f32>,
    pub playback: Vec<f32>,
    pub uitgangen: Vec<f32>,
}

const METER_MASKER: u32 = 0x7fff_ff00;

pub fn lees_meters(node: &mut Node) -> std::io::Result<Meters> {
    // Drie gebieden; het laatste heeft één quadlet per fysieke ingang, stream
    // en fysieke uitgang. Wat de eerste twee betekenen is niet helemaal
    // duidelijk, dus die slaan we over.
    let voor = 2 * (KANALEN + 2 * KANALEN);
    let q = node.lees_blok(METER_OFFSET, voor + 3 * KANALEN)?;
    let niveau = |v: u32| (v & METER_MASKER) as f32 / METER_MASKER as f32;
    let deel = |i: usize| q[voor + i * KANALEN..voor + (i + 1) * KANALEN]
        .iter()
        .map(|&v| niveau(v))
        .collect();
    Ok(Meters {
        ingangen: deel(0),
        playback: deel(1),
        uitgangen: deel(2),
    })
}

// ------------------------------------------------------------------ draaiknop

/// Wat de draaiknop op de voorkant meldt. Het apparaat doet er zelf niets
/// mee: de computer moet de nieuwe waarde terugschrijven.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Knop {
    /// Ingang 0..4 (mic 1/2, line 3/4) met het ruwe niveau.
    Gain { kanaal: usize, niveau: u8 },
    /// Fysieke uitgang 0..18 met de ruwe volumewaarde.
    Volume { kanaal: usize, waarde: u32 },
}

pub fn ontleed_knop(bericht: u32) -> Vec<Knop> {
    if bericht & 0x0400_0000 == 0 {
        return Vec::new(); // MIDI, niet voor ons
    }
    let doel = (bericht & 0xf000_0000) >> 28;
    let niveau = (bericht & 0x00ff_fc00) >> 10;
    let paar = bericht & 0x0200_0000 != 0;
    let rechts = bericht & 0x0800_0000 != 0;
    let kanalen = |basis: usize| -> Vec<usize> {
        if paar {
            vec![basis, basis + 1]
        } else if rechts {
            vec![basis + 1]
        } else {
            vec![basis]
        }
    };
    match doel {
        0 | 1 => kanalen(doel as usize * 2)
            .into_iter()
            .map(|kanaal| Knop::Gain {
                kanaal,
                niveau: niveau as u8,
            })
            .collect(),
        2..=10 => kanalen((doel as usize - 2) * 2)
            .into_iter()
            .map(|kanaal| Knop::Volume {
                kanaal,
                waarde: amp_naar_waarde(niveau),
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn standaardconfiguratie() {
        let q = Configuratie::default().quadlets();
        assert_eq!(q[0], 0x0004_0410);
        assert_eq!(q[1], 0x0000_0010);
        assert_eq!(q[2], 0x8400_001f);
    }

    #[test]
    fn knop_voor_koptelefoon_stereo() {
        // doel 5 (koptelefoon), gekoppeld, niveau 0 (= +6 dB)
        let msg = 0x5000_0000 | 0x0400_0000 | 0x0200_0000;
        assert_eq!(
            ontleed_knop(msg),
            vec![
                Knop::Volume { kanaal: 6, waarde: MAX_WAARDE },
                Knop::Volume { kanaal: 7, waarde: MAX_WAARDE }
            ]
        );
    }

    #[test]
    fn knop_voor_mic_rechts() {
        let msg = 0x0400_0000 | 0x0800_0000 | (40 << 10);
        assert_eq!(ontleed_knop(msg), vec![Knop::Gain { kanaal: 1, niveau: 40 }]);
    }
}
