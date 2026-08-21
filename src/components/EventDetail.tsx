import { useState } from "react";
import { DateTime } from "luxon";
import { useApp } from "../store";
import type { CalEvent } from "../store";
import { humanize, parseRRule } from "./RepeatBuilder";

type Props = {
  event: CalEvent;
  onClose: () => void;
  onEdit: () => void;
  onDelete: () => Promise<void>;
  onRsvp: (partstat: string) => Promise<void>;
};

const RSVP_OPTIONS = [
  { partstat: "ACCEPTED", label: "Accept", className: "primary" },
  { partstat: "TENTATIVE", label: "Maybe", className: "" },
  { partstat: "DECLINED", label: "Decline", className: "danger" },
] as const;

function rsvpLabel(partstat?: string | null): string {
  switch ((partstat || "").toUpperCase()) {
    case "ACCEPTED":
      return "Accepted";
    case "TENTATIVE":
      return "Maybe";
    case "DECLINED":
      return "Declined";
    case "NEEDS-ACTION":
      return "Needs response";
    default:
      return partstat || "—";
  }
}

export function EventDetail({ event, onClose, onEdit, onDelete, onRsvp }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const current = (event.my_partstat || "").toUpperCase();
  const canRsvp = !!event.my_partstat;
  const tz = useApp((s) => s.config?.locale.timezone) || "Europe/Stockholm";

  async function changeRsvp(partstat: string) {
    if (busy || partstat === current) return;
    setBusy(true);
    setError(undefined);
    try {
      await onRsvp(partstat);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <div className="drawer" onClick={(e) => e.stopPropagation()}>
        <h2>{event.title || "(no title)"}</h2>
        <div className="form-grid">
          <div>
            <div className="muted">When</div>
            <div>
              {event.all_day
                ? `${event.start} → ${event.end || ""}`
                : `${format(event.start, tz)} → ${format(event.end, tz)}`}
            </div>
          </div>
          <div>
            <div className="muted">Calendar</div>
            <div>
              <span
                className="swatch"
                style={{
                  background: event.color,
                  display: "inline-block",
                  marginRight: 6,
                }}
              />
              {event.calendar_name}
            </div>
          </div>
          {event.location && (
            <div>
              <div className="muted">Location</div>
              <div>{event.location}</div>
            </div>
          )}
          {event.description && (
            <div>
              <div className="muted">Notes</div>
              <div style={{ whiteSpace: "pre-wrap" }}>{event.description}</div>
            </div>
          )}
          {event.rrule && (
            <div>
              <div className="muted">Repeats</div>
              <div>
                {(() => {
                  const parsed = parseRRule(event.rrule, event.start || "", tz);
                  return parsed ? humanize(parsed) : event.rrule;
                })()}
                <div className="muted" style={{ fontSize: "0.68rem" }}>
                  {event.rrule}
                </div>
              </div>
            </div>
          )}
          {event.organizer && (
            <div>
              <div className="muted">Organizer</div>
              <div>{event.organizer}</div>
            </div>
          )}
          {event.attendees?.length > 0 && (
            <div>
              <div className="muted">Attendees</div>
              <ul style={{ margin: 0, paddingLeft: "1.1rem" }}>
                {event.attendees.map((a) => (
                  <li key={a.email}>
                    {a.cn || a.email} <span className="muted">{a.partstat || ""}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>

        {canRsvp && (
          <div className="rsvp-panel">
            <div className="muted">Your response</div>
            <div className="rsvp-current">{rsvpLabel(event.my_partstat)}</div>
            <div className="actions rsvp-actions">
              {RSVP_OPTIONS.map((opt) => (
                <button
                  key={opt.partstat}
                  type="button"
                  className={[opt.className, current === opt.partstat ? "rsvp-selected" : ""]
                    .filter(Boolean)
                    .join(" ")}
                  disabled={busy}
                  aria-pressed={current === opt.partstat}
                  onClick={() => changeRsvp(opt.partstat)}
                >
                  {opt.label}
                  {current === opt.partstat ? " ✓" : ""}
                </button>
              ))}
            </div>
            {error && (
              <div className="error" style={{ border: 0, padding: "0.35rem 0" }}>
                {error}
              </div>
            )}
          </div>
        )}

        <div className="actions">
          {!event.readonly && (
            <>
              <button type="button" className="primary" onClick={onEdit}>
                Edit
              </button>
              <button type="button" className="danger" onClick={() => onDelete()}>
                Delete
              </button>
            </>
          )}
          <button type="button" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

function format(v?: string | null, timeZone?: string) {
  if (!v) return "—";
  if (timeZone) {
    const dt = DateTime.fromISO(v, { setZone: true });
    if (dt.isValid) {
      const zoned = dt.setZone(timeZone);
      if (zoned.isValid) return zoned.toLocaleString(DateTime.DATETIME_MED);
    }
  }
  const d = new Date(v);
  if (Number.isNaN(d.getTime())) return v;
  return d.toLocaleString();
}
