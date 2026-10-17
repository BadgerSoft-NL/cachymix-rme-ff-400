// De bouwstenen van een kanaalstrip: fader, meter, pan en sleepgetal.
// Alles sleept relatief, zodat een klik nooit een sprong maakt; met Shift
// erbij gaat het vijf keer zo fijn, dubbelklik zet terug.

import { useEffect, useRef, useState } from "react";
import { MAX_DB, UIT_DB } from "./api";
import { dbNaarStand, laatsteMeters, MeterSoort, meterStand, opMeters, standNaarDb, toonDb } from "./kanalen";

function sleep(
  e: React.PointerEvent,
  opBeweging: (dx: number, dy: number, fijn: boolean) => void,
  opEinde?: () => void,
) {
  if (e.button !== 0) return;
  e.preventDefault();
  const doel = e.currentTarget as HTMLElement;
  doel.setPointerCapture(e.pointerId);
  let x = e.clientX;
  let y = e.clientY;
  const beweeg = (ev: PointerEvent) => {
    const dx = ev.clientX - x;
    const dy = ev.clientY - y;
    x = ev.clientX;
    y = ev.clientY;
    opBeweging(dx, dy, ev.shiftKey);
  };
  const los = () => {
    doel.removeEventListener("pointermove", beweeg);
    doel.removeEventListener("pointerup", los);
    doel.removeEventListener("pointercancel", los);
    opEinde?.();
  };
  doel.addEventListener("pointermove", beweeg);
  doel.addEventListener("pointerup", los);
  doel.addEventListener("pointercancel", los);
}

// ------------------------------------------------------------------ fader

const SCHAALSTREPEN = [6, 0, -6, -12, -20, -30, -40, -60];

export function Fader({
  db,
  onChange,
  standaard = 0,
  uit = false,
}: {
  db: number;
  onChange: (db: number) => void;
  standaard?: number;
  uit?: boolean;
}) {
  const baan = useRef<HTMLDivElement>(null);
  // Tijdens het slepen telt de stand, niet de afgeronde dB uit de props:
  // anders blijft de fader onderin hangen waar de wet heel steil is.
  const stand = useRef(dbNaarStand(db));
  const sleeptRef = useRef(false);
  if (!sleeptRef.current) stand.current = dbNaarStand(db);

  const zet = (s: number) => {
    stand.current = Math.min(1, Math.max(0, s));
    const nieuw = Math.round(standNaarDb(stand.current) * 10) / 10;
    onChange(nieuw <= UIT_DB + 0.05 ? UIT_DB : nieuw);
  };

  const opWiel = (e: React.WheelEvent) => {
    const stap = e.shiftKey ? 0.1 : 1;
    const richting = e.deltaY < 0 ? 1 : -1;
    const huidig = db <= UIT_DB ? -61 : db;
    const nieuw = Math.min(MAX_DB, huidig + richting * stap);
    onChange(nieuw < -60 ? UIT_DB : Math.round(nieuw * 10) / 10);
  };

  return (
    <div
      className={"fader" + (uit ? " fader-uit" : "")}
      ref={baan}
      onPointerDown={(e) => {
        sleeptRef.current = true;
        sleep(
          e,
          (_, dy, fijn) => {
            const h = baan.current?.clientHeight ?? 200;
            zet(stand.current - (dy / (h - 24)) * (fijn ? 0.2 : 1));
          },
          () => (sleeptRef.current = false),
        );
      }}
      onDoubleClick={() => onChange(standaard)}
      onWheel={opWiel}
      title="Drag, wheel, Shift for fine, double-click for 0 dB"
    >
      <div className="fader-schaal">
        {SCHAALSTREPEN.map((d) => (
          <div key={d} className="fader-streep" style={{ bottom: `calc((100% - 24px) * ${dbNaarStand(d)} + 12px)` }}>
            <span>{d > 0 ? `+${d}` : d}</span>
          </div>
        ))}
      </div>
      <div className="fader-gleuf" />
      <div className="fader-knop" style={{ bottom: `calc((100% - 24px) * ${stand.current})` }} />
    </div>
  );
}

// ------------------------------------------------------------------ meter

export function Meter({ soort, kanalen }: { soort: MeterSoort; kanalen: number[] }) {
  const balken = useRef<(HTMLDivElement | null)[]>([]);
  const pieken = useRef<(HTMLDivElement | null)[]>([]);
  const clips = useRef<(HTMLDivElement | null)[]>([]);
  const sleutel = kanalen.join(",");

  useEffect(() => {
    const piek = kanalen.map(() => 0);
    const piekTijd = kanalen.map(() => 0);
    const clipTot = kanalen.map(() => 0);
    let getoond = kanalen.map(() => -1);

    const teken = (m: NonNullable<typeof laatsteMeters>) => {
      const nu = performance.now();
      kanalen.forEach((k, i) => {
        const v = m[soort][k] ?? 0;
        const s = meterStand(v);
        if (s >= piek[i] || nu - piekTijd[i] > 1500) {
          piek[i] = s;
          piekTijd[i] = nu;
        }
        if (v >= 0.999) clipTot[i] = nu + 2000;
        if (Math.abs(getoond[i] - s) > 0.002) {
          const b = balken.current[i];
          if (b) b.style.clipPath = `inset(${(1 - s) * 100}% 0 0 0)`;
          getoond[i] = s;
        }
        const p = pieken.current[i];
        if (p) p.style.bottom = `${piek[i] * 100}%`;
        const c = clips.current[i];
        if (c) c.classList.toggle("aan", clipTot[i] > nu);
      });
    };
    if (laatsteMeters) teken(laatsteMeters);
    return opMeters(teken);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [soort, sleutel]);

  return (
    <div className="meter">
      {kanalen.map((k, i) => (
        <div key={k} className="meter-kanaal">
          <div className="meter-clip" ref={(el) => void (clips.current[i] = el)} />
          <div className="meter-baan">
            <div className="meter-balk" ref={(el) => void (balken.current[i] = el)} />
            <div className="meter-piek" ref={(el) => void (pieken.current[i] = el)} />
          </div>
        </div>
      ))}
    </div>
  );
}

// ------------------------------------------------------------------ pan

export function Pan({ pan, onChange, label }: { pan: number; onChange: (p: number) => void; label: string }) {
  const stand = useRef(pan);
  const [sleept, setSleept] = useState(false);
  if (!sleept) stand.current = pan;
  return (
    <div
      className="pan"
      onPointerDown={(e) => {
        setSleept(true);
        sleep(
          e,
          (dx, dy, fijn) => {
            stand.current = Math.min(1, Math.max(-1, stand.current + ((dx - dy) / 60) * (fijn ? 0.2 : 1)));
            onChange(Math.round(stand.current * 100) / 100);
          },
          () => setSleept(false),
        );
      }}
      onDoubleClick={() => onChange(0)}
      onWheel={(e) => onChange(Math.min(1, Math.max(-1, Math.round((pan + (e.deltaY < 0 ? 0.05 : -0.05)) * 100) / 100)))}
      title="Pan — drag sideways, double-click to centre"
    >
      <div className="pan-baan">
        <div
          className="pan-vulling"
          style={{
            left: `${50 + Math.min(0, pan) * 50}%`,
            width: `${Math.abs(pan) * 50}%`,
          }}
        />
        <div className="pan-midden" />
        <div className="pan-wijzer" style={{ left: `${50 + pan * 50}%` }} />
      </div>
      <span className="pan-label">{label}</span>
    </div>
  );
}

// ------------------------------------------------------------------ sleepgetal

/** Een getal dat je verticaal versleept, voor de gains van de voorversterkers. */
export function SleepGetal({
  waarde,
  min,
  max,
  toon,
  onChange,
  titel,
}: {
  waarde: number;
  min: number;
  max: number;
  toon: (v: number) => string;
  onChange: (v: number) => void;
  titel: string;
}) {
  const acc = useRef(waarde);
  return (
    <div
      className="sleepgetal"
      title={titel}
      onPointerDown={(e) => {
        acc.current = waarde;
        sleep(e, (dx, dy, fijn) => {
          acc.current = Math.min(max, Math.max(min, acc.current + ((dx - dy) / 4) * (fijn ? 0.25 : 1)));
          const v = Math.round(acc.current);
          if (v !== waarde) onChange(v);
        });
      }}
      onWheel={(e) => {
        const v = Math.min(max, Math.max(min, waarde + (e.deltaY < 0 ? 1 : -1)));
        if (v !== waarde) onChange(v);
      }}
      onDoubleClick={() => onChange(min)}
    >
      <div className="sleepgetal-vulling" style={{ width: `${((waarde - min) / (max - min)) * 100}%` }} />
      <span>{toon(waarde)}</span>
    </div>
  );
}

// ------------------------------------------------------------------ naam

/** Een naam die je met dubbelklik verandert. Leeg maken zet de standaard terug. */
export function Naam({
  naam,
  standaard,
  onChange,
  className,
  onClick,
}: {
  naam: string | undefined;
  standaard: string;
  onChange: (n: string) => void;
  className?: string;
  onClick?: () => void;
}) {
  const [bewerkt, setBewerkt] = useState(false);
  const getoond = naam && naam.length > 0 ? naam : standaard;
  if (bewerkt) {
    return (
      <input
        className={"naam-invoer " + (className ?? "")}
        autoFocus
        defaultValue={naam ?? ""}
        placeholder={standaard}
        maxLength={24}
        onBlur={(e) => {
          setBewerkt(false);
          onChange(e.currentTarget.value);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") setBewerkt(false);
        }}
      />
    );
  }
  return (
    <div
      className={"naam " + (className ?? "")}
      title={`${getoond} — double-click to rename`}
      onClick={onClick}
      onDoubleClick={() => setBewerkt(true)}
    >
      {getoond}
    </div>
  );
}

export function DbLabel({ db }: { db: number }) {
  return <div className={"db-label" + (db > 0 ? " db-boven" : "")}>{toonDb(db)}</div>;
}
