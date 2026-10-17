# CachyMix

Mixer voor de RME Fireface 400 (FireWire) onder Linux, als Tauri-app:
React-schil, Rust-kant die zelf met het apparaat praat.

## Hoe hij met de Fireface praat

Geen ffado, geen snd-firewire-ctl-services: de kernel-driver `snd_fireface`
doet de audio, deze app doet de mixer via asynchrone transacties op
`/dev/fwN` (`firewire.rs`, ioctls uit `linux/firewire-cdev.h`). De gebruiker
moet in groep `audio` zitten.

- `ff400.rs` — adressen en bitjes, overgenomen uit
  `protocols/fireface/src/former{,/ff400}.rs` van snd-firewire-ctl-services.
- `mixer.rs` — het TotalMix-model (per uitgangspaar: fader/pan/mute per bron)
  en de vertaling naar registerwaarden. 0x8000 = 0 dB, lineair.
- `apparaat.rs` — de draad die schrijft wat vuil is, meters leest (25 Hz),
  de status volgt, opnieuw verbindt en de draaiknop leest via de hwdep-node.
- `klok.rs` — samplerate en buffer. Die zijn van PipeWire, niet van het
  apparaat: `clock.force-rate`/`force-quantum` via `pw-metadata` voor nu, plus
  een drop-in `~/.config/pipewire/pipewire.conf.d/50-cachymix.conf` voor na een
  herstart. Eerst quantum, dan rate, anders blijft de ALSA-periode hangen.
  Alleen 32/44,1/48 kHz: bij 88,2/96 heeft de FF400 14 kanalen, en het
  pro-audio-profiel leest er 18 uit bij 48 kHz.
- `tray.rs` — icoon in Status and Notifications via `ksni` (StatusNotifierItem
  over D-Bus), niet via de tray van Tauri: die gaat over libayatana en kent
  alleen een menu. Klik = venster tonen/verbergen, rechtsklik = menu.
  Venster sluiten = verbergen.
- `autostart.rs` — `~/.config/autostart/nl.jetse.cachymix.desktop` met
  `--hidden`. In een AppImage wijst hij naar `$APPIMAGE`, niet naar
  `current_exe`. Single-instance-plugin voorkomt twee schrijvers op de mixer.

Publiek op https://github.com/BadgerSoft-NL/cachymix-rme-ff-400 (GPL-3.0).
Release: `NO_STRIP=true npm run tauri build -- --bundles appimage`. Let op:
AppImageLauncher verplaatst een AppImage bij de eerste start naar
`~/Applications`.

**Mixer, volumes, gains en configuratie zijn alleen te schrijven.** De app is
dus de waarheid: alles staat in `~/.config/nl.jetse.cachymix/toestand.json` en
gaat bij elke verbinding opnieuw naar het apparaat.

De draaiknop op de voorkant doet zelf niets: snd-fireface geeft de berichten
door via `/dev/snd/hwCxD0`, en de app schrijft de nieuwe waarde terug.

## Twee talen

Wat op het scherm komt is Engels; identifiers, commentaar en commits Nederlands.

## Commando's

```bash
npm run dev                          # alleen de schil, met nagemaakt apparaat (api.ts)
npm run tauri dev                    # de hele app
npm run tauri build -- --no-bundle   # src-tauri/target/release/cachymix
cargo test                           # vanuit src-tauri/
cargo run --example peil             # status en meters lezen, schrijft niets
```
