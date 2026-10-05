import type { Folder, WriteResult } from "./api";
import { formatOffset, formatTz } from "./time";

export type TzMode = "keep" | "phone" | "fixed";

interface Props {
  folder: Folder;
  offsets: number[];
  include: boolean[];
  onInclude: (camera: number, on: boolean) => void;
  tzMode: TzMode;
  onTzMode: (m: TzMode) => void;
  fixedTz: string;
  onFixedTz: (s: string) => void;
  fixedTzBad: boolean;
  phoneTz: number | null;
  out: string;
  onOut: (s: string) => void;
  busy: boolean;
  error: string | null;
  result: WriteResult | null;
  onWrite: () => void;
}

export default function WritePanel(p: Props) {
  const cams = p.folder.cameras.map((c, i) => ({ c, i })).filter(({ c }) => !c.reference);
  const count = cams.reduce((n, { c, i }) => n + (p.include[i] ? c.photos.length : 0), 0);
  return (
    <section className="write-panel" aria-labelledby="write-title">
      <h2 id="write-title">Write corrected copies</h2>

      <fieldset>
        <legend>Cameras</legend>
        {cams.map(({ c, i }) => (
          <label key={c.name} className="check">
            <input type="checkbox" checked={p.include[i] ?? false} onChange={(e) => p.onInclude(i, e.target.checked)} />
            {c.name}, <span className="mono">{formatOffset(p.offsets[i] ?? 0)}</span>
          </label>
        ))}
      </fieldset>

      <fieldset>
        <legend>Time zone</legend>
        <label className="check">
          <input type="radio" name="tz" checked={p.tzMode === "keep"} onChange={() => p.onTzMode("keep")} />
          Leave it as the camera wrote it
        </label>
        <label className="check">
          <input
            type="radio"
            name="tz"
            checked={p.tzMode === "phone"}
            disabled={p.phoneTz === null}
            onChange={() => p.onTzMode("phone")}
          />
          {p.phoneTz === null
            ? "Copy it from the phone (the phone photos don't record one)"
            : `Copy it from the nearest phone photo (${formatTz(p.phoneTz)} here)`}
        </label>
        <label className="check">
          <input type="radio" name="tz" checked={p.tzMode === "fixed"} onChange={() => p.onTzMode("fixed")} />
          Set it to
          <input
            className="tz-input"
            value={p.fixedTz}
            onChange={(e) => p.onFixedTz(e.target.value)}
            onFocus={() => p.onTzMode("fixed")}
            aria-label="Time zone, like +09:00"
            aria-invalid={p.tzMode === "fixed" && p.fixedTzBad}
            spellCheck={false}
          />
        </label>
        {p.tzMode === "fixed" && p.fixedTzBad && (
          <p className="help error">Write the time zone like +09:00 or -03:30.</p>
        )}
      </fieldset>

      <label htmlFor="out-path">Save the copies in</label>
      <input id="out-path" value={p.out} onChange={(e) => p.onOut(e.target.value)} spellCheck={false} />
      <p className="help">
        A new folder, with the same file names. The photos you opened are never changed.
      </p>

      <button
        type="button"
        className="primary"
        disabled={p.busy || count === 0 || (p.tzMode === "fixed" && p.fixedTzBad)}
        onClick={p.onWrite}
      >
        {p.busy ? "Writing copies" : `Write ${count} corrected ${count === 1 ? "copy" : "copies"}`}
      </button>

      <div aria-live="polite">
        {p.error && (
          <p className="help error" role="alert">
            {p.error}
          </p>
        )}
        {p.result && (
          <p className="done">
            Wrote{" "}
            {p.result.cameras.map((c, i) => (
              <span key={c.camera}>
                {i > 0 && " and "}
                {c.written} from the {c.camera}
              </span>
            ))}{" "}
            to <span className="mono">{p.result.out}</span>. Each copy&rsquo;s image data was checked against its
            original and matches.
          </p>
        )}
      </div>
    </section>
  );
}
