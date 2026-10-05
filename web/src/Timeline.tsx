import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { type Folder, thumbUrl } from "./api";
import { clock, day, formatOffset, parseTaken, thumbsToDraw } from "./time";

export interface Pick {
  camera: number;
  index: number;
}

interface Props {
  folder: Folder;
  /** Seconds added to each camera's times; ignored for the reference. */
  offsets: number[];
  selected: number;
  onSelect: (camera: number) => void;
  pick: Pick | null;
  onPick: (p: Pick) => void;
}

const THUMB_W = 64;
const HOUR = 3_600_000;
const PAD = 40;

// Ruler steps to choose from, in ms. The smallest that leaves room for a
// label wins.
const STEPS = [60_000, 5 * 60_000, 15 * 60_000, 30 * 60_000, HOUR, 3 * HOUR, 6 * HOUR, 12 * HOUR, 24 * HOUR];

export default function Timeline({ folder, offsets, selected, onSelect, pick, onPick }: Props) {
  const scroller = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(800);
  const [scrollLeft, setScrollLeft] = useState(0);
  // Pixels per ms, or null to fit the whole trip in view.
  const [zoom, setZoom] = useState<number | null>(null);

  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  const lanes = useMemo(
    () =>
      folder.cameras.map((c, i) => {
        const shift = c.reference ? 0 : (offsets[i] ?? 0) * 1000;
        return c.photos.map((p) => parseTaken(p.taken) + shift);
      }),
    [folder, offsets],
  );

  const [t0, t1] = useMemo(() => {
    let lo = Infinity;
    let hi = -Infinity;
    for (const l of lanes) {
      if (!l.length) continue;
      lo = Math.min(lo, l[0]!);
      hi = Math.max(hi, l[l.length - 1]!);
    }
    if (!isFinite(lo)) return [0, HOUR];
    const pad = Math.max((hi - lo) * 0.02, 10 * 60_000);
    return [lo - pad, hi + pad];
  }, [lanes]);

  const fit = (width - 2 * PAD) / (t1 - t0);
  const scale = zoom ?? fit;
  const x = (t: number) => PAD + (t - t0) * scale;
  const full = Math.max(width, (t1 - t0) * scale + 2 * PAD);

  // Keep the middle of the view still while zooming.
  const zoomTo = (next: number | null) => {
    const el = scroller.current;
    const s = next ?? fit;
    if (!el) return setZoom(next);
    const mid = t0 + (el.scrollLeft + el.clientWidth / 2 - PAD) / scale;
    setZoom(next);
    requestAnimationFrame(() => {
      el.scrollLeft = PAD + (mid - t0) * s - el.clientWidth / 2;
    });
  };

  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    let frame = 0;
    const onScroll = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => setScrollLeft(el.scrollLeft));
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => el.removeEventListener("scroll", onScroll);
  }, []);

  const ticks = useMemo(() => {
    const step = STEPS.find((s) => s * scale >= 72) ?? 24 * HOUR;
    const out: { t: number; label: string; day: boolean }[] = [];
    for (let t = Math.ceil(t0 / step) * step; t <= t1; t += step) {
      const midnight = t % (24 * HOUR) === 0;
      out.push({ t, label: midnight ? day(t) : clock(t).slice(0, 5), day: midnight });
    }
    return out;
  }, [t0, t1, scale]);

  // Only thumbnails near the visible part of the strip are put in the page.
  const visible = (px: number) => px > scrollLeft - width && px < scrollLeft + 2 * width;

  return (
    <section className="timeline" aria-label="Timeline">
      <div className="zoom" role="group" aria-label="Zoom">
        <button type="button" aria-pressed={zoom === null} onClick={() => zoomTo(null)}>
          Whole trip
        </button>
        <button type="button" aria-pressed={zoom !== null && Math.abs(zoom * 24 * HOUR - width) < 2} onClick={() => zoomTo(width / (24 * HOUR))}>
          A day
        </button>
        <button type="button" aria-pressed={zoom !== null && Math.abs(zoom * 2 * HOUR - width) < 2} onClick={() => zoomTo(width / (2 * HOUR))}>
          Two hours
        </button>
      </div>
      <div className="scroller" ref={scroller}>
        <div className="canvas" style={{ width: full }}>
          <div className="ruler" aria-hidden="true">
            {ticks.map((k) => (
              <span key={k.t} className={k.day ? "tick day" : "tick"} style={{ left: x(k.t) }}>
                {k.label}
              </span>
            ))}
          </div>
          {folder.cameras.map((cam, ci) => {
            const xs = lanes[ci]!.map(x);
            const drawn = thumbsToDraw(xs, THUMB_W + 4);
            return (
              <div key={cam.name} className={ci === selected ? "lane selected" : "lane"}>
                <div className="lane-head">
                  <span className="lane-name">{cam.name}</span>
                  <span className="lane-meta">
                    {cam.photos.length} photos
                    {cam.reference ? ", reference" : `, ${formatOffset(offsets[ci] ?? 0)}`}
                  </span>
                </div>
                <div
                  className={cam.reference ? "strip reference" : "strip"}
                  onPointerDown={() => !cam.reference && onSelect(ci)}
                >
                  {drawn.filter((i) => visible(xs[i]!)).map((i) => {
                    const p = cam.photos[i]!;
                    const picked = pick?.camera === ci && pick.index === i;
                    return (
                      <button
                        type="button"
                        key={i}
                        className={picked ? "frame picked" : "frame"}
                        style={{ left: xs[i]! }}
                        title={`${p.name}, ${clock(lanes[ci]![i]!)}`}
                        onClick={() => onPick({ camera: ci, index: i })}
                      >
                        {p.thumbnail ? (
                          <img src={thumbUrl(folder, ci, i)} alt="" loading="lazy" decoding="async" />
                        ) : (
                          <span className="no-thumb">{p.name}</span>
                        )}
                      </button>
                    );
                  })}
                  <div className="activity" aria-hidden="true">
                    {xs.map((px, i) => (
                      <i key={i} style={{ left: px }} />
                    ))}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
}
