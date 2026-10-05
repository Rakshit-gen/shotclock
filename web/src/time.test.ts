import { describe, expect, it } from "vitest";
import {
  clock,
  dateTime,
  formatOffset,
  formatTz,
  nearestTz,
  nudgeFor,
  pairOffset,
  parseOffset,
  parseTaken,
  parseTz,
  thumbsToDraw,
} from "./time";

describe("offsets", () => {
  it("reads what people type, like the command line does", () => {
    expect(parseOffset("+5h37m")).toBe(20220);
    expect(parseOffset("-1h")).toBe(-3600);
    expect(parseOffset("−1 h 5 min")).toBe(-3900);
    expect(parseOffset("90s")).toBe(90);
    expect(parseOffset("45")).toBe(2700);
    expect(parseOffset("-0:30")).toBe(-1800);
    expect(parseOffset("1:02:03")).toBe(3723);
    for (const bad of ["", "+", "1x", "30m1h", "h", "1::2"]) expect(parseOffset(bad)).toBeNull();
  });

  it("writes them for people and reads its own output back", () => {
    expect(formatOffset(20220)).toBe("+5 h 37 min");
    expect(formatOffset(-3600)).toBe("−1 h");
    expect(formatOffset(0)).toBe("no change");
    for (const s of [1, 59, 3600, -20220, 90061]) expect(parseOffset(formatOffset(s))).toBe(s);
  });

  it("takes the gap between a pair of photos", () => {
    expect(pairOffset("2024-10-03T02:32:00", "2024-10-03T08:09:00")).toBe(20220);
  });
});

describe("clock text", () => {
  it("ignores the browser's time zone", () => {
    const t = parseTaken("2024-10-03T23:59:58");
    expect(clock(t)).toBe("23:59:58");
    expect(dateTime(t)).toBe("Thu 3 Oct 2024, 23:59:58");
    expect(formatTz(540)).toBe("+09:00");
    expect(formatTz(-210)).toBe("−03:30");
    expect(parseTz("+09:00")).toBe(540);
    expect(parseTz("-3:30")).toBe(-210);
    expect(parseTz(formatTz(-210))).toBe(-210);
    expect(parseTz("9")).toBe(540);
    expect(parseTz("+15:00")).toBeNull();
    expect(parseTz("Tokyo")).toBeNull();
  });
});

describe("keyboard nudges", () => {
  const key = (key: string, shiftKey = false, altKey = false) => nudgeFor({ key, shiftKey, altKey });
  it("moves a minute, an hour with Shift, a second with Alt", () => {
    expect(key("ArrowRight")).toBe(60);
    expect(key("ArrowLeft", true)).toBe(-3600);
    expect(key("ArrowRight", false, true)).toBe(1);
    expect(key("a")).toBeNull();
  });
});

describe("lanes", () => {
  it("draws a thumbnail only where it fits", () => {
    expect(thumbsToDraw([0, 10, 50, 55, 120], 48)).toEqual([0, 2, 4]);
    expect(thumbsToDraw([], 48)).toEqual([]);
  });

  it("takes the time zone from the nearest phone photo that has one", () => {
    const phone = [
      { t: 0, tz: 60 },
      { t: 100, tz: null },
      { t: 200, tz: 540 },
    ];
    expect(nearestTz(phone, 90)).toBe(60);
    expect(nearestTz(phone, 160)).toBe(540);
    expect(nearestTz([], 0)).toBeNull();
  });
});
