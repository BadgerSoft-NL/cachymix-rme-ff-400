// De brug naar de Rust-kant. Zonder Tauri (npm run dev in een gewone browser)
// valt alles terug op een nagemaakt apparaat, zodat de schil te bekijken is
// zonder Fireface aan de kabel.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";

export const KANALEN = 18;
export const BRONNEN = 36;
export const PAREN = 9;
export const UIT_DB = -90;
export const MAX_DB = 6;
export const MIC_MAX = 65;
export const LINE_MAX = 36;

export type Niveau = "Hoog" | "Consumer" | "Pro";
export type Klokbron = "Intern" | "Adat" | "Spdif" | "WordClock" | "Ltc";

export interface Kruispunt {
  db: number;
  pan: number;
  mute: boolean;
}

export interface Uitgang {
  db: number;
  mute: boolean;
}

export interface Configuratie {
  fantoom: [boolean, boolean];
  instrument: [boolean, boolean];
  pad: [boolean, boolean];
  line_in: Niveau;
  line_uit: Niveau;
  koptelefoon: Niveau;
  klokbron: Klokbron;
  spdif_in: "Coaxiaal" | "Optisch";
  optisch_uit: "Adat" | "Spdif";
  spdif_pro: boolean;
  spdif_emphasis: boolean;
  word_single_speed: boolean;
}

/** `null` is Auto: PipeWire beslist zelf. */
export interface Klok {
  rate: number | null;
  buffer: number | null;
}

/** Wat PipeWire nu doet. */
export interface KlokNu {
  rate: number;
  quantum: number;
  force_rate: number;
  force_quantum: number;
}

export const RATES = [32000, 44100, 48000];
export const BUFFERS = [32, 64, 128, 256, 512, 1024, 2048];

export interface Toestand {
  routes: Kruispunt[][];
  bron_gekoppeld: boolean[];
  uitgangen: Uitgang[];
  solo: boolean[];
  gains: [number, number, number, number];
  configuratie: Configuratie;
  klok: Klok;
  namen: { bronnen: string[]; uitgangen: string[] };
}

export interface Status {
  samplerate: number;
  actieve_klok: Klokbron;
  ingestelde_klok: Klokbron;
  externe_rate: number | null;
  spdif_rate: number | null;
  adat_lock: boolean;
  adat_sync: boolean;
  spdif_lock: boolean;
  spdif_sync: boolean;
  word_lock: boolean;
  word_sync: boolean;
}

export interface Verbinding {
  verbonden: boolean;
  status: Status | null;
  melding: string | null;
}

export interface Meters {
  ingangen: number[];
  playback: number[];
  uitgangen: number[];
}

const inTauri = "__TAURI_INTERNALS__" in window;

// ------------------------------------------------------------------ nagemaakt

function standaardToestand(): Toestand {
  const routes: Kruispunt[][] = [];
  for (let p = 0; p < PAREN; p++) {
    const rij: Kruispunt[] = [];
    for (let b = 0; b < BRONNEN; b++) {
      const aan = b === KANALEN + 2 * p || (p === 3 && b === KANALEN);
      rij.push({ db: aan ? 0 : UIT_DB, pan: 0, mute: false });
    }
    routes.push(rij);
  }
  return {
    routes,
    bron_gekoppeld: Array(BRONNEN / 2).fill(true),
    uitgangen: Array.from({ length: PAREN }, (_, p) => ({ db: p < 3 ? -20 : p === 3 ? -12 : 0, mute: false })),
    solo: Array(BRONNEN).fill(false),
    gains: [20, 0, 0, 0],
    configuratie: {
      fantoom: [false, false],
      instrument: [false, false],
      pad: [false, false],
      line_in: "Hoog",
      line_uit: "Hoog",
      koptelefoon: "Hoog",
      klokbron: "Intern",
      spdif_in: "Coaxiaal",
      optisch_uit: "Adat",
      spdif_pro: false,
      spdif_emphasis: false,
      word_single_speed: false,
    },
    klok: { rate: null, buffer: null },
    namen: { bronnen: [], uitgangen: [] },
  };
}

const nep = {
  toestand: standaardToestand(),
  luisteraars: new Map<string, Set<(p: unknown) => void>>(),
};

function nepStuur(naam: string, p: unknown) {
  nep.luisteraars.get(naam)?.forEach((f) => f(p));
}

if (!inTauri) {
  let t = 0;
  setInterval(() => {
    t += 0.04;
    const golf = (i: number, schaal: number) =>
      Math.max(0, schaal * (0.5 + 0.5 * Math.sin(t * (1.3 + i * 0.17) + i)) * (0.6 + 0.4 * Math.random()));
    nepStuur("meters", {
      ingangen: Array.from({ length: KANALEN }, (_, i) => (i < 4 ? golf(i, 0.5) : i < 8 ? golf(i, 0.05) : 0)),
      playback: Array.from({ length: KANALEN }, (_, i) => (i < 2 ? golf(i, 0.7) : 0)),
      uitgangen: Array.from({ length: KANALEN }, (_, i) => (i < 2 || i === 6 || i === 7 ? golf(i, 0.6) : 0)),
    } satisfies Meters);
  }, 40);
}

const kloon = <T,>(v: T): T => JSON.parse(JSON.stringify(v));

async function nepInvoke<T>(cmd: string, a: Record<string, unknown>): Promise<T> {
  const s = nep.toestand;
  const leidend = (bron: number) => (s.bron_gekoppeld[bron >> 1] ? bron & ~1 : bron);
  switch (cmd) {
    case "haal_toestand":
      break;
    case "haal_verbinding":
      return {
        verbonden: true,
        melding: null,
        status: {
          samplerate: 48000,
          actieve_klok: "Intern",
          ingestelde_klok: "Intern",
          externe_rate: null,
          spdif_rate: null,
          adat_lock: false,
          adat_sync: false,
          spdif_lock: true,
          spdif_sync: false,
          word_lock: false,
          word_sync: false,
        },
      } as T;
    case "zet_route": {
      const k = s.routes[a.paar as number][leidend(a.bron as number)];
      if (a.db != null) k.db = a.db as number;
      if (a.pan != null) k.pan = a.pan as number;
      if (a.mute != null) k.mute = a.mute as boolean;
      return undefined as T;
    }
    case "zet_koppeling": {
      const bp = a.bronpaar as number;
      s.bron_gekoppeld[bp] = a.aan as boolean;
      for (const rij of s.routes) {
        if (a.aan) rij[2 * bp].pan = 0;
        else {
          rij[2 * bp + 1] = { ...rij[2 * bp], pan: 1 };
          rij[2 * bp].pan = -1;
        }
      }
      break;
    }
    case "zet_solo":
      s.solo[leidend(a.bron as number)] = a.aan as boolean;
      break;
    case "wis_solo":
      s.solo.fill(false);
      break;
    case "zet_uitgang": {
      const u = s.uitgangen[a.paar as number];
      if (a.db != null) u.db = a.db as number;
      if (a.mute != null) u.mute = a.mute as boolean;
      return undefined as T;
    }
    case "zet_gain":
      s.gains[a.kanaal as number] = a.niveau as number;
      return undefined as T;
    case "zet_configuratie":
      s.configuratie = a.configuratie as Configuratie;
      break;
    case "zet_naam": {
      const lijst = a.soort === "bron" ? s.namen.bronnen : s.namen.uitgangen;
      lijst[a.index as number] = a.naam as string;
      break;
    }
    case "wis_paar":
      for (const k of s.routes[a.paar as number]) {
        k.db = UIT_DB;
        k.mute = false;
      }
      break;
    case "haal_klok":
      return {
        rate: s.klok.rate ?? 48000,
        quantum: s.klok.buffer ?? 1024,
        force_rate: s.klok.rate ?? 0,
        force_quantum: s.klok.buffer ?? 0,
      } as T;
    case "haal_autostart":
      return false as T;
    case "zet_autostart":
      return a.aan as T;
    case "zet_klok":
      s.klok = a.keuze as Klok;
      break;
    case "begin_opnieuw": {
      const { configuratie, namen } = s;
      nep.toestand = { ...standaardToestand(), configuratie, namen };
      return kloon(nep.toestand) as T;
    }
  }
  return kloon(nep.toestand) as T;
}

// ------------------------------------------------------------------ naar buiten

export function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  return inTauri ? tauriInvoke<T>(cmd, args) : nepInvoke<T>(cmd, args);
}

export function luister<T>(naam: string, f: (p: T) => void): () => void {
  if (!inTauri) {
    const set = nep.luisteraars.get(naam) ?? new Set();
    set.add(f as (p: unknown) => void);
    nep.luisteraars.set(naam, set);
    return () => set.delete(f as (p: unknown) => void);
  }
  let los: (() => void) | null = null;
  let weg = false;
  tauriListen<T>(naam, (e) => f(e.payload)).then((u) => {
    if (weg) u();
    else los = u;
  });
  return () => {
    weg = true;
    los?.();
  };
}

export const api = {
  toestand: () => invoke<Toestand>("haal_toestand"),
  verbinding: () => invoke<Verbinding>("haal_verbinding"),
  route: (paar: number, bron: number, w: { db?: number; pan?: number; mute?: boolean }) =>
    invoke<void>("zet_route", { paar, bron, ...w }),
  koppel: (bronpaar: number, aan: boolean) => invoke<Toestand>("zet_koppeling", { bronpaar, aan }),
  solo: (bron: number, aan: boolean) => invoke<Toestand>("zet_solo", { bron, aan }),
  wisSolo: () => invoke<Toestand>("wis_solo"),
  uitgang: (paar: number, w: { db?: number; mute?: boolean }) => invoke<void>("zet_uitgang", { paar, ...w }),
  gain: (kanaal: number, niveau: number) => invoke<void>("zet_gain", { kanaal, niveau }),
  configuratie: (configuratie: Configuratie) => invoke<Toestand>("zet_configuratie", { configuratie }),
  naam: (soort: "bron" | "uitgang", index: number, naam: string) =>
    invoke<Toestand>("zet_naam", { soort, index, naam }),
  wisPaar: (paar: number) => invoke<Toestand>("wis_paar", { paar }),
  beginOpnieuw: () => invoke<Toestand>("begin_opnieuw"),
  klokNu: () => invoke<KlokNu>("haal_klok"),
  autostart: () => invoke<boolean>("haal_autostart"),
  zetAutostart: (aan: boolean) => invoke<boolean>("zet_autostart", { aan }),
  zetKlok: (keuze: Klok) => invoke<Toestand>("zet_klok", { keuze }),
};
