// Presentation preferences only. Account/calendar data stays in the native store.
export function readPreference(key: string, fallback = false): boolean {
  try {
    const value = localStorage.getItem(`omacal.ui.${key}`);
    return value === null ? fallback : value === "true";
  } catch {
    return fallback;
  }
}

export function writePreference(key: string, value: boolean): void {
  try {
    localStorage.setItem(`omacal.ui.${key}`, String(value));
  } catch {
    // Sandboxed or unavailable storage must not prevent navigation.
  }
}
