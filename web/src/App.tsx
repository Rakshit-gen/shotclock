import { useCallback, useEffect, useMemo, useState } from "react";
import { type Folder, type TzChoice, type WriteResult, getFolder, openFolder, writeCopies } from "./api";
import CameraPanel from "./CameraPanel";
import OpenFolder from "./OpenFolder";
import Timeline, { type Pick } from "./Timeline";
import WritePanel, { type TzMode } from "./WritePanel";
import { nearestTz, pairOffset, parseTaken, parseTz } from "./time";

export default function App() {
  const [folder, setFolder] = useState<Folder | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [opening, setOpening] = useState(false);
  const [openError, setOpenError] = useState<string | null>(null);
  const [reopen, setReopen] = useState(false);

  const [offsets, setOffsets] = useState<number[]>([]);
  const [selected, setSelected] = useState(0);
  const [pick, setPick] = useState<Pick | null>(null);
  const [include, setInclude] = useState<boolean[]>([]);
  const [tzMode, setTzMode] = useState<TzMode>("keep");
  const [fixedTz, setFixedTz] = useState("+00:00");
  const [out, setOut] = useState("");
  const [writing, setWriting] = useState(false);
  const [writeError, setWriteError] = useState<string | null>(null);
  const [result, setResult] = useState<WriteResult | null>(null);

  const start = useCallback((f: Folder) => {
    setFolder(f);
    setOffsets(f.cameras.map((c) => c.suggestion?.offset ?? 0));
    setSelected(Math.max(0, f.cameras.findIndex((c) => !c.reference)));
    setInclude(f.cameras.map((c) => !c.reference));
    const phoneTz = f.cameras.find((c) => c.reference)?.photos.some((p) => p.tz !== null);
    setTzMode(phoneTz ? "phone" : "keep");
    setOut(f.out);
    setPick(null);
    setResult(null);
    setWriteError(null);
    setReopen(false);
  }, []);

  useEffect(() => {
    getFolder()
      .then((f) => f.dir !== null && start(f as Folder))
      .catch((e: Error) => setOpenError(e.message))
      .finally(() => setLoaded(true));
  }, [start]);

  const open = (path: string) => {
    setOpening(true);
    setOpenError(null);
    openFolder(path)
      .then(start)
      .catch((e: Error) => setOpenError(e.message))
      .finally(() => setOpening(false));
  };

  const phone = useMemo(() => {
    const ref = folder?.cameras.find((c) => c.reference);
    return (ref?.photos ?? []).map((p) => ({ t: parseTaken(p.taken), tz: p.tz }));
  }, [folder]);
  const phoneTz = useMemo(() => phone.find((p) => p.tz !== null)?.tz ?? null, [phone]);
  const fixedMinutes = parseTz(fixedTz);
  const zoneFor = useCallback(
    (t: number) => (tzMode === "phone" ? nearestTz(phone, t) : tzMode === "fixed" ? fixedMinutes : null),
    [tzMode, phone, fixedMinutes],
  );

  const setOffset = (camera: number, secs: number) => {
    setOffsets((o) => o.map((v, i) => (i === camera ? secs : v)));
    setResult(null);
  };

  const onPick = (p: Pick) => {
    if (!folder) return;
    if (!folder.cameras[p.camera]!.reference) {
      setPick(p);
      setSelected(p.camera);
      return;
    }
    if (!pick) return;
    const cam = folder.cameras[pick.camera]!.photos[pick.index]!;
    const ph = folder.cameras[p.camera]!.photos[p.index]!;
    setOffset(pick.camera, pairOffset(cam.taken, ph.taken));
    setPick(null);
  };

  const write = () => {
    if (!folder) return;
    const tz: TzChoice = tzMode === "phone" ? "phone" : tzMode === "fixed" ? fixedMinutes : null;
    const fixes = folder.cameras
      .map((c, i) => ({ c, i }))
      .filter(({ c, i }) => !c.reference && include[i])
      .map(({ i }) => ({ camera: i, offset: offsets[i] ?? 0, tz }));
    setWriting(true);
    setWriteError(null);
    setResult(null);
    writeCopies(fixes, out)
      .then(setResult)
      .catch((e: Error) => setWriteError(e.message))
      .finally(() => setWriting(false));
  };

  const cameras = folder?.cameras.filter((c) => !c.reference).length ?? 0;

  return (
    <div className="app">
      <header className="top">
        <span className="brand">shotclock</span>
        {folder && !reopen && (
          <>
            <span className="path mono" title={folder.dir}>
              {/* The left mark keeps the leading slash where it belongs when
                  the path is cut from the left. */}
              {"\u200e" + folder.dir}
            </span>
            <button type="button" onClick={() => setReopen(true)}>
              Open another folder
            </button>
          </>
        )}
      </header>

      {loaded && (!folder || reopen) && (
        <main className="start">
          {!folder && (
            <p className="lede">
              shotclock fixes the dates on photos from a camera whose clock was wrong: never set after a battery
              change, an hour out after daylight saving, or still on home time abroad. Put the camera&rsquo;s photos
              and your phone&rsquo;s from the same days in one folder and open it here. The camera is lined up with
              the phone, and corrected copies go to a new folder. The originals are never changed.
            </p>
          )}
          <OpenFolder
            initial={folder?.dir ?? ""}
            busy={opening}
            error={openError}
            onOpen={open}
            onCancel={folder ? () => setReopen(false) : undefined}
          />
          {!folder && <p className="help">Reads JPEG files. HEIC and raw files are listed but skipped.</p>}
        </main>
      )}

      {folder && !reopen && (
        <main className="work">
          {cameras === 0 ? (
            <p className="notice">
              Every photo in this folder is from the {folder.cameras[0]?.name ?? "same camera"}, so there is nothing
              to line it up with. Add the phone&rsquo;s photos from the same days to the folder and open it again.
            </p>
          ) : (
            <>
              <Timeline
                folder={folder}
                offsets={offsets}
                selected={selected}
                onSelect={setSelected}
                onOffset={setOffset}
                pick={pick}
                onPick={onPick}
              />
              <div className="panels">
                <CameraPanel
                  key={selected}
                  folder={folder}
                  camera={selected}
                  offset={offsets[selected] ?? 0}
                  onOffset={(s) => setOffset(selected, s)}
                  pick={pick}
                  onCancelPick={() => setPick(null)}
                  zoneFor={zoneFor}
                />
                <WritePanel
                  folder={folder}
                  offsets={offsets}
                  include={include}
                  onInclude={(i, on) => setInclude((v) => v.map((x, j) => (j === i ? on : x)))}
                  tzMode={tzMode}
                  onTzMode={setTzMode}
                  fixedTz={fixedTz}
                  onFixedTz={setFixedTz}
                  fixedTzBad={fixedMinutes === null}
                  phoneTz={phoneTz}
                  out={out}
                  onOut={setOut}
                  busy={writing}
                  error={writeError}
                  result={result}
                  onWrite={write}
                />
              </div>
            </>
          )}
          {folder.skipped.length > 0 && (
            <details className="skipped">
              <summary>
                Skipped {folder.skipped.length} {folder.skipped.length === 1 ? "file" : "files"}
              </summary>
              <ul>
                {folder.skipped.map((s) => (
                  <li key={s.name}>
                    <span className="mono">{s.name}</span>: {s.reason}
                  </li>
                ))}
              </ul>
            </details>
          )}
        </main>
      )}
    </div>
  );
}
