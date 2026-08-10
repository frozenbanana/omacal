import { FormEvent, useState } from "react";
import type { Calendar, EventInput } from "../store";

type Props = {
  draft: Partial<EventInput> & { id?: number };
  calendars: Calendar[];
  timezone: string;
  title?: string;
  onClose: () => void;
  onSave: (input: EventInput) => Promise<void>;
};

export function EventEditor({ draft, calendars, timezone, title, onClose, onSave }: Props) {
  const [summary, setSummary] = useState(draft.summary || "");
  const [description, setDescription] = useState(draft.description || "");
  const [location, setLocation] = useState(draft.location || "");
  const writable = calendars.filter((c) => !c.readonly && c.subscribed !== false);
  const [calendarId, setCalendarId] = useState(
    draft.calendar_id || writable[0]?.id || calendars[0]?.id,
  );
  const [allDay, setAllDay] = useState(!!draft.all_day);
  const [dtstart, setDtstart] = useState(toLocalInput(draft.dtstart || "", !!draft.all_day));
  const [dtend, setDtend] = useState(toLocalInput(draft.dtend || "", !!draft.all_day));
  const [rrule, setRrule] = useState(draft.rrule || "");
  const [alarm, setAlarm] = useState(draft.alarms?.[0]?.trigger || "-PT15M");
  const [attendees, setAttendees] = useState(
    (draft.attendees || []).map((a) => a.email).join(", "),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!calendarId) return;
    setBusy(true);
    setError(undefined);
    try {
      const attendeeList = attendees
        .split(/[,;\s]+/)
        .map((s) => s.trim())
        .filter(Boolean)
        .map((email) => ({
          email,
          cn: null,
          partstat: "NEEDS-ACTION",
          role: "REQ-PARTICIPANT",
          rsvp: true,
        }));
      await onSave({
        calendar_id: calendarId,
        uid: draft.uid,
        summary,
        description,
        location,
        dtstart: fromLocalInput(dtstart, allDay),
        dtend: fromLocalInput(dtend, allDay),
        all_day: allDay,
        timezone,
        rrule: rrule.trim() || null,
        alarms: alarm ? [{ trigger: alarm, description: "Reminder" }] : [],
        attendees: attendeeList,
        href: draft.href,
        etag: draft.etag,
      });
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <form
        className="drawer"
        onClick={(e) => e.stopPropagation()}
        onSubmit={submit}
      >
        <h2>{title ?? (draft.uid ? "Edit event" : "New event")}</h2>
        <div className="form-grid">
          <label>
            Title
            <input
              value={summary}
              onChange={(e) => setSummary(e.target.value)}
              required
              autoFocus
            />
          </label>
          <label>
            Calendar
            <select
              value={calendarId}
              onChange={(e) => setCalendarId(Number(e.target.value))}
            >
              {writable.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.displayname}
                  </option>
                ))}
            </select>
          </label>
          <label className="cal-row" style={{ textTransform: "none" }}>
            <input
              type="checkbox"
              checked={allDay}
              onChange={(e) => setAllDay(e.target.checked)}
            />
            All day
          </label>
          <div className="form-row">
            <label>
              Starts
              <input
                type={allDay ? "date" : "datetime-local"}
                value={dtstart}
                onChange={(e) => setDtstart(e.target.value)}
                required
              />
            </label>
            <label>
              Ends
              <input
                type={allDay ? "date" : "datetime-local"}
                value={dtend}
                onChange={(e) => setDtend(e.target.value)}
                required
              />
            </label>
          </div>
          <label>
            Location
            <input value={location} onChange={(e) => setLocation(e.target.value)} />
          </label>
          <label>
            Notes
            <textarea
              rows={3}
              value={description}
              onChange={(e) => setDescription(e.target.value)}
            />
          </label>
          <label>
            Repeat (RRULE)
            <input
              placeholder="FREQ=WEEKLY;BYDAY=MO,WE"
              value={rrule}
              onChange={(e) => setRrule(e.target.value)}
            />
          </label>
          <label>
            Reminder
            <select value={alarm} onChange={(e) => setAlarm(e.target.value)}>
              <option value="">None</option>
              <option value="-PT5M">5 minutes before</option>
              <option value="-PT15M">15 minutes before</option>
              <option value="-PT30M">30 minutes before</option>
              <option value="-PT1H">1 hour before</option>
              <option value="-P1D">1 day before</option>
            </select>
          </label>
          <label>
            Attendees (emails)
            <input
              placeholder="a@example.com, b@example.com"
              value={attendees}
              onChange={(e) => setAttendees(e.target.value)}
            />
          </label>
        </div>
        {error && <p className="error">{error}</p>}
        <div className="actions">
          <button type="submit" className="primary" disabled={busy}>
            {busy ? "Saving…" : "Save"}
          </button>
          <button type="button" onClick={onClose}>
            Cancel
          </button>
        </div>
      </form>
    </div>
  );
}

function toLocalInput(value: string, allDay: boolean): string {
  if (!value) return "";
  if (allDay) return value.slice(0, 10);
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) {
    return value.slice(0, 16).replace(" ", "T");
  }
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function fromLocalInput(value: string, allDay: boolean): string {
  if (allDay) return value.slice(0, 10);
  // treat as local wall time; backend applies timezone
  if (value.length === 16) return `${value}:00`;
  return value;
}
