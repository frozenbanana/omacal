// Contrast is measured against opaque, final surfaces (WCAG relative luminance).
// Calendar colors remain identities; only their presentation changes with the theme.
type RGB = [number, number, number];

function rgb(color: string): RGB {
  const hex = color.trim().replace(/^#/, "");
  const full = hex.length === 3 ? [...hex].map((c) => c + c).join("") : hex;
  if (!/^[\da-f]{6}([\da-f]{2})?$/i.test(full)) return [128, 128, 128];
  return [0, 2, 4].map((i) => parseInt(full.slice(i, i + 2), 16)) as RGB;
}

export function mixColor(color: string, background: string, amount: number): string {
  const a = rgb(color);
  const b = rgb(background);
  return `#${a
    .map((v, i) =>
      Math.round(v * amount + b[i] * (1 - amount))
        .toString(16)
        .padStart(2, "0")
    )
    .join("")}`;
}

function luminance(color: string): number {
  const values = rgb(color).map((v) => {
    const s = v / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return values[0] * 0.2126 + values[1] * 0.7152 + values[2] * 0.0722;
}

export function contrastRatio(a: string, b: string): number {
  const x = luminance(a);
  const y = luminance(b);
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

export function readableColor(preferred: string, background: string, minimum = 4.5): string {
  if (contrastRatio(preferred, background) >= minimum) return preferred;
  const target =
    contrastRatio("#ffffff", background) > contrastRatio("#000000", background)
      ? "#ffffff"
      : "#000000";
  for (let step = 1; step <= 100; step++) {
    const candidate = mixColor(target, preferred, step / 100);
    if (contrastRatio(candidate, background) >= minimum) return candidate;
  }
  return target;
}

export function eventPalette(color: string, background: string, foreground: string) {
  const surface = mixColor(color, background, 0.18);
  const text = readableColor(foreground, surface);
  return {
    surface,
    text,
    secondary: readableColor(mixColor(text, surface, 0.78), surface),
    edge: readableColor(color, surface, 3),
  };
}
