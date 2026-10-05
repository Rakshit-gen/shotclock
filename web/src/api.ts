// Calls to shotclock-web. Shapes mirror src/bin/web.rs and src/scan.rs.

export interface Photo {
  name: string;
  /** Wall-clock time as the camera wrote it, "2024-10-03T08:24:58". */
  taken: string;
  /** Minutes east of UTC, if recorded. */
  tz: number | null;
  thumbnail: boolean;
}

export interface Suggestion {
  /** Seconds to add to the camera's times. */
  offset: number;
  confidence: number;
  word: "high" | "medium" | "low";
  matched: number;
  runner_up: number | null;
  note: string | null;
}

export interface Camera {
  name: string;
  reference: boolean;
  photos: Photo[];
  suggestion: Suggestion | null;
}

export interface Folder {
  dir: string;
  out: string;
  generation: number;
  cameras: Camera[];
  skipped: { name: string; reason: string }[];
}

export type TzChoice = null | "phone" | number;

export interface WriteResult {
  out: string;
  cameras: { camera: string; written: number }[];
}

async function json<T>(res: Response): Promise<T> {
  const body = await res.json().catch(() => null);
  if (!res.ok) throw new Error(body?.error ?? `The server answered ${res.status}.`);
  return body as T;
}

const post = <T>(url: string, body: unknown) =>
  fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  }).then((r) => json<T>(r));

export const getFolder = () =>
  fetch("/api/folder").then((r) => json<Folder | { dir: null }>(r));

export const openFolder = (path: string) => post<Folder>("/api/folder", { path });

export const writeCopies = (
  fixes: { camera: number; offset: number; tz: TzChoice }[],
  out: string,
) => post<WriteResult>("/api/write", { fixes, out });

export const thumbUrl = (f: Folder, camera: number, index: number) =>
  `/api/thumb/${f.generation}/${camera}/${index}`;
