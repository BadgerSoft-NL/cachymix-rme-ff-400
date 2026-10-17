import { useCallback, useEffect, useState } from "react";
import { api, Configuratie, KANALEN, luister, Meters, PAREN, Toestand, Verbinding } from "./api";
import { Instellingen } from "./Instellingen";
import { PAARNAMEN, stuurMeters } from "./kanalen";
import { BronStrip, UitgangStrip, Voorversterker } from "./strips";
import { version } from "../package.json";

const KLOKNAAM: Record<string, string> = {
  Intern: "Internal",
  Adat: "ADAT",
  Spdif: "SPDIF",
  WordClock: "Word clock",
  Ltc: "LTC",
};

function leesGekozen(): number {
  try {
    const v = Number(localStorage.getItem("gekozenPaar"));
    return Number.isInteger(v) && v >= 0 && v < PAREN ? v : 0;
  } catch {
    return 0;
  }
}

export default function App() {
  const [toestand, setToestand] = useState<Toestand | null>(null);
  const [verbinding, setVerbinding] = useState<Verbinding>({ verbonden: false, status: null, melding: null });
  const [paar, setPaar] = useState(leesGekozen);
  const [instellingen, setInstellingen] = useState(false);

  useEffect(() => {
    api.toestand().then(setToestand);
    api.verbinding().then(setVerbinding);
    const los = [
      luister<Toestand>("toestand", setToestand),
      luister<Verbinding>("verbinding", setVerbinding),
      luister<Meters>("meters", stuurMeters),
    ];
    return () => los.forEach((f) => f());
  }, []);

  const kies = (p: number) => {
    setPaar(p);
    try {
      localStorage.setItem("gekozenPaar", String(p));
    } catch {
      /* zonder opslag onthoudt hij het gewoon niet */
    }
  };

  // Faders sturen tientallen wijzigingen per seconde: die passen we hier
  // meteen toe in plaats van op een antwoord met de hele toestand te wachten.
  const route = useCallback(
    (bron: number, w: { db?: number; pan?: number; mute?: boolean }) => {
      setToestand((t) => {
        if (!t) return t;
        const routes = t.routes.slice();
        const rij = routes[paar].slice();
        rij[bron] = { ...rij[bron], ...w };
        routes[paar] = rij;
        return { ...t, routes };
      });
      api.route(paar, bron, w);
    },
    [paar],
  );

  const uitgang = useCallback((p: number, w: { db?: number; mute?: boolean }) => {
    setToestand((t) => {
      if (!t) return t;
      const uitgangen = t.uitgangen.slice();
      uitgangen[p] = { ...uitgangen[p], ...w };
      return { ...t, uitgangen };
    });
    api.uitgang(p, w);
  }, []);

  const gain = useCallback((kanaal: number, niveau: number) => {
    setToestand((t) => {
      if (!t) return t;
      const gains = [...t.gains] as Toestand["gains"];
      gains[kanaal] = niveau;
      return { ...t, gains };
    });
    api.gain(kanaal, niveau);
  }, []);

  const configuratie = useCallback((c: Configuratie) => {
    setToestand((t) => (t ? { ...t, configuratie: c } : t));
    api.configuratie(c).then(setToestand);
  }, []);

  if (!toestand) return <div className="laden">Loading…</div>;

  const soloActief = toestand.solo.some((s) => s);
  const s = verbinding.status;

  const rij = (vanaf: number) => {
    const strips = [];
    for (let k = 0; k < KANALEN; k += 2) {
      const bron = vanaf + k;
      const gekoppeld = toestand.bron_gekoppeld[bron >> 1];
      const leden = gekoppeld ? [bron] : [bron, bron + 1];
      for (const b of leden) {
        const voorversterkers =
          b < 4
            ? (gekoppeld ? [b, b + 1] : [b]).map((kanaal) => (
                <Voorversterker
                  key={kanaal}
                  kanaal={kanaal}
                  gain={toestand.gains[kanaal]}
                  configuratie={toestand.configuratie}
                  onGain={(v) => gain(kanaal, v)}
                  onConfiguratie={configuratie}
                />
              ))
            : null;
        strips.push(
          <BronStrip
            key={b}
            bron={b}
            gekoppeld={gekoppeld}
            kruispunt={toestand.routes[paar][b]}
            solo={toestand.solo[b]}
            soloActief={soloActief}
            naam={toestand.namen.bronnen[b]}
            onRoute={(w) => route(b, w)}
            onSolo={(aan) => api.solo(b, aan).then(setToestand)}
            onKoppel={(aan) => api.koppel(b >> 1, aan).then(setToestand)}
            onNaam={(n) => api.naam("bron", b, n).then(setToestand)}
            voorversterker={voorversterkers && <div className="vv-groep">{voorversterkers}</div>}
          />,
        );
      }
    }
    return strips;
  };

  const paarNaam = (p: number) => toestand.namen.uitgangen[p] || PAARNAMEN[p];

  return (
    <div className="app">
      <header className="balk">
        <div className="merk">
          CachyMix <span className="merk-versie">{version}</span> <span className="merk-sub">Fireface 400</span>
        </div>
        <div className={"verbinding" + (verbinding.verbonden ? " aan" : "")}>
          <span className="punt" />
          {verbinding.verbonden ? "Connected" : verbinding.melding ?? "Searching…"}
        </div>
        {s && verbinding.verbonden && (
          <div className="klokinfo">
            {(s.samplerate / 1000).toFixed(1)} kHz · {KLOKNAAM[s.actieve_klok]}
            {s.actieve_klok !== s.ingestelde_klok && (
              <span className="waarschuwing"> (set to {KLOKNAAM[s.ingestelde_klok]}, not locked)</span>
            )}
          </div>
        )}
        <div className="balk-rechts">
          <span className="mixnaam">
            Mixing to <b>{paarNaam(paar)}</b>
          </span>
          {soloActief && (
            <button className="knop-tekst solo-aan" onClick={() => api.wisSolo().then(setToestand)}>
              Clear solo
            </button>
          )}
          <button
            className="knop-tekst"
            onClick={() => {
              if (confirm(`Remove every source from the mix for ${paarNaam(paar)}?`))
                api.wisPaar(paar).then(setToestand);
            }}
          >
            Clear mix
          </button>
          <button className="knop-tekst" onClick={() => setInstellingen(true)}>
            Settings
          </button>
        </div>
      </header>

      <main className="rijen">
        <section className="rij">
          <div className="rij-titel">Hardware inputs</div>
          <div className="rij-strips">{rij(0)}</div>
        </section>
        <section className="rij">
          <div className="rij-titel">Software playback</div>
          <div className="rij-strips">{rij(KANALEN)}</div>
        </section>
        <section className="rij rij-uit">
          <div className="rij-titel">Hardware outputs</div>
          <div className="rij-strips">
            {Array.from({ length: PAREN }, (_, p) => (
              <UitgangStrip
                key={p}
                paar={p}
                uitgang={toestand.uitgangen[p]}
                gekozen={p === paar}
                naam={toestand.namen.uitgangen[p]}
                standaard={PAARNAMEN[p]}
                onKies={() => kies(p)}
                onUitgang={(w) => uitgang(p, w)}
                onNaam={(n) => api.naam("uitgang", p, n).then(setToestand)}
              />
            ))}
          </div>
        </section>
      </main>

      {instellingen && (
        <Instellingen
          configuratie={toestand.configuratie}
          klok={toestand.klok}
          verbinding={verbinding}
          onChange={configuratie}
          onKlok={(k) => api.zetKlok(k).then(setToestand)}
          onSluit={() => setInstellingen(false)}
          onBeginOpnieuw={() => api.beginOpnieuw().then(setToestand)}
        />
      )}
    </div>
  );
}
