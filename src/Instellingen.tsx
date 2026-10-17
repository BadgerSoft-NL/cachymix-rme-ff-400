// Het paneel met alles wat geen fader is: klok, niveaus en digitale I/O.

import { useEffect, useState } from "react";
import { api, BUFFERS, Configuratie, Klok, Klokbron, KlokNu, luister, Niveau, RATES, Verbinding } from "./api";

function Keuze<T extends string>({
  label,
  waarde,
  opties,
  onChange,
}: {
  label: string;
  waarde: T;
  opties: [T, string][];
  onChange: (v: T) => void;
}) {
  return (
    <div className="keuze">
      <span className="keuze-label">{label}</span>
      <div className="keuze-opties">
        {opties.map(([v, tekst]) => (
          <button key={v} className={"optie" + (v === waarde ? " aan" : "")} onClick={() => onChange(v)}>
            {tekst}
          </button>
        ))}
      </div>
    </div>
  );
}

function Vink({ label, aan, onChange }: { label: string; aan: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="vink">
      <input type="checkbox" checked={aan} onChange={(e) => onChange(e.currentTarget.checked)} />
      {label}
    </label>
  );
}

const INGANGSNIVEAU: [Niveau, string][] = [
  ["Hoog", "Lo Gain"],
  ["Pro", "+4 dBu"],
  ["Consumer", "−10 dBV"],
];

const UITGANGSNIVEAU: [Niveau, string][] = [
  ["Hoog", "Hi Gain"],
  ["Pro", "+4 dBu"],
  ["Consumer", "−10 dBV"],
];

const KLOK: [Klokbron, string][] = [
  ["Intern", "Internal"],
  ["Adat", "ADAT"],
  ["Spdif", "SPDIF"],
  ["WordClock", "Word"],
  ["Ltc", "LTC"],
];

function Lamp({ label, lock, sync }: { label: string; lock: boolean; sync: boolean }) {
  return (
    <div className="lamp">
      <span className={"lamp-punt" + (sync ? " sync" : lock ? " lock" : "")} />
      {label}
      <span className="lamp-tekst">{sync ? "sync" : lock ? "lock" : "no lock"}</span>
    </div>
  );
}

const toonRate = (r: number) => `${(r / 1000).toFixed(1).replace(/\.0$/, "")} kHz`;

/**
 * Samplerate en buffer. Die horen bij PipeWire, dus na het zetten lezen we
 * terug wat PipeWire er echt van maakte; het apparaat opnieuw openen duurt
 * even, vandaar de tweede keer kijken.
 */
function KlokKeuze({ klok, onKlok }: { klok: Klok; onKlok: (k: Klok) => Promise<void> }) {
  const [nu, setNu] = useState<KlokNu | null>(null);
  const [fout, setFout] = useState<string | null>(null);
  const [bezig, setBezig] = useState(false);

  const ververs = () => api.klokNu().then(setNu, (e) => setFout(String(e)));
  useEffect(() => {
    ververs();
  }, []);

  const zet = async (k: Klok) => {
    setBezig(true);
    setFout(null);
    try {
      await onKlok(k);
    } catch (e) {
      setFout(String(e));
    }
    await ververs();
    setTimeout(() => {
      ververs();
      setBezig(false);
    }, 1200);
  };

  const rate = nu ? nu.force_rate || nu.rate : 0;
  const quantum = nu ? nu.force_quantum || nu.quantum : 0;

  return (
    <>
      <Keuze
        label="Sample rate"
        waarde={klok.rate === null ? "auto" : String(klok.rate)}
        opties={[["auto", "Auto"], ...RATES.map((r): [string, string] => [String(r), toonRate(r)])]}
        onChange={(v) => zet({ ...klok, rate: v === "auto" ? null : Number(v) })}
      />
      <Keuze
        label="Buffer size"
        waarde={klok.buffer === null ? "auto" : String(klok.buffer)}
        opties={[["auto", "Auto"], ...BUFFERS.map((b): [string, string] => [String(b), String(b)])]}
        onChange={(v) => zet({ ...klok, buffer: v === "auto" ? null : Number(v) })}
      />
      <p className="uitleg">
        {nu && rate > 0 && quantum > 0 ? (
          <>
            Now running: <b>{toonRate(rate)}</b>, {quantum} samples ={" "}
            <b>{((quantum / rate) * 1000).toFixed(1)} ms</b> per buffer
            {bezig && " (applying…)"}
          </>
        ) : (
          "Reading PipeWire…"
        )}
        <br />
        Set through PipeWire, which reopens the Fireface (a short dropout). Kept after a restart via
        ~/.config/pipewire/pipewire.conf.d/50-cachymix.conf; Auto on both removes that file. 88.2/96 kHz is not
        available yet: the FF400 has fewer channels there.
      </p>
      {fout && <p className="fout">{fout}</p>}
    </>
  );
}

function AutostartKeuze() {
  const [aan, setAan] = useState(false);
  const [fout, setFout] = useState<string | null>(null);
  useEffect(() => {
    api.autostart().then(setAan);
    return luister<boolean>("autostart", setAan);
  }, []);
  return (
    <>
      <Vink
        label="Start at login, hidden in the system tray"
        aan={aan}
        onChange={(v) => {
          setFout(null);
          api.zetAutostart(v).then(setAan, (e) => setFout(String(e)));
        }}
      />
      {fout && <p className="fout">{fout}</p>}
    </>
  );
}

export function Instellingen({
  configuratie,
  klok,
  verbinding,
  onChange,
  onKlok,
  onSluit,
  onBeginOpnieuw,
}: {
  configuratie: Configuratie;
  klok: Klok;
  verbinding: Verbinding;
  onChange: (c: Configuratie) => void;
  onKlok: (k: Klok) => Promise<void>;
  onSluit: () => void;
  onBeginOpnieuw: () => void;
}) {
  const c = configuratie;
  const zet = <K extends keyof Configuratie>(k: K, v: Configuratie[K]) => onChange({ ...c, [k]: v });
  const s = verbinding.status;

  return (
    <div className="laag" onPointerDown={(e) => e.target === e.currentTarget && onSluit()}>
      <div className="paneel">
        <div className="paneel-kop">
          <h2>Settings</h2>
          <button className="sluit" onClick={onSluit}>
            ✕
          </button>
        </div>

        <section>
          <h3>Sample rate &amp; buffer</h3>
          <KlokKeuze klok={klok} onKlok={onKlok} />
        </section>

        <section>
          <h3>Clock</h3>
          <Keuze label="Clock source" waarde={c.klokbron} opties={KLOK} onChange={(v) => zet("klokbron", v)} />
          {s && (
            <div className="lampen">
              <Lamp label="ADAT" lock={s.adat_lock} sync={s.adat_sync} />
              <Lamp label="SPDIF" lock={s.spdif_lock} sync={s.spdif_sync} />
              <Lamp label="Word" lock={s.word_lock} sync={s.word_sync} />
            </div>
          )}
        </section>

        <section>
          <h3>Analog levels</h3>
          <Keuze label="Inputs 5–8" waarde={c.line_in} opties={INGANGSNIVEAU} onChange={(v) => zet("line_in", v)} />
          <Keuze label="Outputs 1–6" waarde={c.line_uit} opties={UITGANGSNIVEAU} onChange={(v) => zet("line_uit", v)} />
          <Keuze label="Phones" waarde={c.koptelefoon} opties={UITGANGSNIVEAU} onChange={(v) => zet("koptelefoon", v)} />
        </section>

        <section>
          <h3>Digital I/O</h3>
          <Keuze
            label="SPDIF input"
            waarde={c.spdif_in}
            opties={[
              ["Coaxiaal", "Coaxial"],
              ["Optisch", "Optical"],
            ]}
            onChange={(v) => zet("spdif_in", v)}
          />
          <Keuze
            label="Optical output"
            waarde={c.optisch_uit}
            opties={[
              ["Adat", "ADAT"],
              ["Spdif", "SPDIF"],
            ]}
            onChange={(v) => zet("optisch_uit", v)}
          />
          <div className="vinken">
            <Vink label="SPDIF professional format" aan={c.spdif_pro} onChange={(v) => zet("spdif_pro", v)} />
            <Vink label="SPDIF emphasis" aan={c.spdif_emphasis} onChange={(v) => zet("spdif_emphasis", v)} />
            <Vink
              label="Word clock out at single speed"
              aan={c.word_single_speed}
              onChange={(v) => zet("word_single_speed", v)}
            />
          </div>
        </section>

        <section>
          <h3>App</h3>
          <AutostartKeuze />
          <p className="uitleg">
            Closing the window keeps CachyMix in the system tray (Status and Notifications), so the front-panel knob
            keeps working. Quit from the tray menu.
          </p>
        </section>

        <section>
          <h3>Mixer</h3>
          <button
            className="gevaar"
            onClick={() => {
              if (confirm("Reset all mixes, outputs and gains to the default routing (playback N → output N)?"))
                onBeginOpnieuw();
            }}
          >
            Reset mixer to defaults
          </button>
          <p className="uitleg">
            The Fireface cannot report its mixer back, so CachyMix remembers everything and sends it again whenever the
            device (re)appears.
          </p>
        </section>
      </div>
    </div>
  );
}
