import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { CalEvent } from "../store";
import { eventTimeLabel } from "../eventLayout";

export type DayPopoverAnchor = {
  left: number;
  top: number;
  right: number;
  bottom: number;
};

type Props = {
  date: Date;
  /** Already calendar-filtered events for this day (all-day + timed). */
  events: CalEvent[];
  time24h: boolean;
  anchor: DayPopoverAnchor;
  onClose: () => void;
  onSelectEvent: (ev: CalEvent) => void;
};

const GAP = 6;
const MARGIN = 8;

export function DayPopover({
  date,
  events,
  time24h,
  anchor,
  onClose,
  onSelectEvent,
}: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    let left = anchor.left;
    if (left + w > vw - MARGIN) left = Math.max(MARGIN, anchor.right - w);
    left = Math.max(MARGIN, Math.min(left, vw - w - MARGIN));
    let top = anchor.bottom + GAP;
    if (top + h > vh - MARGIN) top = Math.max(MARGIN, anchor.top - h - GAP);
    top = Math.max(MARGIN, top);
    setPos({ left, top });
  }, [anchor]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  return (
    <>
      <div className="day-popover-backdrop" onClick={onClose} onContextMenu={(e) => { e.preventDefault(); onClose(); }} />
      <div
        ref={ref}
        role="dialog"
        aria-label={`${events.length} events on ${date.toLocaleDateString()}`}
        className="day-popover"
        style={pos ? { left: pos.left, top: pos.top } : { visibility: "hidden" }}
      >
        <div className="day-popover-header">
          <span className="day-popover-title">{date.toLocaleDateString("en-US", { weekday: "long", month: "long", day: "numeric" })}</span>
          <button type="button" className="day-popover-close" aria-label="Close" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="day-popover-list" role="listbox">
          {events.length === 0 && <div className="day-popover-empty">No events</div>}
          {events.map((ev) => {
            const time = eventTimeLabel(ev, time24h);
            const label = ev.all_day ? `${ev.title}` : `${time}, ${ev.title}`;
            return (
              <button
                key={ev.id}
                type="button"
                role="option"
                className="day-popover-row"
                title={label}
                onClick={() => onSelectEvent(ev)}
              >
                <span className="day-popover-dot" style={{ background: ev.color, borderColor: ev.color }} />
                {!ev.all_day && <span className="day-popover-time">{time}</span>}
                <span className="day-popover-title-text">{ev.title || "(no title)"}</span>
              </button>
            );
          })}
        </div>
      </div>
    </>
  );
}
