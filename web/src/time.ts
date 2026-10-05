// Times on the page are milliseconds on a clock with no time zone: the
// camera's wall-clock reading, treated as if it were UTC so the browser's
// own zone never shifts anything.

export const parseTaken = (s: string) => Date.parse(s + "Z");

const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const two = (n: number) => String(n).padStart(2, "0");

/** "08:24:58" */
export function clock(ms: number): string {
  const d = new Date(ms);
  return `${two(d.getUTCHours())}:${two(d.getUTCMinutes())}:${two(d.getUTCSeconds())}`;
}

/** "Thu 3 Oct" */
export function day(ms: number): string {
  const d = new Date(ms);
  return `${DAYS[d.getUTCDay()]} ${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]}`;
}

/** "Thu 3 Oct 2024, 08:24:58" */
export function dateTime(ms: number): string {
  return `${day(ms)} ${new Date(ms).getUTCFullYear()}, ${clock(ms)}`;
}

/** Seconds as "+5 h 37 min", "−1 h", "+12 min 30 s" or "no change". */
export function formatOffset(secs: number): string {
  if (secs === 0) return "no change";
  const sign = secs < 0 ? "−" : "+";
  let a = Math.abs(Math.round(secs));
  const d = Math.floor(a / 86400);
  a -= d * 86400;
  const h = Math.floor(a / 3600);
  const m = Math.floor((a % 3600) / 60);
  const s = a % 60;
  const parts = [d && `${d} d`, h && `${h} h`, m && `${m} min`, s && `${s} s`].filter(Boolean);
  return `${sign}${parts.join(" ")}`;
}

/** "+09:00" */
export function formatTz(minutes: number): string {
  const a = Math.abs(minutes);
  return `${minutes < 0 ? "−" : "+"}${two(Math.floor(a / 60))}:${two(a % 60)}`;
}

/**
 * What people type for an offset: "+5h37m", "-1h", "90s", "-0:30", "45"
 * (minutes). Same rules as the command line. Null if it can't be read.
 */
export function parseOffset(input: string): number | null {
  let t = input.trim().toLowerCase().replace(/\s+/g, "").replace("−", "-");
  let sign = 1;
  if (t.startsWith("-") || t.startsWith("+")) {
    sign = t.startsWith("-") ? -1 : 1;
    t = t.slice(1);
  }
  if (!t) return null;
  const colon = /^(\d+):(\d+)(?::(\d+))?$/.exec(t);
  if (colon) {
    return sign * (+colon[1]! * 3600 + +colon[2]! * 60 + +(colon[3] ?? 0));
  }
  if (/^\d+$/.test(t)) return sign * +t * 60;
  const units = /^(?:(\d+)d)?(?:(\d+)h)?(?:(\d+)m(?:in)?)?(?:(\d+)s)?$/.exec(t);
  if (!units || units.slice(1).every((u) => u === undefined)) return null;
  const [d, h, m, s] = units.slice(1).map((u) => +(u ?? 0)) as [number, number, number, number];
  return sign * (d * 86400 + h * 3600 + m * 60 + s);
}

/** The offset that makes a camera photo land on a phone photo. */
export const pairOffset = (camera: string, phone: string) =>
  Math.round((parseTaken(phone) - parseTaken(camera)) / 1000);

/**
 * How far a key press nudges a camera, in seconds: arrows move a minute,
 * with Shift an hour, with Alt a second. Null for other keys.
 */
export function nudgeFor(e: { key: string; shiftKey: boolean; altKey: boolean }): number | null {
  const dir = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
  if (!dir) return null;
  return dir * (e.shiftKey ? 3600 : e.altKey ? 1 : 60);
}

/**
 * Which photos get a thumbnail in a lane: walk left to right and take a
 * photo only if it starts clear of the last one drawn. The rest show as
 * ticks. `xs` must be sorted.
 */
export function thumbsToDraw(xs: number[], width: number): number[] {
  const out: number[] = [];
  let next = -Infinity;
  xs.forEach((x, i) => {
    if (x >= next) {
      out.push(i);
      next = x + width;
    }
  });
  return out;
}

/** The time zone of the phone photo nearest to `t`, or null. */
export function nearestTz(phone: { t: number; tz: number | null }[], t: number): number | null {
  let best: number | null = null;
  let gap = Infinity;
  for (const p of phone) {
    if (p.tz === null) continue;
    const g = Math.abs(p.t - t);
    if (g < gap) {
      gap = g;
      best = p.tz;
    }
  }
  return best;
}
