import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { EventEditor } from "./EventEditor";

describe("EventEditor recurrence scope", () => {
  it("locks the calendar and hides repeat controls for one occurrence", () => {
    const markup = renderToStaticMarkup(
      <EventEditor
        draft={{
          calendar_id: 1,
          uid: "series-1",
          summary: "Morning practice",
          description: "",
          location: "",
          dtstart: "2026-09-14T07:00:00+00:00",
          dtend: "2026-09-14T08:00:00+00:00",
          all_day: false,
          timezone: "Europe/Stockholm",
          rrule: "FREQ=WEEKLY;BYDAY=MO",
          alarms: [],
          attendees: [],
        }}
        calendars={[
          {
            id: 1,
            account_id: "account",
            href: "/calendar/",
            displayname: "Calendar",
            color: "#ffffff",
            visible: true,
            readonly: false,
            subscribed: true,
            sort_order: 0,
          },
        ]}
        timezone="Europe/Stockholm"
        recurrenceEditScope="single"
        onClose={() => {}}
        onSave={async () => {}}
      />
    );

    expect(markup).toContain("only this event");
    expect(markup).toMatch(/<select[^>]*disabled/);
    expect(markup).not.toContain("Does not repeat");
    expect(markup).not.toContain("FREQ=WEEKLY");
  });
});
