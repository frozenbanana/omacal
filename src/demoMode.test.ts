process.env.TZ = "Europe/Stockholm";
import { describe, expect, it } from "vitest";
import {
  useApp,
  bootListeners,
  takePendingImports,
  deleteEvent,
  saveEventOccurrence,
} from "./store";
import { isTauri } from "./demoData";

// Minimal DOM so applyThemeToDom works; no __TAURI_INTERNALS__ (plain browser).
const styleStub = { setProperty: () => {} };
(globalThis as Record<string, unknown>).window = { __TAURI_INTERNALS__: undefined };
(globalThis as Record<string, unknown>).document = {
  documentElement: { style: styleStub, dataset: {} },
  body: { style: {} },
} as never;

// Simulates running outside the Tauri webview (node has no window.__TAURI_INTERNALS__).
describe("browser preview fallback", () => {
  it("detects a non-Tauri environment", () => {
    expect(isTauri()).toBe(false);
  });

  it("bootListeners resolves without crashing (no __TAURI_INTERNALS__)", async () => {
    await expect(bootListeners()).resolves.toBeUndefined();
  });

  it("store actions route to the demo backend", async () => {
    await expect(takePendingImports()).resolves.toEqual([]);
    await expect(deleteEvent(999)).resolves.toBeUndefined();
    await expect(
      saveEventOccurrence(1, "2026-09-14T07:00:00+00:00", {
        calendar_id: 1,
        summary: "One occurrence",
        description: "",
        location: "",
        dtstart: "2026-09-14T10:00:00",
        dtend: "2026-09-14T11:00:00",
        all_day: false,
        timezone: "Europe/Stockholm",
        rrule: null,
        alarms: [],
        attendees: [],
      })
    ).resolves.toBeUndefined();
  });

  it("load() populates demo data", async () => {
    await useApp.getState().load();
    const s = useApp.getState();
    expect(s.ready).toBe(true);
    expect(s.calendars.length).toBeGreaterThan(0);
    expect(s.events.length).toBeGreaterThan(0);
    expect(s.config?.locale.timezone).toBe("Europe/Stockholm");
  });

  it("search works against demo events", async () => {
    await useApp.getState().setSearch("nogi");
    const results = useApp.getState().searchResults;
    expect(results.length).toBeGreaterThan(0);
    await useApp.getState().setSearch("");
    expect(useApp.getState().searchResults).toEqual([]);
  });
});
