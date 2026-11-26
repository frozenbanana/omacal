import { afterEach, describe, expect, it, vi } from "vitest";
import { readPreference, writePreference } from "./uiPreferences";

afterEach(() => vi.unstubAllGlobals());

describe("presentation preferences", () => {
  it("defaults safely when storage is unavailable", () => {
    vi.stubGlobal("localStorage", undefined);
    expect(readPreference("sidebar-collapsed")).toBe(false);
    expect(readPreference("sidebar-collapsed", true)).toBe(true);
    expect(() => writePreference("sidebar-collapsed", true)).not.toThrow();
  });

  it("persists only namespaced UI state and restores both values", () => {
    const data = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => data.get(key) ?? null,
      setItem: (key: string, value: string) => data.set(key, value),
    });
    expect(readPreference("sidebar-collapsed")).toBe(false);
    writePreference("sidebar-collapsed", true);
    expect(data.get("omacal.ui.sidebar-collapsed")).toBe("true");
    expect(readPreference("sidebar-collapsed")).toBe(true);
    writePreference("sidebar-collapsed", false);
    expect(readPreference("sidebar-collapsed")).toBe(false);
  });

  it("does not block navigation when storage throws", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => {
        throw new Error("Denied");
      },
      setItem: () => {
        throw new Error("Quota exceeded");
      },
    });
    expect(readPreference("sidebar-collapsed")).toBe(false);
    expect(() => writePreference("sidebar-collapsed", true)).not.toThrow();
  });
});
