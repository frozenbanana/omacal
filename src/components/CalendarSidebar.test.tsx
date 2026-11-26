import { afterEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { buildMockSnapshot } from "../demoData";
import { useApp } from "../store";
import { CalendarSidebar } from "./CalendarSidebar";

// SSR normally reads Zustand's initial snapshot. Use the test's current state
// so static rendering exercises each visibility/subscription combination.
vi.mock("../store", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../store")>();
  return { ...actual, useApp: Object.assign(() => actual.useApp.getState(), actual.useApp) };
});

const original = useApp.getState();
afterEach(() => useApp.setState(original));

describe("calendar sidebar", () => {
  it("separates synced but hidden calendars from visible and unsubscribed calendars", () => {
    const snap = buildMockSnapshot();
    const base = snap.calendars[0];
    useApp.setState({
      config: snap.config,
      calendars: [
        { ...base, id: 101, displayname: "Visible work", visible: true, subscribed: true },
        { ...base, id: 102, displayname: "Hidden shared", visible: false, subscribed: true },
        { ...base, id: 103, displayname: "Stopped syncing", visible: false, subscribed: false },
      ],
    });
    const html = renderToStaticMarkup(<CalendarSidebar />);
    expect(html).toContain("Not shown (1)");
    expect(html).toContain("Unsubscribed (1)");
    expect(html).toContain('aria-label="Show Visible work"');
    expect(html).toContain('aria-label="Show Hidden shared"');
    const hidden = html.slice(
      html.indexOf('<details class="cal-hidden"'),
      html.indexOf("</details>")
    );
    expect(hidden).toContain("Hidden shared");
    expect(hidden).not.toContain("Visible work");
    expect(hidden).not.toContain("Stopped syncing");
    expect(hidden).not.toContain(" open=");
  });

  it("does not show an empty hidden-calendar section", () => {
    const snap = buildMockSnapshot();
    useApp.setState({ config: snap.config, calendars: [{ ...snap.calendars[0], visible: true }] });
    const html = renderToStaticMarkup(<CalendarSidebar />);
    expect(html).not.toContain('class="cal-hidden"');
  });
});
