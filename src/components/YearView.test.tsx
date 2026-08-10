process.env.TZ = "Europe/Stockholm";
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { CalEvent } from "../store";
import { YearView } from "./YearView";

function ev(over: Partial<CalEvent> & { start: string }): CalEvent {
  return {
    id: 1,
    calendar_id: 1,
    href: `h-${over.start}`,
    etag: null,
    uid: `u-${over.start}`,
    title: "Event",
    description: "",
    location: "",
    end: null,
    all_day: false,
    rrule: null,
    color: "#ff0000",
    calendar_name: "Cal",
    status: null,
    organizer: null,
    attendees: [],
    alarms: [],
    my_partstat: null,
    readonly: false,
    ...over,
  };
}

function cellBlock(markup: string, ariaLabel: string): string {
  const idx = markup.indexOf(ariaLabel);
  expect(idx, `missing aria-label ${ariaLabel}`).toBeGreaterThan(-1);
  const start = markup.lastIndexOf("<button", idx);
  const end = markup.indexOf("</button>", idx);
  return markup.slice(start, end);
}

describe("YearView", () => {
  const events: CalEvent[] = [
    ev({ id: 1, start: "2026-08-05T05:00:00.000Z", end: "2026-08-05T06:00:00.000Z", title: "NOGI Morning" }),
    ev({ id: 2, start: "2026-08-05T12:00:00.000Z", end: "2026-08-05T13:00:00.000Z", title: "Lotus Tech Sync" }),
    ev({ id: 3, start: "2026-08-05T17:00:00.000Z", end: "2026-08-05T18:00:00.000Z", title: "NOGI Evening" }),
    // multi-day personal event: Planeringsvecka Mon Aug 10 -> Fri Aug 14
    ev({ id: 4, start: "2026-08-10", end: "2026-08-15", all_day: true, title: "Planeringsvecka" }),
    ev({ id: 5, start: "2026-08-12T05:00:00.000Z", end: "2026-08-12T06:00:00.000Z", title: "Planning sync" }),
  ];

  const markup = renderToStaticMarkup(
    <YearView
      year={2026}
      events={events}
      today={new Date(2026, 7, 12, 12, 0, 0)}
      weekStartsOn={1}
      onSelectDay={() => {}}
      onSelectMonth={() => {}}
    />,
  );

  it("renders the year title and all twelve months", () => {
    expect(markup).toContain('class="year-title">2026</h2>');
    expect(markup.match(/class="mm-title"/g)?.length).toBe(12);
  });

  it("shows density dots + aria count for a busy day (Aug 5: 3 events)", () => {
    const cell = cellBlock(markup, "August 5, 3 events");
    expect(cell.match(/class="mm-dot"/g)?.length).toBe(3);
  });

  it("shows up to three dots for Aug 12 (2 events) and highlights today", () => {
    const cell = cellBlock(markup, "August 12, 2 events");
    expect(cell.match(/class="mm-dot"/g)?.length).toBe(2);
    expect(cell).toContain("mm-today");
    expect(markup.match(/mm-today/g)?.length).toBe(1);
  });

  it("renders a thin all-day span for the multi-day event", () => {
    expect(markup).toContain("mm-span mm-span-start mm-span-end");
    // dense dots are preferred over titles: the mini cell shows no agenda text
    expect(markup).not.toContain("+6 more");
    expect(cellBlock(markup, "August 12, 2 events")).not.toContain("Planning sync");
  });

  it("a low-density date has no dots and no events text", () => {
    const cell = cellBlock(markup, "August 20");
    expect(cell).not.toContain("mm-dot");
  });
});
