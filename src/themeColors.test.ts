import { describe, expect, it } from "vitest";
import { contrastRatio, eventPalette, mixColor, readableColor } from "./themeColors";

describe("theme contrast", () => {
  it("uses WCAG luminance and mixes opaque surfaces", () => {
    expect(contrastRatio("#000000", "#ffffff")).toBe(21);
    expect(mixColor("#ffffff", "#000000", 0.5)).toBe("#808080");
    expect(mixColor("#fff", "#000", 0.5)).toBe("#808080");
  });

  it("preserves readable theme colors and repairs faint secondary text", () => {
    expect(readableColor("#d8dee9", "#2e3440")).toBe("#d8dee9");
    for (const background of ["#2e3440", "#ffffff", "#808080"]) {
      expect(
        contrastRatio(readableColor(background, background), background)
      ).toBeGreaterThanOrEqual(4.5);
    }
  });

  it.each([
    ["#2e3440", "#d8dee9"], // Nord
    ["#eff1f5", "#4c4f69"], // light
    ["#000000", "#ffffff"],
    ["#808080", "#909090"], // deliberately low-contrast theme
  ])("keeps event labels readable on %s", (background, foreground) => {
    for (const color of ["#ff2968", "#711a76", "#0087c5", "#e9d859", "#ffffff", "#000000"]) {
      const palette = eventPalette(color, background, foreground);
      expect(contrastRatio(palette.text, palette.surface)).toBeGreaterThanOrEqual(4.5);
      expect(contrastRatio(palette.secondary, palette.surface)).toBeGreaterThanOrEqual(4.5);
      expect(contrastRatio(palette.edge, palette.surface)).toBeGreaterThanOrEqual(3);
    }
  });

  it("recomputes presentation when the theme changes without changing calendar identity", () => {
    const dark = eventPalette("#711a76", "#2e3440", "#d8dee9");
    const light = eventPalette("#711a76", "#eff1f5", "#4c4f69");
    expect(dark.surface).not.toBe(light.surface);
    expect(dark.text).not.toBe(light.text);
  });
});
