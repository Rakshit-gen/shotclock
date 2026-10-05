import { useEffect, useState } from "react";
import type { Folder } from "./api";
import type { Pick } from "./Timeline";
import { clock, day, formatOffset, formatTz, parseOffset, parseTaken } from "./time";

const dayYear = (t: number) => `${day(t)} ${new Date(t).getUTCFullYear()}`;

interface Props {
  folder: Folder;
  camera: number;
  offset: number;
  onOffset: (secs: number) => void;
  pick: Pick | null;
  onCancelPick: () => void;
  /** The time zone that would be written for a corrected time, if any. */
  zoneFor: (t: number) => number | null;
}

const NUDGES: [string, number][] = [
  ["−1 h", -3600],
  ["−1 min", -60],
  ["+1 min", 60],
  ["+1 h", 3600],
];

const PREVIEW_ROWS = 5;

export default function CameraPanel({ folder, camera, offset, onOffset, pick, onCancelPick, zoneFor }: Props) {
  const cam = folder.cameras[camera]!;
  const s = cam.suggestion;
  const [draft, setDraft] = useState(formatOffset(offset));
  const [bad, setBad] = useState(false);

  // Follow drags and nudges, unless the field is being typed in.
  useEffect(() => {
    if (document.activeElement?.id !== "offset-input") setDraft(formatOffset(offset));
  }, [offset]);

  const apply = () => {
    const v = draft.trim() === "no change" ? 0 : parseOffset(draft);
    setBad(v === null);
    if (v !== null) onOffset(v);
  };

  const picked = pick?.camera === camera ? cam.photos[pick.index] : undefined;
  const rows = cam.photos.slice(0, PREVIEW_ROWS);

  return (
    <section className="camera-panel" aria-labelledby="camera-name">
      <h2 id="camera-name">{cam.name}</h2>

      <form
        className="offset-form"
        onSubmit={(e) => {
          e.preventDefault();
          apply();
        }}
      >
        <label htmlFor="offset-input">Add to every photo&rsquo;s time</label>
        <div className="offset-row">
          <input
            id="offset-input"
            className="offset-input"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={apply}
            spellCheck={false}
            autoComplete="off"
            aria-invalid={bad}
            aria-describedby="offset-help"
            style={{ width: `${Math.max(draft.length, 6) + 2}ch` }}
          />
          <div className="nudges" role="group" aria-label="Nudge">
            {NUDGES.map(([label, n]) => (
              <button type="button" key={label} onClick={() => onOffset(offset + n)}>
                {label}
              </button>
            ))}
          </div>
        </div>
        <p id="offset-help" className={bad ? "help error" : "help"}>
          {bad
            ? "That isn't an offset shotclock can read. Write it like +5h37m, -1h or -0:30."
            : "Type an offset, drag the strip, or select it and use the arrow keys: a minute each, an hour with Shift, a second with Alt."}
        </p>
      </form>

      <div className="suggestion">
        {s ? (
          <>
            <p>
              Suggested: <strong className="mono">{formatOffset(s.offset)}</strong>, {s.word} confidence.{" "}
              {Math.round(s.matched * 100)}% of this camera&rsquo;s photos land within 3 minutes of a phone photo.
              {s.note && <> Rounded because it is {s.note}.</>}
              {s.word === "low" && s.runner_up !== null && (
                <> Another good fit is {formatOffset(s.runner_up)}; check a few photos before writing.</>
              )}
            </p>
            <button type="button" disabled={offset === s.offset} onClick={() => onOffset(s.offset)}>
              {offset === s.offset ? "Using the suggestion" : "Use the suggestion"}
            </button>
          </>
        ) : (
          <p>
            There are no phone photos close enough in time to suggest an offset. Type one, drag the strip, or pick a
            pair of photos.
          </p>
        )}
      </div>

      <div className="pairing" aria-live="polite">
        {picked ? (
          <p>
            Picked <span className="mono">{picked.name}</span> ({clock(parseTaken(picked.taken))} on the camera). Now
            click a phone photo of the same moment.{" "}
            <button type="button" className="link" onClick={onCancelPick}>
              Cancel
            </button>
          </p>
        ) : (
          <p>
            Know two photos of the same moment? Click one from this camera on the timeline, then the phone&rsquo;s.
          </p>
        )}
      </div>

      <table className="preview">
        <caption>
          {cam.photos.length > PREVIEW_ROWS
            ? `First ${PREVIEW_ROWS} of ${cam.photos.length} photos`
            : `All ${cam.photos.length} photos`}
        </caption>
        <thead>
          <tr>
            <th scope="col">File</th>
            <th scope="col">Camera said</th>
            <th scope="col">Becomes</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((p) => {
            const t = parseTaken(p.taken);
            const to = t + offset * 1000;
            const zone = zoneFor(to);
            return (
              <tr key={p.name}>
                <td className="mono">{p.name}</td>
                <td>
                  <span className="when-day">{dayYear(t)}</span>
                  <span className="mono">{clock(t)}</span>
                </td>
                <td>
                  <span className="when-day">{dayYear(to)}</span>
                  <span className="mono">{clock(to)}</span>
                  {zone !== null && <span className="mono zone"> {formatTz(zone)}</span>}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </section>
  );
}
