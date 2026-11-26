import { describe, expect, it } from "vitest";
import { buildMockSnapshot } from "./demoData";
import { eventContextItems } from "./eventContextMenu";

const base = buildMockSnapshot().events[0];

describe("event context menu actions", () => {
  it("offers existing actions for a regular event, without unsafe cut", () => {
    const items = eventContextItems({ ...base, readonly: false, rrule: null }, true);
    expect(items.map((item) => item.id)).toEqual([
      "details",
      "edit",
      "copy",
      "duplicate",
      "delete",
    ]);
    expect(items.every((item) => !item.disabled)).toBe(true);
  });

  it("exposes occurrence and series edits separately", () => {
    const items = eventContextItems(
      { ...base, readonly: false, rrule: "FREQ=WEEKLY", recurrence_id: "2026-11-26T09:25:00Z" },
      true
    );
    expect(items.find((item) => item.id === "edit-single")?.disabled).toBe(false);
    expect(items.find((item) => item.id === "edit-series")?.disabled).toBe(false);
    expect(items.find((item) => item.id === "copy")?.label).toBe("Copy this occurrence");
    expect(items.find((item) => item.id === "edit")).toBeUndefined();
  });

  it("does not offer a single-instance edit without occurrence identity", () => {
    const items = eventContextItems(
      { ...base, readonly: false, rrule: "FREQ=WEEKLY", recurrence_id: null },
      true
    );
    expect(items.find((item) => item.id === "edit-single")?.disabled).toBe(true);
  });

  it("allows copying from read-only calendars but never modifying the source", () => {
    const items = eventContextItems(
      { ...base, readonly: true, rrule: "FREQ=WEEKLY", recurrence_id: "2026-11-26" },
      true
    );
    for (const id of ["edit-single", "edit-series", "delete"]) {
      expect(items.find((item) => item.id === id)?.disabled).toBe(true);
    }
    expect(items.find((item) => item.id === "copy")?.disabled).toBeUndefined();
    expect(items.find((item) => item.id === "duplicate")?.disabled).toBe(false);
    expect(eventContextItems(base, false).find((item) => item.id === "duplicate")?.disabled).toBe(
      true
    );
  });
});
