import type { AppConfig, Calendar, CalEvent, ThemeColors } from "./store";

// ---------------------------------------------------------------------------
// Browser preview mode (no Tauri runtime).
//
// When the frontend is served by plain Vite (http://localhost:1420) outside the
// Tauri webview, `window.__TAURI_INTERNALS__` is undefined. In that case the
// store falls back to these demo fixtures + stub IPC handlers so the UI can be
// inspected in a regular browser. The real Tauri path is untouched.
// ---------------------------------------------------------------------------

export function isTauri(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ !== "undefined"
  );
}

const pad = (n: number) => (n < 10 ? `0${n}` : String(n));

function localDateKey(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

function demoColors(): ThemeColors {
  const colors = [
    "#2e3440", // color0 background
    "#bf616a", // color1 red
    "#a3be8c", // color2 green
    "#ebcb8b", // color3 yellow
    "#81a1c1", // color4 blue/accent
    "#b48ead", // color5 magenta
    "#88c0d0", // color6 cyan
    "#d8dee9", // color7 foreground
    "#4c566a", // color8 muted
    "#bf616a", // color9 bright red
    "#a3be8c", // color10 bright green
    "#ebcb8b", // color11 bright yellow
    "#81a1c1", // color12 bright blue
    "#b48ead", // color13 bright magenta
    "#8fbcbb", // color14 bright cyan
    "#d8dee9", // color15 bright foreground
  ];
  return {
    accent: "#81a1c1",
    foreground: "#d8dee9",
    background: "#2e3440",
    cursor: "#d8dee9",
    selection_foreground: "#2e3440",
    selection_background: "#434c5e",
    muted: "#4c566a",
    colors,
    light: false,
    name: "nord",
  };
}

export function buildMockSnapshot(): {
  config: AppConfig;
  calendars: Calendar[];
  events: CalEvent[];
  pending_invites: CalEvent[];
  theme: ThemeColors;
  last_sync: string | null;
  last_sync_error: string | null;
  default_calendar_id: number;
} {
  const calendars: Calendar[] = [
    {
      id: 1,
      account_id: "demo",
      href: "/demo/personal/",
      displayname: "Personal",
      color: "#82fb9c",
      visible: true,
      readonly: false,
      subscribed: true,
      sort_order: 0,
    },
    {
      id: 2,
      account_id: "demo",
      href: "/demo/work/",
      displayname: "Work",
      color: "#6cc4ff",
      visible: true,
      readonly: false,
      subscribed: true,
      sort_order: 1,
    },
    {
      id: 3,
      account_id: "demo",
      href: "/demo/family/",
      displayname: "Family",
      color: "#ffb86b",
      visible: true,
      readonly: false,
      subscribed: true,
      sort_order: 2,
    },
    {
      id: 4,
      account_id: "demo",
      href: "/demo/archive/",
      displayname: "Archive",
      color: "#8b949e",
      visible: false,
      readonly: true,
      subscribed: true,
      sort_order: 3,
    },
  ];

  const now = new Date();
  const y = now.getFullYear();
  const m = now.getMonth();
  const D = (n: number, h = 0, min = 0) => new Date(y, m, n, h, min, 0, 0);
  const lastDayOfMonth = new Date(y, m + 1, 0).getDate();
  const colorFor = (id: number) => calendars.find((c) => c.id === id)!.color;

  let uid = 0;
  function ev(
    calendarId: number,
    title: string,
    start: Date,
    end: Date | null,
    opts: { all_day?: boolean; description?: string; location?: string } = {}
  ): CalEvent {
    uid += 1;
    return {
      id: uid,
      calendar_id: calendarId,
      href: `/demo/evt-${uid}.ics`,
      etag: null,
      uid: `demo-${uid}`,
      title,
      description: opts.description ?? "",
      location: opts.location ?? "",
      start: opts.all_day ? localDateKey(start) : start.toISOString(),
      end: end ? (opts.all_day ? localDateKey(end) : end.toISOString()) : null,
      all_day: !!opts.all_day,
      rrule: null,
      color: colorFor(calendarId),
      calendar_name: calendars.find((c) => c.id === calendarId)!.displayname,
      status: null,
      organizer: null,
      attendees: [],
      alarms: [],
      my_partstat: null,
      readonly: false,
    };
  }

  const events: CalEvent[] = [
    // Dense weekday (day 5) — exercises compact rows + overflow popover.
    ev(1, "NOGI Morning", D(5, 7, 0), D(5, 8, 0)),
    ev(1, "Deep Work", D(5, 8, 30), D(5, 11, 0)),
    ev(2, "Lunch with team", D(5, 12, 0), D(5, 13, 0), { location: "Köket" }),
    ev(2, "Lotus Tech Sync", D(5, 14, 0), D(5, 15, 0)),
    ev(2, "Board working meeting", D(5, 15, 0), D(5, 16, 30)),
    ev(1, "NOGI Evening", D(5, 19, 0), D(5, 20, 0)),
    // Midnight-crossing timed event (ends 07:00 next day, no reason to break).
    ev(3, "SUP med november", D(5, 22, 0), D(6, 7, 0)),
    // Multi-day project week covering "today" if it falls in this month.
    ev(1, "Planeringsvecka", D(10, 12), D(15, 12), {
      all_day: true,
      description: "Årlig planeringsvecka",
    }),
    ev(2, "Daglig planering", D(10, 9, 0), D(10, 9, 30)),
    // All-day trip across a single week (Tue–Thu).
    ev(3, "Svensjöl – Blekinge", D(18, 12), D(21, 12), {
      all_day: true,
      description: "Family trip",
    }),
    // All-day crossing Sunday -> Monday (weekday-agnostic by month).
    ev(3, "Helgresa", D(22, 12), D(25, 12), { all_day: true }),
    // All-day crossing the month boundary.
    ev(2, "Fakturering", D(lastDayOfMonth, 12), D(3, 12, 0), { all_day: true }),
  ];

  const config: AppConfig = {
    locale: { timezone: "Europe/Stockholm", time_24h: true, week_starts_on: 1 },
    sync_interval_secs: 600,
    accounts: [
      {
        id: "demo",
        display_name: "Demo Account",
        caldav_url: "https://demo.example.com/remote.php/dav/",
        username: "demo",
        addresses: ["demo@example.com"],
        enabled: true,
      },
    ],
  };

  return {
    config,
    calendars,
    events,
    pending_invites: [
      ev(2, "Gästföreläsning: WebDAV i praktiken", D(18, 13, 0), D(18, 14, 0), {
        description: "Invitation",
      }),
    ],
    theme: demoColors(),
    last_sync: null,
    last_sync_error: null,
    default_calendar_id: 1,
  };
}

let snapshot = buildMockSnapshot();

export function resetMockSnapshot(): void {
  snapshot = buildMockSnapshot();
}

// Minimal stub handlers so store actions don't crash during browser preview.
export async function mockInvoke<T>(cmd: string, args?: unknown): Promise<T> {
  switch (cmd) {
    case "get_snapshot":
      return snapshot as unknown as T;
    case "search_events":
      return searchMockEvents(String((args as { query: string }).query)) as unknown as T;
    case "sync_now":
      return undefined as unknown as T;
    case "set_calendar_visible":
    case "set_calendar_color":
    case "set_default_calendar":
    case "set_calendar_subscribed":
    case "reorder_calendars":
    case "save_event":
    case "save_event_occurrence":
    case "delete_event":
    case "delete_event_occurrence":
    case "respond_invite":
    case "respond_invites_bulk":
      return undefined as unknown as T;
    case "take_pending_imports":
      return [] as unknown as T;
    case "test_account":
      return [] as unknown as T;
    case "remove_account":
      return undefined as unknown as T;
    case "add_account": {
      const a = args as { req: { display_name?: string } } | undefined;
      return {
        id: `demo-${Date.now()}`,
        display_name: a?.req.display_name ?? "Demo Account",
        caldav_url: "",
        username: "",
        addresses: [],
        enabled: true,
      } as unknown as T;
    }
    case "save_config":
    case "get_config":
      return snapshot.config as unknown as T;
    case "preview_ics":
      throw new Error("ICS import is unavailable in browser preview mode");
    default:
      throw new Error(`Unsupported command in browser preview mode: ${cmd}`);
  }
}

function searchMockEvents(query: string): CalEvent[] {
  const q = query.toLowerCase();
  return snapshot.events.filter((e) =>
    [e.title, e.description, e.location].some((s) => s.toLowerCase().includes(q))
  );
}
