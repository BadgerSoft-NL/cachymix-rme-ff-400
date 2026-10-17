// De drie soorten strips: een bron (ingang of playback) in het gekozen
// uitgangspaar, en een uitgangspaar zelf.

import { Configuratie, Kruispunt, KANALEN, LINE_MAX, MIC_MAX, UIT_DB, Uitgang } from "./api";
import { bronNaam, bronPaarNaam, toonPan } from "./kanalen";
import { DbLabel, Fader, Meter, Naam, Pan, SleepGetal } from "./onderdelen";

export interface BronProps {
  /** Linker (of enige) bron van de strip, 0..36. */
  bron: number;
  gekoppeld: boolean;
  kruispunt: Kruispunt;
  solo: boolean;
  soloActief: boolean;
  naam: string | undefined;
  onRoute: (w: { db?: number; pan?: number; mute?: boolean }) => void;
  onSolo: (aan: boolean) => void;
  onKoppel: (aan: boolean) => void;
  onNaam: (n: string) => void;
  /** Voor ingang 1..4: de voorversterker. */
  voorversterker?: React.ReactNode;
}

export function BronStrip(p: BronProps) {
  const playback = p.bron >= KANALEN;
  const kanaal = playback ? p.bron - KANALEN : p.bron;
  const kanalen = p.gekoppeld ? [kanaal, kanaal + 1] : [kanaal];
  const standaard = p.gekoppeld ? bronPaarNaam(p.bron) : bronNaam(p.bron);
  const stil = p.kruispunt.mute || (p.soloActief && !p.solo) || p.kruispunt.db <= UIT_DB;
  const isLinks = p.bron % 2 === 0;

  return (
    <div className={"strip" + (p.gekoppeld ? " strip-stereo" : "") + (stil ? " strip-stil" : "")}>
      <Naam naam={p.naam} standaard={standaard} onChange={p.onNaam} />
      <div className="strip-kop">{p.voorversterker}</div>
      <Pan
        pan={p.kruispunt.pan}
        label={p.gekoppeld ? (p.kruispunt.pan === 0 ? "Bal" : toonPan(p.kruispunt.pan)) : toonPan(p.kruispunt.pan)}
        onChange={(pan) => p.onRoute({ pan })}
      />
      <div className="strip-midden">
        <Meter soort={playback ? "playback" : "ingangen"} kanalen={kanalen} />
        <Fader db={p.kruispunt.db} onChange={(db) => p.onRoute({ db })} uit={stil} />
      </div>
      <DbLabel db={p.kruispunt.db} />
      <div className="knoppen">
        <button
          className={"knop knop-mute" + (p.kruispunt.mute ? " aan" : "")}
          onClick={() => p.onRoute({ mute: !p.kruispunt.mute })}
          title="Mute in this mix"
        >
          M
        </button>
        <button
          className={"knop knop-solo" + (p.solo ? " aan" : "")}
          onClick={() => p.onSolo(!p.solo)}
          title="Solo (all mixes)"
        >
          S
        </button>
        {(p.gekoppeld || isLinks) && (
          <button
            className={"knop knop-koppel" + (p.gekoppeld ? " aan" : "")}
            onClick={() => p.onKoppel(!p.gekoppeld)}
            title={p.gekoppeld ? "Split into two mono channels" : "Link with the next channel as a stereo pair"}
          >
            {p.gekoppeld ? "∞" : "–"}
          </button>
        )}
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ voorversterker

const micToon = (v: number) => `${v} dB`;
const lineToon = (v: number) => `${(v / 2).toFixed(1)} dB`;

/** Gain en schakelaars van ingang 1..4 (`kanaal` 0..3). */
export function Voorversterker({
  kanaal,
  gain,
  configuratie,
  onGain,
  onConfiguratie,
}: {
  kanaal: number;
  gain: number;
  configuratie: Configuratie;
  onGain: (v: number) => void;
  onConfiguratie: (c: Configuratie) => void;
}) {
  const mic = kanaal < 2;
  const i = kanaal % 2;
  const wissel = (veld: "fantoom" | "instrument" | "pad") => {
    const lijst = [...configuratie[veld]] as [boolean, boolean];
    lijst[i] = !lijst[i];
    onConfiguratie({ ...configuratie, [veld]: lijst });
  };
  return (
    <div className="voorversterker">
      <SleepGetal
        waarde={gain}
        min={0}
        max={mic ? MIC_MAX : LINE_MAX}
        toon={mic ? micToon : lineToon}
        onChange={onGain}
        titel={`Gain input ${kanaal + 1} — drag or wheel, double-click for 0`}
      />
      <div className="vv-schakelaars">
        {mic ? (
          <button
            className={"mini" + (configuratie.fantoom[i] ? " aan rood" : "")}
            onClick={() => wissel("fantoom")}
            title="+48 V phantom power"
          >
            48V
          </button>
        ) : (
          <>
            <button
              className={"mini" + (configuratie.instrument[i] ? " aan" : "")}
              onClick={() => wissel("instrument")}
              title="Instrument (Hi-Z) input"
            >
              Inst
            </button>
            <button
              className={"mini" + (configuratie.pad[i] ? " aan" : "")}
              onClick={() => wissel("pad")}
              title="Pad: attenuate the input"
            >
              Pad
            </button>
          </>
        )}
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ uitgang

export function UitgangStrip({
  paar,
  uitgang,
  gekozen,
  naam,
  standaard,
  onKies,
  onUitgang,
  onNaam,
}: {
  paar: number;
  uitgang: Uitgang;
  gekozen: boolean;
  naam: string | undefined;
  standaard: string;
  onKies: () => void;
  onUitgang: (w: { db?: number; mute?: boolean }) => void;
  onNaam: (n: string) => void;
}) {
  return (
    <div
      className={"strip strip-stereo strip-uit" + (gekozen ? " gekozen" : "") + (uitgang.mute ? " strip-stil" : "")}
    >
      <Naam naam={naam} standaard={standaard} onChange={onNaam} className="naam-uit" onClick={onKies} />
      <button className={"kies" + (gekozen ? " aan" : "")} onClick={onKies} title="Show and edit the mix for this output">
        {gekozen ? "editing" : "edit mix"}
      </button>
      <div className="strip-midden">
        <Meter soort="uitgangen" kanalen={[2 * paar, 2 * paar + 1]} />
        <Fader db={uitgang.db} onChange={(db) => onUitgang({ db })} uit={uitgang.mute} />
      </div>
      <DbLabel db={uitgang.db} />
      <div className="knoppen">
        <button
          className={"knop knop-mute" + (uitgang.mute ? " aan" : "")}
          onClick={() => onUitgang({ mute: !uitgang.mute })}
          title="Mute output"
        >
          M
        </button>
      </div>
    </div>
  );
}
