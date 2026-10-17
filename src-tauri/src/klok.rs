//! Samplerate en buffergrootte: de rest van wat TotalMix onder "Settings" heeft.
//!
//! Onder Linux bepaalt niet het apparaat maar PipeWire die twee: wie de
//! Fireface opent (PipeWire dus) kiest de rate, en de periode van de
//! ALSA-buffer volgt het quantum. Zetten gaat daarom via PipeWire, langs twee
//! wegen tegelijk:
//!
//! - `clock.force-rate` en `clock.force-quantum` in de settings-metadata, zodat
//!   het meteen gebeurt (PipeWire opent het apparaat dan opnieuw);
//! - een drop-in in `~/.config/pipewire/pipewire.conf.d/`, zodat het na een
//!   herstart van PipeWire of na het inloggen nog zo is, ook zonder deze app.
//!
//! Alleen enkele snelheid (32 / 44,1 / 48 kHz). Bij 88,2 en 96 kHz heeft de
//! FF400 14 kanalen in plaats van 18, en het pro-audio-profiel van PipeWire
//! leest het aantal kanalen eenmalig uit bij 48 kHz.

use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};

pub const RATES: [u32; 3] = [32000, 44100, 48000];
pub const BUFFERS: [u32; 7] = [32, 64, 128, 256, 512, 1024, 2048];

/// Wat de gebruiker koos. `None` is "Auto": PipeWire beslist zelf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Klok {
    pub rate: Option<u32>,
    pub buffer: Option<u32>,
}

/// Wat PipeWire nu doet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub struct KlokNu {
    pub rate: u32,
    pub quantum: u32,
    pub force_rate: u32,
    pub force_quantum: u32,
}

impl Klok {
    pub fn geldig(&self) -> Result<(), String> {
        if let Some(r) = self.rate.filter(|r| !RATES.contains(r)) {
            return Err(format!("Sample rate {r} is not supported"));
        }
        if let Some(b) = self.buffer.filter(|b| !BUFFERS.contains(b)) {
            return Err(format!("Buffer size {b} is not supported"));
        }
        Ok(())
    }
}

fn metadata(sleutel: &str, waarde: u32) -> Result<(), String> {
    let uit = Command::new("pw-metadata")
        .args(["-n", "settings", "0", sleutel, &waarde.to_string()])
        .output()
        .map_err(|e| format!("Cannot run pw-metadata: {e}"))?;
    if uit.status.success() {
        Ok(())
    } else {
        Err(format!(
            "pw-metadata failed: {}",
            String::from_utf8_lossy(&uit.stderr).trim()
        ))
    }
}

pub fn lees() -> Result<KlokNu, String> {
    let uit = Command::new("pw-metadata")
        .args(["-n", "settings", "0"])
        .output()
        .map_err(|e| format!("Cannot run pw-metadata: {e}"))?;
    Ok(ontleed(&String::from_utf8_lossy(&uit.stdout)))
}

/// Regels als `update: id:0 key:'clock.rate' value:'48000' type:''`.
fn ontleed(tekst: &str) -> KlokNu {
    let mut nu = KlokNu::default();
    for regel in tekst.lines() {
        let tussen = |na: &str| -> Option<&str> {
            let begin = regel.find(na)? + na.len();
            let eind = regel[begin..].find('\'')?;
            Some(&regel[begin..begin + eind])
        };
        let (Some(sleutel), Some(waarde)) = (tussen("key:'"), tussen("value:'")) else {
            continue;
        };
        let Ok(getal) = waarde.trim().parse::<u32>() else {
            continue;
        };
        match sleutel {
            "clock.rate" => nu.rate = getal,
            "clock.quantum" => nu.quantum = getal,
            "clock.force-rate" => nu.force_rate = getal,
            "clock.force-quantum" => nu.force_quantum = getal,
            _ => {}
        }
    }
    nu
}

fn dropin() -> Option<PathBuf> {
    let basis = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(basis.join("pipewire/pipewire.conf.d/50-cachymix.conf"))
}

fn dropin_tekst(k: &Klok) -> Option<String> {
    if k.rate.is_none() && k.buffer.is_none() {
        return None;
    }
    let mut regels = Vec::new();
    if let Some(r) = k.rate {
        regels.push(format!("    default.clock.rate          = {r}"));
        regels.push(format!("    default.clock.allowed-rates = [ {r} ]"));
    }
    if let Some(b) = k.buffer {
        // min = max: geen app kan het quantum dan nog groter of kleiner vragen.
        regels.push(format!("    default.clock.quantum       = {b}"));
        regels.push(format!("    default.clock.min-quantum   = {b}"));
        regels.push(format!("    default.clock.max-quantum   = {b}"));
    }
    Some(format!(
        "# Geschreven door CachyMix (Settings > Sample rate & buffer).\n\
         # Zet daar beide op Auto en dit bestand verdwijnt weer.\n\
         context.properties = {{\n{}\n}}\n",
        regels.join("\n")
    ))
}

/// Past de keuze toe, nu en voor de volgende keer.
pub fn zet(k: &Klok) -> Result<(), String> {
    k.geldig()?;
    // Eerst het quantum, dan de rate. Andersom komt het quantum binnen terwijl
    // PipeWire de Fireface voor de nieuwe rate nog opnieuw opent, en dan
    // blijft de ALSA-periode op de oude waarde staan (zo gemeten).
    let nu = lees().unwrap_or_default();
    let quantum = k.buffer.unwrap_or(0);
    let rate = k.rate.unwrap_or(0);
    if nu.force_quantum != quantum {
        metadata("clock.force-quantum", quantum)?;
        if nu.force_rate != rate {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    if nu.force_rate != rate {
        metadata("clock.force-rate", rate)?;
    }

    let Some(pad) = dropin() else { return Ok(()) };
    match dropin_tekst(k) {
        Some(tekst) => {
            if let Some(map) = pad.parent() {
                std::fs::create_dir_all(map).map_err(|e| format!("{}: {e}", map.display()))?;
            }
            std::fs::write(&pad, tekst).map_err(|e| format!("{}: {e}", pad.display()))?;
        }
        None => {
            let _ = std::fs::remove_file(&pad);
        }
    }
    Ok(())
}

/// Bij het starten van de app: alleen de metadata, en alleen als er iets
/// gekozen is. Zo blijft "Auto" echt van PipeWire.
pub fn herstel(k: &Klok) {
    if k.rate.is_some() || k.buffer.is_some() {
        let _ = zet(k);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn leest_de_metadata() {
        let t = "Found \"settings\" metadata 31\n\
                 update: id:0 key:'clock.rate' value:'48000' type:''\n\
                 update: id:0 key:'clock.allowed-rates' value:'[ 48000 ]' type:''\n\
                 update: id:0 key:'clock.quantum' value:'1024' type:''\n\
                 update: id:0 key:'clock.force-quantum' value:'256' type:''\n\
                 update: id:0 key:'clock.force-rate' value:'0' type:''\n";
        assert_eq!(
            ontleed(t),
            KlokNu {
                rate: 48000,
                quantum: 1024,
                force_rate: 0,
                force_quantum: 256
            }
        );
    }

    #[test]
    fn auto_schrijft_geen_dropin() {
        assert_eq!(dropin_tekst(&Klok::default()), None);
        let t = dropin_tekst(&Klok {
            rate: Some(44100),
            buffer: None,
        })
        .unwrap();
        assert!(t.contains("default.clock.rate          = 44100"));
        assert!(!t.contains("quantum"));
    }

    #[test]
    fn weigert_dubbele_snelheid() {
        assert!(Klok {
            rate: Some(96000),
            buffer: None
        }
        .geldig()
        .is_err());
    }
}
