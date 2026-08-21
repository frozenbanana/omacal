process.env.TZ = "Europe/Stockholm";
import { describe, expect, it } from "vitest";
import type { CalEvent } from "./store";
import {
  addDayKey,
  calendarDaysBetween,
  dayKey,
  eventDensityForDate,
  eventShowDays,
  getEventCountForDate,
  getOverflowCount,
  getTimedEventsForDayKey,
  getAllDayEventsForDayKey,
  groupEventsByDay,
  layoutMultiDayEventsForWeek,
  occupiedDayKeys,
  startOfWeek,
} from "./eventLayout";

function ev(over: Partial<CalEvent> & { start: string }): CalEvent {
  return {
    id: 1,
    calendar_id: 1,
    href: `href-${over.start}`,
    etag: null,
    uid: `uid-${over.start}`,
    title: "Event",
    description: "",
    location: "",
    end: null,
    all_day: false,
    rrule: null,
    color: "#ff0000",
    calendar_name: "Cal",
    status: null,
    organizer: null,
    attendees: [],
    alarms: [],
    my_partstat: null,
    readonly: false,
    ...over,
  };
}

describe("day key / calendar math (Europe/Stockholm)", () => {
  it("addDayKey walks calendar days, not 24h blocks", () => {
    expect(addDayKey("2026-03-28", 1)).toBe("2026-03-29");
    expect(addDayKey("2026-12-28", 7)).toBe("2027-01-04");
  });

  it("startOfWeek honors Monday-first", () => {
    const d = new Date(2026, 7, 5); // Wed Aug 5 2026
    expect(dayKey(startOfWeek(d, 1))).toBe("2026-08-03");
  });

  it("calendarDaysBetween is DST-safe (Mar spring-forward)", () => {
    const a = new Date(2026, 2, 28); // Sat
    const b = new Date(2026, 2, 29); // Sun (DST jump)
    expect(calendarDaysBetween(a, b)).toBe(1);
  });
});

describe("eventShowDays", () => {
  it("all-day event spanning several days is inclusive of end-1", () => {
    const e = ev({ start: "2026-08-05", end: "2026-08-09", all_day: true });
    expect(eventShowDays(e)).toEqual({ startKey: "2026-08-05", endKey: "2026-08-08" });
    expect(occupiedDayKeys(e)).toEqual(["2026-08-05", "2026-08-06", "2026-08-07", "2026-08-08"]);
  });

  it("single-day all-day uses exclusive end", () => {
    const e = ev({ start: "2026-08-05", end: "2026-08-06", all_day: true });
    expect(eventShowDays(e)).toEqual({ startKey: "2026-08-05", endKey: "2026-08-05" });
  });

  it("all-day without end degrades to one day", () => {
    const e = ev({ start: "2026-08-05", all_day: true });
    expect(eventShowDays(e)).toEqual({ startKey: "2026-08-05", endKey: "2026-08-05" });
  });

  it("timed event crossing month boundary", () => {
    // local 2026-08-31 22:00 -> 2026-09-01 01:00 (Stockholm = UTC+2)
    const e = ev({
      start: "2026-08-31T20:00:00.000Z",
      end: "2026-08-31T23:00:00.000Z",
    });
    expect(eventShowDays(e)).toEqual({ startKey: "2026-08-31", endKey: "2026-09-01" });
  });

  it("event ending exactly at local midnight does not occupy the next day", () => {
    // local 2026-08-05 22:00 -> 2026-08-06 00:00 (Stockholm utc+2 => end Z = 22:00 prev day)
    const e = ev({
      start: "2026-08-05T20:00:00.000Z",
      end: "2026-08-05T22:00:00.000Z",
    });
    expect(eventShowDays(e)).toEqual({ startKey: "2026-08-05", endKey: "2026-08-05" });
  });

  it("timed event spanning into the next morning occupies both days", () => {
    // local 2026-08-05 22:00 -> 2026-08-06 07:00
    const e = ev({
      start: "2026-08-05T20:00:00.000Z",
      end: "2026-08-06T05:00:00.000Z",
    });
    expect(eventShowDays(e)).toEqual({ startKey: "2026-08-05", endKey: "2026-08-06" });
  });

  it("all-day across DST fall-back boundary keeps exact day count", () => {
    const e = ev({ start: "2026-10-24", end: "2026-10-27", all_day: true });
    // inclusive 24, 25, 26 — 25 Oct 2026 is the fall-back day
    expect(occupiedDayKeys(e)).toEqual(["2026-10-24", "2026-10-25", "2026-10-26"]);
  });
});

describe("layoutMultiDayEventsForWeek", () => {
  it("splits a Sunday->Monday crossing into per-week segments", () => {
    // all-day Sun Aug 2 -> Tue Aug 4 inclusive
    const e = ev({ id: 1, start: "2026-08-02", end: "2026-08-05", all_day: true });
    const prevWeek = startOfWeek(new Date(2026, 6, 27), 1); // Mon Jul 27
    const thisWeek = startOfWeek(new Date(2026, 7, 3), 1); // Mon Aug 3
    const segPrev = layoutMultiDayEventsForWeek([e], prevWeek, 1);
    const segThis = layoutMultiDayEventsForWeek([e], thisWeek, 1);
    // Jul 27 week: Sun Aug 2 = col 6; starts here, continues next week
    expect(segPrev).toHaveLength(1);
    expect(segPrev[0].startCol).toBe(6);
    expect(segPrev[0].endCol).toBe(6);
    expect(segPrev[0].isStart).toBe(true);
    expect(segPrev[0].isEnd).toBe(false);
    // Aug 3 week: Mon = col 0 (start clipped), ends Tue col 1
    expect(segThis).toHaveLength(1);
    expect(segThis[0].startCol).toBe(0);
    expect(segThis[0].endCol).toBe(1);
    expect(segThis[0].isStart).toBe(false);
    expect(segThis[0].isEnd).toBe(true);
  });

  it("segments cross month boundaries per month/week", () => {
    // Fri Aug 28 -> Tue Sep 1 inclusive
    const e = ev({ id: 2, start: "2026-08-28", end: "2026-09-02", all_day: true });
    const augWeek = startOfWeek(new Date(2026, 7, 24), 1); // Mon Aug 24
    const sepWeek = startOfWeek(new Date(2026, 7, 31), 1); // Mon Aug 31
    const aug = layoutMultiDayEventsForWeek([e], augWeek, 1);
    const sep = layoutMultiDayEventsForWeek([e], sepWeek, 1);
    expect(aug[0].startCol).toBe(4); // Fri
    expect(aug[0].endCol).toBe(6); // Sun
    expect(aug[0].isStart).toBe(true);
    expect(aug[0].isEnd).toBe(false);
    expect(sep[0].startCol).toBe(0); // Mon
    expect(sep[0].endCol).toBe(1); // Tue
    expect(sep[0].isStart).toBe(false);
    expect(sep[0].isEnd).toBe(true);
  });

  it("assigns different lanes to overlapping all-day events", () => {
    const a = ev({ id: 10, start: "2026-08-03", end: "2026-08-06", all_day: true });
    const b = ev({ id: 11, start: "2026-08-04", end: "2026-08-05", all_day: true });
    const segs = layoutMultiDayEventsForWeek([a, b], new Date(2026, 7, 3), 1);
    expect(segs).toHaveLength(2);
    expect(segs[0].lane).not.toBe(segs[1].lane);
  });

  it("reuses a lane for non-overlapping events (consistent stacking)", () => {
    const a = ev({ id: 20, start: "2026-08-03", end: "2026-08-04", all_day: true });
    const b = ev({ id: 21, start: "2026-08-05", end: "2026-08-06", all_day: true });
    const segs = layoutMultiDayEventsForWeek([a, b], new Date(2026, 7, 3), 1);
    expect(segs[0].lane).toBe(0);
    expect(segs[1].lane).toBe(0);
  });
});

describe("grouping / per-day queries", () => {
  it("groups multi-day events onto each occupied day", () => {
    const allDay = ev({ start: "2026-08-03", end: "2026-08-06", all_day: true });
    const timed = ev({ start: "2026-08-05T05:00:00.000Z", end: "2026-08-05T06:00:00.000Z" });
    const byDay = groupEventsByDay([allDay, timed]);
    expect(byDay.get("2026-08-03")?.length).toBe(1);
    expect(byDay.get("2026-08-05")?.length).toBe(2);
    expect(byDay.get("2026-08-06")).toBeUndefined(); // exclusive end
  });

  it("splits timed vs all-day per day", () => {
    const allDay = ev({ start: "2026-08-05", end: "2026-08-06", all_day: true });
    const timed = ev({ start: "2026-08-05T05:00:00.000Z", end: "2026-08-05T06:00:00.000Z" });
    expect(getAllDayEventsForDayKey([allDay, timed], "2026-08-05").length).toBe(1);
    expect(getTimedEventsForDayKey([allDay, timed], "2026-08-05").length).toBe(1);
  });
});

describe("overflow & density", () => {
  it("computes +N overflow", () => {
    const many = Array.from({ length: 6 }, (_, i) =>
      ev({ id: i, start: "2026-08-05", all_day: true })
    );
    expect(getOverflowCount(many, 3)).toBe(3);
    expect(getOverflowCount(many, 9)).toBe(0);
  });

  it("year density counts all events on a day", () => {
    const d = new Date(2026, 7, 5, 12, 0, 0);
    const timed = ev({ start: "2026-08-05T05:00:00.000Z", end: "2026-08-05T06:00:00.000Z" });
    const allDay = ev({ start: "2026-08-04", end: "2026-08-06", all_day: true });
    expect(getEventCountForDate([timed, allDay], d)).toBe(2);
  });

  it("year density only reflects currently filtered calendars", () => {
    const a = ev({
      calendar_id: 1,
      start: "2026-08-05T05:00:00.000Z",
      end: "2026-08-05T06:00:00.000Z",
    });
    const b = ev({
      calendar_id: 2,
      start: "2026-08-05T07:00:00.000Z",
      end: "2026-08-05T08:00:00.000Z",
    });
    const d = new Date(2026, 7, 5, 12, 0, 0);
    expect(getEventCountForDate([a, b], d)).toBe(2);
    expect(getEventCountForDate([a], d)).toBe(1);
    const density = eventDensityForDate([a, b], d);
    expect(density.count).toBe(2);
  });

  it("caps density colors at three, preserving calendar colors", () => {
    const events = Array.from({ length: 5 }, (_, i) =>
      ev({ id: i, start: "2026-08-05", all_day: true })
    );
    const density = eventDensityForDate(events, new Date(2026, 7, 5, 12));
    expect(density.count).toBe(5);
    expect(density.colors.length).toBeLessThanOrEqual(3);
  });
});
