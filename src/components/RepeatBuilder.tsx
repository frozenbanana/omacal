import { useEffect, useMemo, useState } from "react";
import { DateTime } from "luxon";

type Day = "MO" | "TU" | "WE" | "TH" | "FR" | "SA" | "SU";
const DAYS: Day[] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];
const DAY_LABEL: Record<Day, string> = {
  MO: "Mon",
  TU: "Tue",
  WE: "Wed",
  TH: "Thu",
  FR: "Fri",
  SA: "Sat",
  SU: "Sun",
};
const DAY_ISO: Record<Day, number> = { MO: 1, TU: 2, WE: 3, TH: 4, FR: 5, SA: 6, SU: 7 };

export type RepeatEnds =
  { mode: "never" } | { mode: "on"; until: string } | { mode: "after"; count: number };

export interface RepeatValue {
  freq: "DAILY" | "WEEKLY";
  interval: number;
  byday: Day[]; // only for WEEKLY
  ends: RepeatEnds;
}

function weekdayFromDTStart(dtstart: string, timeZone?: string): Day {
  // dtstart is wall "YYYY-MM-DDTHH:mm" or RFC3339
  let dt: DateTime | null = null;
  if (dtstart.includes("T")) {
    dt = DateTime.fromISO(dtstart, { zone: timeZone || undefined });
    if (!dt.isValid)
      dt = DateTime.fromFormat(dtstart.slice(0, 16), "yyyy-MM-dd'T'HH:mm", { zone: timeZone });
    if (!dt.isValid) dt = DateTime.fromISO(dtstart, { setZone: true });
  }
  if (!dt || !dt.isValid) dt = DateTime.now().setZone(timeZone || "Europe/Stockholm");
  const wd = dt.weekday; // 1 Mon ..7 Sun
  const map: Record<number, Day> = {
    1: "MO",
    2: "TU",
    3: "WE",
    4: "TH",
    5: "FR",
    6: "SA",
    7: "SU",
  };
  return map[wd] || "MO";
}

export function parseRRule(
  rrule: string | null | undefined,
  dtstart: string,
  timeZone?: string
): RepeatValue | null {
  if (!rrule || !rrule.trim()) return null;
  const raw = rrule.trim();
  const clean = raw.toUpperCase().startsWith("RRULE:") ? raw.slice(6) : raw;
  const parts = Object.fromEntries(
    clean
      .split(";")
      .map((p) => {
        const [k, v] = p.split("=");
        return [k?.toUpperCase(), v];
      })
      .filter(([k]) => !!k)
  );
  const freq = (parts.FREQ as RepeatValue["freq"]) || "WEEKLY";
  if (freq !== "DAILY" && freq !== "WEEKLY") {
    // For unsupported freqs (MONTHLY/YEARLY), fallback to raw preserved? For v1 we treat as weekly custom preserved
    // Return null to indicate no builder support -> keep raw string as is in editor (fallback)
    // But we still try to parse weekly/daily only
    return null;
  }
  const interval = Math.max(1, Math.min(99, parseInt(parts.INTERVAL || "1", 10) || 1));
  let byday: Day[] = [];
  if (freq === "WEEKLY") {
    if (parts.BYDAY) {
      byday = parts.BYDAY.split(",")
        .map((s: string) => s.trim().toUpperCase() as Day)
        .filter((d: Day) => (DAYS as unknown as string[]).includes(d));
    }
    if (byday.length === 0) byday = [weekdayFromDTStart(dtstart, timeZone)];
  }
  let ends: RepeatEnds = { mode: "never" };
  if (parts.COUNT) {
    const c = parseInt(parts.COUNT, 10);
    if (c > 0 && c <= 999) ends = { mode: "after", count: c };
  } else if (parts.UNTIL) {
    // UNTIL may be YYYYMMDD or YYYYMMDDTHHMMSSZ
    const untilIso = parts.UNTIL;
    let dt: DateTime | null = null;
    if (/^\d{8}T\d{6}Z$/.test(untilIso)) {
      dt = DateTime.fromFormat(untilIso, "yyyyMMdd'T'HHmmss'Z'", { zone: "utc" });
      if (dt.isValid) dt = dt.setZone(timeZone || "Europe/Stockholm");
    } else if (/^\d{8}$/.test(untilIso)) {
      dt = DateTime.fromFormat(untilIso, "yyyyMMdd", { zone: timeZone || "Europe/Stockholm" });
    } else if (untilIso.includes("T")) {
      dt = DateTime.fromFormat(untilIso, "yyyyMMdd'T'HHmmss", { zone: timeZone });
    }
    if (dt && dt.isValid) {
      ends = { mode: "on", until: dt.toFormat("yyyy-MM-dd") };
    } else {
      ends = { mode: "on", until: untilIso.slice(0, 10) };
    }
  }
  return { freq, interval, byday, ends };
}

export function buildRRule(
  v: RepeatValue | null,
  _dtstart?: string,
  timeZone?: string,
  allDay?: boolean
): string | null {
  if (!v) return null;
  const parts: string[] = [`FREQ=${v.freq}`];
  if (v.interval > 1) parts.push(`INTERVAL=${v.interval}`);
  if (v.freq === "WEEKLY" && v.byday.length > 0) {
    // Sort by ISO weekday order for stability
    const sorted = [...v.byday].sort((a, b) => DAY_ISO[a] - DAY_ISO[b]);
    parts.push(`BYDAY=${sorted.join(",")}`);
  }
  if (v.ends.mode === "after") {
    parts.push(`COUNT=${v.ends.count}`);
  } else if (v.ends.mode === "on") {
    let untilStr = v.ends.until;
    // until is yyyy-MM-dd from date input
    if (allDay) {
      const dt = DateTime.fromISO(untilStr, { zone: timeZone });
      if (dt.isValid) untilStr = dt.toFormat("yyyyMMdd");
      else untilStr = untilStr.replace(/-/g, "");
      parts.push(`UNTIL=${untilStr}`);
    } else {
      // For timed, we need to emit UNTIL at end of that day in DTSTART wall time?
      // Emit as wall date's end? For minimal we emit YYYYMMDDT235959Z? But to keep wall semantics,
      // we emit UNTIL as that date at 23:59:59 in event's timezone, converted to UTC if DTSTART is UTC,
      // else wall. Since DTSTART is wall TZID, UNTIL can be UTC.
      // For simplicity, emit date at 23:59:59 in timezone and convert to UTC Z for timed events.
      const dt = DateTime.fromISO(untilStr, { zone: timeZone || "Europe/Stockholm" }).endOf("day");
      if (dt.isValid) {
        // If DTSTART is wall TZID, we will later in ics.rs handle UNTIL wall vs UTC, but here we emit UTC Z
        // For timed events we emit UTC Z equivalent of end of day wall
        const utc = dt.setZone("utc");
        untilStr = utc.toFormat("yyyyMMdd'T'HHmmss'Z'");
        parts.push(`UNTIL=${untilStr}`);
      } else {
        parts.push(`UNTIL=${untilStr.replace(/-/g, "")}T235959Z`);
      }
    }
  }
  return parts.join(";");
}

export function humanize(v: RepeatValue | null): string {
  if (!v) return "Does not repeat";
  const every = v.interval === 1 ? "Every" : `Every ${v.interval}`;
  if (v.freq === "DAILY") {
    const base = v.interval === 1 ? "Daily" : `${every} days`;
    if (v.ends.mode === "after") return `${base}, ${v.ends.count} times`;
    if (v.ends.mode === "on") return `${base} until ${v.ends.until}`;
    return base;
  }
  // WEEKLY
  const days =
    v.byday.length === 5 &&
    v.byday.includes("MO" as Day) &&
    v.byday.includes("TU" as Day) &&
    v.byday.includes("WE" as Day) &&
    v.byday.includes("TH" as Day) &&
    v.byday.includes("FR" as Day) &&
    !v.byday.includes("SA") &&
    !v.byday.includes("SU")
      ? "weekday"
      : v.byday.map((d) => DAY_LABEL[d]).join(", ");
  const base = v.interval === 1 ? `Weekly on ${days}` : `${every} weeks on ${days}`;
  if (v.ends.mode === "after") return `${base}, ${v.ends.count} times`;
  if (v.ends.mode === "on") return `${base} until ${v.ends.until}`;
  return base;
}

export function RepeatBuilder(props: {
  rrule: string | null | undefined;
  dtstart: string;
  allDay?: boolean;
  timezone?: string;
  onChange: (rrule: string | null) => void;
}) {
  const { rrule, dtstart, allDay, timezone, onChange } = props;
  const initialParsed = useMemo(
    () => parseRRule(rrule || null, dtstart || "", timezone),
    [rrule, dtstart, timezone]
  );
  // Track whether RRULE was unsupported (parse returned null but rrule non-empty)
  const isUnsupported = !!rrule && !initialParsed;
  const [enabled, setEnabled] = useState<boolean>(() => !!rrule);
  const [value, setValue] = useState<RepeatValue | null>(() => initialParsed);
  const [rawFallback, setRawFallback] = useState<string>(() => (isUnsupported ? rrule || "" : ""));

  useEffect(() => {
    const p = parseRRule(rrule || null, dtstart || "", timezone);
    if (rrule && !p) {
      setEnabled(true);
      setRawFallback(rrule);
      setValue(null);
    } else {
      setRawFallback("");
      setValue(p);
      setEnabled(!!p);
    }
  }, [rrule, dtstart, timezone]);

  const push = (next: RepeatValue | null) => {
    setValue(next);
    onChange(next ? buildRRule(next, dtstart, timezone, allDay) : null);
  };

  const toggleEnabled = (nextEnabled: boolean) => {
    setEnabled(nextEnabled);
    if (!nextEnabled) {
      onChange(null);
      setValue(null);
    } else {
      const defDay = weekdayFromDTStart(dtstart, timezone);
      const def: RepeatValue = {
        freq: "WEEKLY",
        interval: 1,
        byday: [defDay],
        ends: { mode: "never" },
      };
      push(def);
    }
  };

  if (isUnsupported) {
    // Fallback raw editor for unsupported RRULE (e.g. MONTHLY)
    return (
      <div style={{ display: "grid", gap: "0.4rem" }}>
        <label className="cal-row" style={{ textTransform: "none" }}>
          <input
            type="checkbox"
            checked={enabled}
            onChange={(e) => toggleEnabled(e.target.checked)}
          />{" "}
          Repeat
        </label>
        {enabled && (
          <>
            <div className="muted" style={{ fontSize: "0.72rem" }}>
              Custom RRULE (unsupported frequency — edit raw)
            </div>
            <input
              value={rawFallback}
              onChange={(e) => {
                setRawFallback(e.target.value);
                onChange(e.target.value.trim() || null);
              }}
              placeholder="FREQ=WEEKLY;BYDAY=MO,WE"
            />
          </>
        )}
      </div>
    );
  }

  return (
    <div style={{ display: "grid", gap: "0.5rem" }}>
      <label className="cal-row" style={{ textTransform: "none" }}>
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => toggleEnabled(e.target.checked)}
        />{" "}
        Repeat
      </label>
      {enabled && value && (
        <>
          <div className="form-row" style={{ gridTemplateColumns: "1fr 1fr" }}>
            <label>
              Frequency
              <select
                value={value.freq}
                onChange={(e) => push({ ...value, freq: e.target.value as RepeatValue["freq"] })}
              >
                <option value="DAILY">Daily</option>
                <option value="WEEKLY">Weekly</option>
              </select>
            </label>
            <label>
              Every
              <span style={{ display: "flex", gap: "0.35rem", alignItems: "center" }}>
                <input
                  type="number"
                  min={1}
                  max={99}
                  value={value.interval}
                  onChange={(e) =>
                    push({
                      ...value,
                      interval: Math.max(1, Math.min(99, parseInt(e.target.value || "1", 10))),
                    })
                  }
                  style={{ width: "4.5rem" }}
                />
                <span style={{ fontSize: "0.72rem", color: "var(--muted)" }}>
                  {value.freq === "DAILY" ? "day(s)" : "week(s)"}
                </span>
              </span>
            </label>
          </div>
          {value.freq === "WEEKLY" && (
            <div>
              <div className="muted" style={{ fontSize: "0.72rem", marginBottom: "0.2rem" }}>
                On
              </div>
              <div style={{ display: "flex", flexWrap: "wrap", gap: "0.3rem" }}>
                {DAYS.map((d) => {
                  const active = value.byday.includes(d);
                  return (
                    <button
                      key={d}
                      type="button"
                      onClick={() => {
                        const next = active
                          ? value.byday.filter((x) => x !== d)
                          : [...value.byday, d];
                        // Keep at least one
                        if (next.length === 0) return;
                        push({ ...value, byday: next });
                      }}
                      style={{
                        padding: "0.2rem 0.45rem",
                        borderRadius: "999px",
                        border: "1px solid var(--border)",
                        background: active ? "var(--accent)" : "transparent",
                        color: active ? "var(--bg)" : "var(--fg)",
                        fontSize: "0.72rem",
                        fontWeight: active ? 700 : 400,
                      }}
                    >
                      {DAY_LABEL[d]}
                    </button>
                  );
                })}
                <button
                  type="button"
                  onClick={() => push({ ...value, byday: ["MO", "TU", "WE", "TH", "FR"] })}
                  style={{
                    padding: "0.2rem 0.45rem",
                    fontSize: "0.68rem",
                    border: "1px dashed var(--border)",
                    background: "transparent",
                    borderRadius: "999px",
                  }}
                >
                  Weekdays
                </button>
              </div>
            </div>
          )}
          <label>
            Ends
            <select
              value={value.ends.mode}
              onChange={(e) => {
                const m = e.target.value as RepeatEnds["mode"];
                if (m === "never") push({ ...value, ends: { mode: "never" } });
                else if (m === "after") push({ ...value, ends: { mode: "after", count: 10 } });
                else
                  push({
                    ...value,
                    ends: {
                      mode: "on",
                      until: DateTime.now().plus({ months: 3 }).toFormat("yyyy-MM-dd"),
                    },
                  });
              }}
            >
              <option value="never">Never</option>
              <option value="on">On date</option>
              <option value="after">After</option>
            </select>
          </label>
          {value.ends.mode === "on" && (
            <label>
              Until
              <input
                type="date"
                value={value.ends.until}
                onChange={(e) => push({ ...value, ends: { mode: "on", until: e.target.value } })}
              />
            </label>
          )}
          {value.ends.mode === "after" && (
            <label>
              Count
              <span style={{ display: "flex", gap: "0.35rem", alignItems: "center" }}>
                <input
                  type="number"
                  min={1}
                  max={999}
                  value={value.ends.count}
                  onChange={(e) =>
                    push({
                      ...value,
                      ends: {
                        mode: "after",
                        count: Math.max(1, Math.min(999, parseInt(e.target.value || "1", 10))),
                      },
                    })
                  }
                  style={{ width: "4.5rem" }}
                />
                <span style={{ fontSize: "0.72rem", color: "var(--muted)" }}>times</span>
              </span>
            </label>
          )}
          <div className="muted" style={{ fontSize: "0.72rem", fontStyle: "italic" }}>
            {humanize(value)}
          </div>
        </>
      )}
    </div>
  );
}
