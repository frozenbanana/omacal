import type { CalEvent } from "./store";
import { DateTime } from "luxon";

// ---------------------------------------------------------------------------
// Shared calendar event layout helpers.
//
// Config-driven timezone: when `timeZone` is provided (e.g. "Europe/Stockholm"),
// all wall-time calculations use that IANA zone via Luxon, matching
// FullCalendar's `timeZone` plugin. Without it we fall back to browser LOCAL
// time (legacy, and current Vitest TZ=Europe/Stockholm).
// All-day events are stored as date-only "YYYY-MM-DD" strings by the backend;
// those are interpreted as calendar dates in the target zone (never UTC) to
// avoid off-by-one day shifts. Day arithmetic uses calendar fields (y/m/d), not
// raw milliseconds, so DST transitions cannot break day counting.
// ---------------------------------------------------------------------------

const MS_DAY = 86400000;
const DATE_RE = /^(\d{4})-(\d{2})-(\d{2})$/;

// FullCalendar's default `nextDayThreshold` — kept for parity/documentation.
export const NEXT_DAY_THRESHOLD_MS = 9 * 60 * 60 * 1000;
export const DEFAULT_WEEK_STARTS_ON = 1; // Monday

export function pad2(n: number): string {
  return n < 10 ? `0${n}` : String(n);
}

/** Calendar-day key "YYYY-MM-DD" in target zone (or local if omitted). */
export function dayKey(d: Date, timeZone?: string): string {
  if (timeZone) {
    const dt = DateTime.fromJSDate(d, { zone: timeZone });
    if (dt.isValid) return dt.toFormat("yyyy-MM-dd");
  }
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
}

/** Midnight for a Date in target zone (or local). */
export function toLocalMidnight(d: Date, timeZone?: string): Date {
  if (timeZone) {
    const dt = DateTime.fromJSDate(d, { zone: timeZone }).startOf("day");
    if (dt.isValid) return dt.toJSDate();
  }
  return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

/** Noon timestamp for a day key (avoids DST midnight ambiguity) — zone-aware. */
export function parseDayKey(key: string, timeZone?: string): Date {
  const [y, m, d] = key.split("-").map(Number);
  if (timeZone) {
    const dt = DateTime.fromObject({ year: y, month: m, day: d, hour: 12 }, { zone: timeZone });
    if (dt.isValid) return dt.toJSDate();
  }
  return new Date(y, m - 1, d, 12, 0, 0, 0);
}

/** Add days using calendar arithmetic in zone (DST-safe). */
export function addCalendarDays(date: Date, n: number, timeZone?: string): Date {
  if (timeZone) {
    const dt = DateTime.fromJSDate(date, { zone: timeZone }).plus({ days: n }).startOf("day");
    if (dt.isValid) return dt.toJSDate();
  }
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + n);
}

/** DST-safe whole-calendar-day difference between two dates. */
export function calendarDaysBetween(a: Date, b: Date, timeZone?: string): number {
  if (timeZone) {
    const da = DateTime.fromJSDate(a, { zone: timeZone });
    const db = DateTime.fromJSDate(b, { zone: timeZone });
    if (da.isValid && db.isValid) {
      const ua = Date.UTC(da.year, da.month - 1, da.day);
      const ub = Date.UTC(db.year, db.month - 1, db.day);
      return Math.round((ub - ua) / MS_DAY);
    }
  }
  const ua = Date.UTC(a.getFullYear(), a.getMonth(), a.getDate());
  const ub = Date.UTC(b.getFullYear(), b.getMonth(), b.getDate());
  return Math.round((ub - ua) / MS_DAY);
}

export function addDayKey(key: string, n = 1, timeZone?: string): string {
  return dayKey(addCalendarDays(parseDayKey(key, timeZone), n, timeZone), timeZone);
}

/**
 * Parse a backend event start/end string into a Date.
 * All-day date-only strings become midnight in the target zone; anything else
 * is parsed as an ISO instant and its zone-adjusted components are used
 * (matches FullCalendar timeZone behaviour).
 */
export function parseLocalDate(
  value: string | null | undefined,
  allDay: boolean,
  timeZone?: string
): Date | null {
  if (!value) return null;
  const m = DATE_RE.exec(value.trim());
  if (m && allDay) {
    if (timeZone) {
      const dt = DateTime.fromObject(
        { year: +m[1], month: +m[2], day: +m[3], hour: 0, minute: 0, second: 0 },
        { zone: timeZone }
      );
      if (dt.isValid) return dt.toJSDate();
    }
    return new Date(+m[1], +m[2] - 1, +m[3], 0, 0, 0, 0);
  }
  // For timed, keep instant but caller should use timeZone-aware formatters
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return null;
  // If timeZone is set, the Date instant is the same; dayKey/eventTimeLabel
  // will convert via Luxon. No need to shift here.
  return d;
}

/**
 * Inclusive local day range [startKey, endKey] an event occupies, or null.
 *
 * End semantics match FullCalendar: DTEND is EXCLUSIVE. A timed event ending
 * exactly at local midnight does not occupy that day. A zero/negative-length
 * event degrades to a single day (+1h timed / +1 day all-day).
 */
export function eventShowDays(
  ev: CalEvent,
  timeZone?: string
): { startKey: string; endKey: string } | null {
  const start = parseLocalDate(ev.start, ev.all_day, timeZone);
  if (!start) return null;

  const parsedEnd = parseLocalDate(ev.end, ev.all_day, timeZone);
  const endExclusive =
    parsedEnd && parsedEnd.getTime() > start.getTime()
      ? parsedEnd
      : ev.all_day
        ? addCalendarDays(start, 1, timeZone)
        : new Date(start.getTime() + 60 * 60 * 1000);

  const startKey = dayKey(start, timeZone);
  let endKey: string;
  if (ev.all_day) {
    endKey = dayKey(addCalendarDays(endExclusive, -1, timeZone), timeZone);
  } else {
    // last occupied day = zone day of (end - 1ms)
    endKey = dayKey(toLocalMidnight(new Date(endExclusive.getTime() - 1), timeZone), timeZone);
  }
  if (endKey < startKey) endKey = startKey;
  return { startKey, endKey };
}

/** Every zone day key an event occupies. */
export function occupiedDayKeys(ev: CalEvent, timeZone?: string): string[] {
  const b = eventShowDays(ev, timeZone);
  if (!b) return [];
  const keys: string[] = [];
  let cur = b.startKey;
  while (cur <= b.endKey) {
    keys.push(cur);
    cur = addDayKey(cur, 1, timeZone);
  }
  return keys;
}

export function isEventOnDay(ev: CalEvent, date: Date, timeZone?: string): boolean {
  const b = eventShowDays(ev, timeZone);
  if (!b) return false;
  const k = dayKey(date, timeZone);
  return k >= b.startKey && k <= b.endKey;
}

export function isAllDayEvent(ev: CalEvent): boolean {
  return ev.all_day;
}

/** Group events by the zone day keys they occupy. Input must already be calendar-filtered. */
export function groupEventsByDay(events: CalEvent[], timeZone?: string): Map<string, CalEvent[]> {
  const byDay = new Map<string, CalEvent[]>();
  for (const ev of events) {
    for (const k of occupiedDayKeys(ev, timeZone)) {
      const list = byDay.get(k);
      if (list) list.push(ev);
      else byDay.set(k, [ev]);
    }
  }
  return byDay;
}

function eventOccursOnKey(ev: CalEvent, key: string, timeZone?: string): boolean {
  const b = eventShowDays(ev, timeZone);
  return !!b && key >= b.startKey && key <= b.endKey;
}

export function getTimedEventsForDayKey(
  events: CalEvent[],
  key: string,
  timeZone?: string
): CalEvent[] {
  return events.filter((e) => !e.all_day && eventOccursOnKey(e, key, timeZone));
}

export function getTimedEventsForDate(
  events: CalEvent[],
  date: Date,
  timeZone?: string
): CalEvent[] {
  return getTimedEventsForDayKey(events, dayKey(date, timeZone), timeZone);
}

export function getAllDayEventsForDayKey(
  events: CalEvent[],
  key: string,
  timeZone?: string
): CalEvent[] {
  return events.filter((e) => e.all_day && eventOccursOnKey(e, key, timeZone));
}

export function getAllDayEventsForDate(
  events: CalEvent[],
  date: Date,
  timeZone?: string
): CalEvent[] {
  return getAllDayEventsForDayKey(events, dayKey(date, timeZone), timeZone);
}

export function getEventCountForDate(events: CalEvent[], date: Date, timeZone?: string): number {
  const k = dayKey(date, timeZone);
  let n = 0;
  for (const ev of events) if (eventOccursOnKey(ev, k, timeZone)) n += 1;
  return n;
}

/** Count of events on a day's key. */
export function getEventCountForDayKey(events: CalEvent[], key: string, timeZone?: string): number {
  let n = 0;
  for (const ev of events) if (eventOccursOnKey(ev, key, timeZone)) n += 1;
  return n;
}

export function getOverflowCount(eventsOnDay: CalEvent[], maxRows: number): number {
  if (!Number.isFinite(maxRows) || maxRows < 0) return 0;
  return Math.max(0, eventsOnDay.length - maxRows);
}

// ---------------------------------------------------------------------------
// Multi-day / all-day week-aware segments (used by Month pill bars and Year
// mini-month spans).
// ---------------------------------------------------------------------------

export interface WeekSeg {
  ev: CalEvent;
  /** inclusive local day keys of the whole event */
  startKey: string;
  endKey: string;
  /** inclusive columns within [startCol..endCol] (0..6) of this week row */
  startCol: number;
  endCol: number;
  /** true when this week's segment contains the event's true start/end */
  isStart: boolean;
  isEnd: boolean;
  /** stacking lane (0 = top) within this week row */
  lane: number;
}

export function startOfWeek(
  date: Date,
  weekStartsOn = DEFAULT_WEEK_STARTS_ON,
  timeZone?: string
): Date {
  const d = toLocalMidnight(date, timeZone);
  let weekday: number;
  if (timeZone) {
    const dt = DateTime.fromJSDate(d, { zone: timeZone });
    weekday = dt.weekday % 7; // 1=Mon ..7=Sun -> 0=Sun
    if (weekday === 7) weekday = 0;
  } else {
    weekday = d.getDay();
  }
  const off = (weekday - weekStartsOn + 7) % 7;
  return addCalendarDays(d, -off, timeZone);
}

/**
 * Compute one week row's all-day/multi-day segments with lane assignment.
 * Input must already be calendar-filtered. Only all-day events take lanes.
 */
export function layoutMultiDayEventsForWeek(
  events: CalEvent[],
  weekStart: Date,
  weekStartsOn = DEFAULT_WEEK_STARTS_ON
): WeekSeg[] {
  // Use local-noon anchors everywhere so comparisons are time-of-day agnostic.
  const ws = parseDayKey(dayKey(startOfWeek(weekStart, weekStartsOn)));
  const weekLast = parseDayKey(addDayKey(dayKey(ws), 6));

  interface Pending {
    ev: CalEvent;
    startKey: string;
    endKey: string;
    segStart: Date;
    segEnd: Date;
    isStart: boolean;
    isEnd: boolean;
    startCol: number;
    endCol: number;
  }

  const pending: Pending[] = [];
  for (const ev of events) {
    if (!ev.all_day) continue;
    const b = eventShowDays(ev);
    if (!b) continue;
    const gStart = parseDayKey(b.startKey);
    const gEnd = parseDayKey(b.endKey);
    const segStart = gStart.getTime() < ws.getTime() ? ws : gStart;
    const segEnd = gEnd.getTime() > weekLast.getTime() ? weekLast : gEnd;
    if (segEnd.getTime() < segStart.getTime()) continue;
    pending.push({
      ev,
      startKey: b.startKey,
      endKey: b.endKey,
      segStart,
      segEnd,
      isStart: segStart.getTime() === gStart.getTime(),
      isEnd: segEnd.getTime() === gEnd.getTime(),
      startCol: calendarDaysBetween(ws, segStart),
      endCol: calendarDaysBetween(ws, segEnd),
    });
  }

  // Greedy lane assignment, preserving input order (stable stacking).
  const lanes: Pending[][] = [];
  const result: WeekSeg[] = [];
  for (const p of pending) {
    let lane = 0;
    for (; lane < lanes.length; lane += 1) {
      const last = lanes[lane][lanes[lane].length - 1];
      if (last.segEnd.getTime() < p.segStart.getTime()) break;
    }
    if (lane === lanes.length) lanes.push([]);
    lanes[lane].push(p);
    result.push({
      ev: p.ev,
      startKey: p.startKey,
      endKey: p.endKey,
      startCol: p.startCol,
      endCol: p.endCol,
      isStart: p.isStart,
      isEnd: p.isEnd,
      lane,
    });
  }
  return result;
}

/** Distinct event colors among a day's events (first 3), for density dots. */
export function eventDensityForDate(
  events: CalEvent[],
  date: Date,
  timeZone?: string
): { count: number; colors: string[] } {
  const k = dayKey(date, timeZone);
  const colors: string[] = [];
  let count = 0;
  for (const ev of events) {
    if (!eventOccursOnKey(ev, k, timeZone)) continue;
    count += 1;
    if (colors.length < 3 && !colors.includes(ev.color)) colors.push(ev.color);
  }
  return { count, colors };
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

export function eventTimeLabel(ev: CalEvent, time24h: boolean, timeZone?: string): string {
  if (ev.all_day) return "";
  if (timeZone) {
    const dt = ev.start ? DateTime.fromISO(ev.start, { setZone: true }).setZone(timeZone) : null;
    if (dt && dt.isValid) {
      if (time24h) return dt.toFormat("HH:mm");
      return dt.toFormat("h:mm a");
    }
  }
  const start = parseLocalDate(ev.start, false, timeZone);
  if (!start) return "";
  const h = timeZone ? DateTime.fromJSDate(start, { zone: timeZone }).hour : start.getHours();
  const m = timeZone ? DateTime.fromJSDate(start, { zone: timeZone }).minute : start.getMinutes();
  if (time24h) return `${pad2(h)}:${pad2(m)}`;
  const period = h >= 12 ? "PM" : "AM";
  const h12 = h % 12 || 12;
  return `${h12}:${pad2(m)} ${period}`;
}

export function formatDayLong(date: Date, locale = "en-GB"): string {
  return new Intl.DateTimeFormat(locale, {
    weekday: "long",
    month: "long",
    day: "numeric",
  }).format(date);
}

export function formatMonthTitle(date: Date, locale = "en-GB"): string {
  return new Intl.DateTimeFormat(locale, { month: "long", year: "numeric" }).format(date);
}
