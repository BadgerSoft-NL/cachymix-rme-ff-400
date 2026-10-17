// Namen en schalen: wat de gebruiker ziet bij elk kanaal, en hoe een
// faderstand een decibel wordt.

import { KANALEN, MAX_DB, Meters, UIT_DB } from "./api";

/** De fysieke kant, zoals hij op het frontpaneel en de achterkant staat. */
const FYSIEK = [
  "Mic 1", "Mic 2", "Inst 3", "Inst 4", "Line 5", "Line 6", "Line 7", "Line 8",
  "SPDIF L", "SPDIF R",
  "ADAT 1", "ADAT 2", "ADAT 3", "ADAT 4", "ADAT 5", "ADAT 6", "ADAT 7", "ADAT 8",
];

const UIT = [
  "Out 1", "Out 2", "Out 3", "Out 4", "Out 5", "Out 6", "Phones L", "Phones R",
  "SPDIF L", "SPDIF R",
  "ADAT 1", "ADAT 2", "ADAT 3", "ADAT 4", "ADAT 5", "ADAT 6", "ADAT 7", "ADAT 8",
];

export const PAARNAMEN = [
  "Out 1/2", "Out 3/4", "Out 5/6", "Phones", "SPDIF", "ADAT 1/2", "ADAT 3/4", "ADAT 5/6", "ADAT 7/8",
];

/** Standaardnaam van bron 0..36 (de tweede helft is playback). */
export function bronNaam(bron: number): string {
  return bron < KANALEN ? FYSIEK[bron] : `PB ${bron - KANALEN + 1}`;
}

/** Standaardnaam van een gekoppeld bronpaar. */
export function bronPaarNaam(bron: number): string {
  const l = bron & ~1;
  if (l < KANALEN) {
    const a = FYSIEK[l];
    const b = FYSIEK[l + 1];
    if (a.startsWith("SPDIF")) return "SPDIF";
    const soort = a.split(" ")[0];
    return b.startsWith(soort) ? `${soort} ${a.split(" ")[1]}/${b.split(" ")[1]}` : `${a} / ${b}`;
  }
  const n = l - KANALEN + 1;
  return `PB ${n}/${n + 1}`;
}

export function uitgangKanaalNaam(kanaal: number): string {
  return UIT[kanaal];
}

// ------------------------------------------------------------------ fader

// Steunpunten van de faderwet: stand (0..1) tegen dB. Bovenin fijn rond 0 dB,
// onderin grof, ongeveer zoals een echte mengtafel.
const WET: [number, number][] = [
  [0, UIT_DB],
  [0.04, -60],
  [0.2, -40],
  [0.42, -20],
  [0.62, -10],
  [0.76, -4],
  [0.86, 0],
  [1, MAX_DB],
];

export function standNaarDb(stand: number): number {
  const s = Math.min(1, Math.max(0, stand));
  if (s <= 0.005) return UIT_DB;
  for (let i = 1; i < WET.length; i++) {
    const [s1, d1] = WET[i];
    if (s <= s1) {
      const [s0, d0] = WET[i - 1];
      return d0 + ((s - s0) / (s1 - s0)) * (d1 - d0);
    }
  }
  return MAX_DB;
}

export function dbNaarStand(db: number): number {
  if (db <= UIT_DB) return 0;
  for (let i = 1; i < WET.length; i++) {
    const [s1, d1] = WET[i];
    if (db <= d1) {
      const [s0, d0] = WET[i - 1];
      return s0 + ((db - d0) / (d1 - d0)) * (s1 - s0);
    }
  }
  return 1;
}

export function toonDb(db: number): string {
  if (db <= UIT_DB) return "−∞";
  const r = Math.round(db * 10) / 10;
  return (r > 0 ? "+" : r < 0 ? "−" : "") + Math.abs(r).toFixed(1);
}

export function toonPan(pan: number): string {
  const p = Math.round(pan * 100);
  if (p === 0) return "C";
  return p < 0 ? `L${-p}` : `R${p}`;
}

// ------------------------------------------------------------------ meters

export type MeterSoort = "ingangen" | "playback" | "uitgangen";

/**
 * De meters komen 25 keer per seconde binnen. Door React heen zou dat elke
 * strip laten hertekenen; in plaats daarvan schrijft elke meter zichzelf weg
 * vanuit deze bus.
 */
const meterLuisteraars = new Set<(m: Meters) => void>();
export let laatsteMeters: Meters | null = null;

export function stuurMeters(m: Meters) {
  laatsteMeters = m;
  meterLuisteraars.forEach((f) => f(m));
}

export function opMeters(f: (m: Meters) => void): () => void {
  meterLuisteraars.add(f);
  return () => meterLuisteraars.delete(f);
}

/** Lineair (0..1) naar een stand op de meter van −60 tot 0 dBFS. */
export function meterStand(v: number): number {
  if (v <= 0) return 0;
  const db = 20 * Math.log10(v);
  return Math.min(1, Math.max(0, (db + 60) / 60));
}
