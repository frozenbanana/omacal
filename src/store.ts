import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { isTauri, mockInvoke } from "./demoData";

async function safeInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri()) return invoke<T>(cmd, args);
  return mockInvoke<T>(cmd, args);
}

export type Attendee = {
  email: string;
  cn?: string | null;
  partstat?: string | null;
  role?: string | null;
  rsvp: boolean;
};

export type Alarm = {
  trigger: string;
  description?: string | null;
};

export type Calendar = {
  id: number;
  account_id: string;
  href: string;
  displayname: string;
  color: string;
  sync_token?: string | null;
  ctag?: string | null;
  visible: boolean;
  readonly: boolean;
  subscribed: boolean;
  sort_order: number;
};

export type CalEvent = {
  id: number;
  calendar_id: number;
  href: string;
  etag?: string | null;
  uid: string;
  title: string;
  description: string;
  location: string;
  start?: string | null;
  end?: string | null;
  all_day: boolean;
  rrule?: string | null;
  color: string;
  calendar_name: string;
  status?: string | null;
  organizer?: string | null;
  attendees: Attendee[];
  alarms: Alarm[];
  my_partstat?: string | null;
  readonly: boolean;
  master_start?: string | null;
  master_end?: string | null;
};

export type Account = {
  id: string;
  display_name: string;
  caldav_url: string;
  username: string;
  addresses: string[];
  enabled: boolean;
};

export type AppConfig = {
  locale: {
    timezone: string;
    time_24h: boolean;
    week_starts_on: number;
  };
  sync_interval_secs: number;
  accounts: Account[];
};

export type ThemeColors = {
  accent: string;
  foreground: string;
  background: string;
  cursor: string;
  selection_foreground: string;
  selection_background: string;
  colors: string[];
  light: boolean;
  name: string;
};

export type EventInput = {
  calendar_id: number;
  uid?: string | null;
  summary: string;
  description: string;
  location: string;
  dtstart: string;
  dtend: string;
  all_day: boolean;
  timezone: string;
  rrule?: string | null;
  alarms: Alarm[];
  attendees: Attendee[];
  href?: string | null;
  etag?: string | null;
};

export type ImportPreview = {
  summary: string;
  description: string;
  location: string;
  dtstart?: string | null;
  dtend?: string | null;
  all_day: boolean;
  rrule?: string | null;
  alarms: Alarm[];
  attendees: Attendee[];
};

type Store = {
  ready: boolean;
  syncing: boolean;
  error?: string;
  config?: AppConfig;
  calendars: Calendar[];
  events: CalEvent[];
  pending: CalEvent[];
  theme?: ThemeColors;
  lastSync?: string | null;
  lastSyncError?: string | null;
  defaultCalendarId?: number | null;
  view: "dayGridMonth" | "timeGridWeek" | "timeGridDay" | "multiMonthYear";
  selected?: CalEvent | null;
  showSettings: boolean;
  showEditor: boolean;
  editorDraft?: Partial<EventInput> & { id?: number };
  editorTitle?: string;
  pendingImport?: ImportPreview | null;
  importError?: string | null;
  search: string;
  searchResults: CalEvent[];
  load: () => Promise<void>;
  sync: () => Promise<void>;
  setView: (v: Store["view"]) => void;
  setSelected: (e: CalEvent | null) => void;
  setShowSettings: (v: boolean) => void;
  setShowEditor: (v: boolean) => void;
  setEditorDraft: (d: Store["editorDraft"]) => void;
  setEditorTitle: (t?: string) => void;
  setPendingImport: (p: ImportPreview | null) => void;
  setImportError: (e: string | null) => void;
  toggleCalendar: (id: number, visible: boolean) => Promise<void>;
  setCalendarColor: (id: number, color: string) => Promise<void>;
  setDefaultCalendar: (id: number | null) => Promise<void>;
  setCalendarSubscribed: (id: number, subscribed: boolean) => Promise<void>;
  reorderCalendars: (accountId: string, orderedIds: number[]) => Promise<void>;
  setSearch: (q: string) => Promise<void>;
  applyTheme: (t: ThemeColors) => void;
};

function applyThemeToDom(t: ThemeColors) {
  const root = document.documentElement;
  root.style.setProperty("--bg", t.background);
  root.style.setProperty("--fg", t.foreground);
  root.style.setProperty("--accent", t.accent);
  root.style.setProperty("--cursor", t.cursor);
  root.style.setProperty("--sel-fg", t.selection_foreground);
  root.style.setProperty("--sel-bg", t.selection_background);
  t.colors.forEach((c, i) => root.style.setProperty(`--color${i}`, c));
  root.dataset.theme = t.light ? "light" : "dark";
  document.body.style.background = t.background;
  document.body.style.color = t.foreground;
}

export const useApp = create<Store>((set, get) => ({
  ready: false,
  syncing: false,
  calendars: [],
  events: [],
  pending: [],
  defaultCalendarId: null,
  view: "timeGridWeek",
  selected: null,
  showSettings: false,
  showEditor: false,
  pendingImport: null,
  importError: null,
  search: "",
  searchResults: [],

  applyTheme: (t) => {
    applyThemeToDom(t);
    set({ theme: t });
  },

  load: async () => {
    try {
      const snap = await safeInvoke<{
        config: AppConfig;
        calendars: Calendar[];
        events: CalEvent[];
        pending_invites: CalEvent[];
        theme: ThemeColors;
        last_sync?: string | null;
        last_sync_error?: string | null;
        default_calendar_id?: number | null;
      }>("get_snapshot");
      applyThemeToDom(snap.theme);
      set({
        ready: true,
        config: snap.config,
        calendars: snap.calendars,
        events: snap.events,
        pending: snap.pending_invites,
        theme: snap.theme,
        lastSync: snap.last_sync,
        lastSyncError: snap.last_sync_error,
        defaultCalendarId: snap.default_calendar_id ?? null,
        error: undefined,
        showSettings: snap.config.accounts.length === 0,
      });
    } catch (e) {
      set({ ready: true, error: String(e) });
    }
  },

  sync: async () => {
    set({ syncing: true, error: undefined });
    try {
      await safeInvoke("sync_now");
      await get().load();
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ syncing: false });
    }
  },

  setView: (v) => set({ view: v }),
  setSelected: (e) => set({ selected: e }),
  setShowSettings: (v) => set({ showSettings: v }),
  setShowEditor: (v) => set({ showEditor: v }),
  setEditorDraft: (d) => set({ editorDraft: d }),
  setEditorTitle: (t) => set({ editorTitle: t }),
  setPendingImport: (p) => set({ pendingImport: p }),
  setImportError: (e) => set({ importError: e }),

  toggleCalendar: async (id, visible) => {
    if (!isTauri()) {
      set({
        calendars: get().calendars.map((c) => (c.id === id ? { ...c, visible } : c)),
      });
      return;
    }
    await invoke("set_calendar_visible", { id, visible });
    await get().load();
  },

  setCalendarColor: async (id, color) => {
    if (!isTauri()) {
      set({
        calendars: get().calendars.map((c) => (c.id === id ? { ...c, color } : c)),
      });
      return;
    }
    await invoke("set_calendar_color", { id, color });
    await get().load();
  },

  setDefaultCalendar: async (id) => {
    if (!isTauri()) {
      set({ defaultCalendarId: id });
      return;
    }
    await invoke("set_default_calendar", { id });
    await get().load();
  },

  setCalendarSubscribed: async (id, subscribed) => {
    if (!isTauri()) {
      set({
        calendars: get().calendars.map((c) =>
          c.id === id ? { ...c, subscribed, visible: subscribed ? true : c.visible } : c,
        ),
      });
      return;
    }
    await invoke("set_calendar_subscribed", { id, subscribed });
    await get().load();
    if (subscribed) {
      // Refetch objects for restored calendar
      await get().sync();
    }
  },

  reorderCalendars: async (accountId, orderedIds) => {
    if (!isTauri()) {
      const order = new Map(orderedIds.map((id, i) => [id, i]));
      set({
        calendars: get()
          .calendars.map((c) =>
            c.account_id === accountId && order.has(c.id)
              ? { ...c, sort_order: order.get(c.id)! }
              : c,
          )
          .sort((a, b) => a.sort_order - b.sort_order),
      });
      return;
    }
    await invoke("reorder_calendars", {
      accountId,
      orderedIds,
    });
    await get().load();
  },

  setSearch: async (q) => {
    set({ search: q });
    if (!q.trim()) {
      set({ searchResults: [] });
      return;
    }
    const results = await safeInvoke<CalEvent[]>("search_events", { query: q });
    set({ searchResults: results });
  },
}));

export async function bootListeners() {
  if (!isTauri()) return; // browser preview: no backend events to listen for
  await listen<ThemeColors>("theme-changed", (ev) => {
    useApp.getState().applyTheme(ev.payload);
  });
  await listen("sync-finished", () => {
    useApp.getState().load();
  });
  await listen("alarm-fired", () => {
    // could toast; notifications already shown via mako
  });
  await listen<string>("import-ics", async (ev) => {
    try {
      const preview = await previewIcs(ev.payload);
      useApp.getState().setImportError(null);
      useApp.getState().setPendingImport(preview);
    } catch (e) {
      useApp.getState().setImportError(String(e));
    }
  });
}

export async function saveEvent(input: EventInput) {
  return safeInvoke<CalEvent>("save_event", { input });
}

export async function previewIcs(path: string): Promise<ImportPreview> {
  return safeInvoke<ImportPreview>("preview_ics", { path });
}

export async function takePendingImports(): Promise<string[]> {
  return safeInvoke<string[]>("take_pending_imports");
}

export async function deleteEvent(id: number) {
  return safeInvoke("delete_event", { id });
}

export async function deleteEventOccurrence(id: number, occurrenceStart: string, mode: "single" | "future" | "all") {
  return safeInvoke("delete_event_occurrence", { req: { id, occurrence_start: occurrenceStart, mode } });
}

export async function respondInvite(eventId: number, partstat: string) {
  return safeInvoke<CalEvent>("respond_invite", {
    req: { event_id: eventId, partstat },
  });
}

export async function respondInvitesBulk(eventIds: number[], partstat: string) {
  return safeInvoke<{ ok: number; failed: number; errors: string[] }>(
    "respond_invites_bulk",
    {
      req: { event_ids: eventIds, partstat },
    },
  );
}

export async function addAccount(payload: {
  display_name: string;
  caldav_url: string;
  username: string;
  password: string;
  addresses: string[];
}) {
  return safeInvoke<Account>("add_account", { req: payload });
}

export async function removeAccount(account_id: string) {
  return safeInvoke("remove_account", { accountId: account_id });
}

export async function testAccount(
  caldav_url: string,
  username: string,
  password: string,
) {
  return safeInvoke<string[]>("test_account", {
    caldavUrl: caldav_url,
    username,
    password,
  });
}
