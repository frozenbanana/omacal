import type { CalEvent } from "./store";

// ---------------------------------------------------------------------------
// Shared calendar event layout helpers.
//
// All date math is LOCAL-time (browser timezone == app rendering timezone,
// matching how FullCalendar renders without an explicit `timeZone` option).
// All-day events are stored as date-only "YYYY-MM-DD" strings by the backend;
// those are interpreted as LOCAL calendar dates (never UTC) to avoid
// off-by-one day shifts. Day arithmetic uses calendar fields (y/m/d), not raw
// milliseconds, so DST transitions cannot break day counting.
// ---------------------------------------------------------------------------

const MS_DAY = 86400000;
const DATE_RE = /^(\d{4})-(\d{2})-(\d{2})$/;

// FullCalendar's default `nextDayThreshold` — kept for parity/documentation.
export const NEXT_DAY_THRESHOLD_MS = 9 * 60 * 60 * 1000;
export const DEFAULT_WEEK_STARTS_ON = 1; // Monday

export function pad2(n: number): string {
  return n < 10 ? `0${n}` : String(n);
}

/** Local calendar-day key "YYYY-MM-DD".  */
export function dayKey(d: Date): string {
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
}

/** Local midnight for a Date. */
export function toLocalMidnight(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

/** Local-noon timestamp for a day key (avoids DST midnight ambiguity). */
export function parseDayKey(key: string): Date {
  const [y, m, d] = key.split("-").map(Number);
  return new Date(y, m - 1, d, 12, 0, 0, 0);
}

/** Add days using local calendar arithmetic (DST-safe). */
export function addCalendarDays(date: Date, n: number): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + n);
}

/** DST-safe whole-calendar-day difference between two dates. */
export function calendarDaysBetween(a: Date, b: Date): number {
  const ua = Date.UTC(a.getFullYear(), a.getMonth(), a.getDate());
  const ub = Date.UTC(b.getFullYear(), b.getMonth(), b.getDate());
  return Math.round((ub - ua) / MS_DAY);
}

export function addDayKey(key: string, n = 1): string {
  return dayKey(addCalendarDays(parseDayKey(key), n));
}

/**
 * Parse a backend event start/end string into a local Date.
 * All-day date-only strings become LOCAL midnight; anything else is parsed as
 * an ISO instant and its local components are used (matches FC behaviour).
 */
export function parseLocalDate(
  value: string | null | undefined,
  allDay: boolean,
): Date | null {
  if (!value) return null;
  const m = DATE_RE.exec(value.trim());
  if (m && allDay) {
    return new Date(+m[1], +m[2] - 1, +m[3], 0, 0, 0, 0);
  }
  const d = new Date(value);
  return Number.isNaN(d.getTime()) ? null : d;
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
): { startKey: string; endKey: string } | null {
  const start = parseLocalDate(ev.start, ev.all_day);
  if (!start) return null;

  const parsedEnd = parseLocalDate(ev.end, ev.all_day);
  const endExclusive =
    parsedEnd && parsedEnd.getTime() > start.getTime()
      ? parsedEnd
      : ev.all_day
        ? addCalendarDays(start, 1)
        : new Date(start.getTime() + 60 * 60 * 1000);

  const startKey = dayKey(start);
  let endKey: string;
  if (ev.all_day) {
    endKey = dayKey(addCalendarDays(endExclusive, -1));
  } else {
    // last occupied day = local day of (end - 1ms)
    endKey = dayKey(toLocalMidnight(new Date(endExclusive.getTime() - 1)));
  }
  if (endKey < startKey) endKey = startKey;
  return { startKey, endKey };
}

/** Every local day key an event occupies. */
export function occupiedDayKeys(ev: CalEvent): string[] {
  const b = eventShowDays(ev);
  if (!b) return [];
  const keys: string[] = [];
  let cur = b.startKey;
  while (cur <= b.endKey) {
    keys.push(cur);
    cur = addDayKey(cur, 1);
  }
  return keys;
}

export function isEventOnDay(ev: CalEvent, date: Date): boolean {
  const b = eventShowDays(ev);
  if (!b) return false;
  const k = dayKey(date);
  return k >= b.startKey && k <= b.endKey;
}

export function isAllDayEvent(ev: CalEvent): boolean {
  return ev.all_day;
}

/** Group events by the local day keys they occupy. Input must already be calendar-filtered. */
export function groupEventsByDay(events: CalEvent[]): Map<string, CalEvent[]> {
  const byDay = new Map<string, CalEvent[]>();
  for (const ev of events) {
    for (const k of occupiedDayKeys(ev)) {
      const list = byDay.get(k);
      if (list) list.push(ev);
      else byDay.set(k, [ev]);
    }
  }
  return byDay;
}

function eventOccursOnKey(ev: CalEvent, key: string): boolean {
  const b = eventShowDays(ev);
  return !!b && key >= b.startKey && key <= b.endKey;
}

export function getTimedEventsForDayKey(events: CalEvent[], key: string): CalEvent[] {
  return events.filter((e) => !e.all_day && eventOccursOnKey(e, key));
}

export function getTimedEventsForDate(events: CalEvent[], date: Date): CalEvent[] {
  return getTimedEventsForDayKey(events, dayKey(date));
}

export function getAllDayEventsForDayKey(events: CalEvent[], key: string): CalEvent[] {
  return events.filter((e) => e.all_day && eventOccursOnKey(e, key));
}

export function getAllDayEventsForDate(events: CalEvent[], date: Date): CalEvent[] {
  return getAllDayEventsForDayKey(events, dayKey(date));
}

export function getEventCountForDate(events: CalEvent[], date: Date): number {
  const k = dayKey(date);
  let n = 0;
  for (const ev of events) if (eventOccursOnKey(ev, k)) n += 1;
  return n;
}

/** Count of events on a day's key. */
export function getEventCountForDayKey(events: CalEvent[], key: string): number {
  let n = 0;
  for (const ev of events) if (eventOccursOnKey(ev, key)) n += 1;
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

export function startOfWeek(date: Date, weekStartsOn = DEFAULT_WEEK_STARTS_ON): Date {
  const d = toLocalMidnight(date);
  const diff = (d.getDay() - weekStartsOn + 7) % 7;
  return addCalendarDays(d, -diff);
}

/**
 * Compute one week row's all-day/multi-day segments with lane assignment.
 * Input must already be calendar-filtered. Only all-day events take lanes.
 */
export function layoutMultiDayEventsForWeek(
  events: CalEvent[],
  weekStart: Date,
  weekStartsOn = DEFAULT_WEEK_STARTS_ON,
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
): { count: number; colors: string[] } {
  const k = dayKey(date);
  const colors: string[] = [];
  let count = 0;
  for (const ev of events) {
    if (!eventOccursOnKey(ev, k)) continue;
    count += 1;
    if (colors.length < 3 && !colors.includes(ev.color)) colors.push(ev.color);
  }
  return { count, colors };
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

export function eventTimeLabel(ev: CalEvent, time24h: boolean): string {
  if (ev.all_day) return "";
  const start = parseLocalDate(ev.start, false);
  if (!start) return "";
  const h = start.getHours();
  const m = start.getMinutes();
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
