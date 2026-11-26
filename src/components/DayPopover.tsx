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
  timezone?: string;
  onEventContextMenu?: (event: CalEvent, x: number, y: number, trigger: HTMLElement) => void;
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
  timezone,
  onEventContextMenu,
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
      if (document.querySelector(".event-context-menu")) return;
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
      <div
        className="day-popover-backdrop"
        onClick={onClose}
        onContextMenu={(e) => {
          e.preventDefault();
          onClose();
        }}
      />
      <div
        ref={ref}
        role="dialog"
        aria-label={`${events.length} events on ${date.toLocaleDateString()}`}
        className="day-popover"
        style={pos ? { left: pos.left, top: pos.top } : { visibility: "hidden" }}
      >
        <div className="day-popover-header">
          <span className="day-popover-title">
            {date.toLocaleDateString("en-US", { weekday: "long", month: "long", day: "numeric" })}
          </span>
          <button type="button" className="day-popover-close" aria-label="Close" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="day-popover-list" role="listbox">
          {events.length === 0 && <div className="day-popover-empty">No events</div>}
          {events.map((ev) => {
            const time = eventTimeLabel(ev, time24h, timezone);
            const label = ev.all_day ? `${ev.title}` : `${time}, ${ev.title}`;
            return (
              <button
                key={ev.id}
                type="button"
                role="option"
                className="day-popover-row"
                title={label}
                aria-haspopup={onEventContextMenu ? "menu" : undefined}
                onContextMenu={(event) => {
                  if (!onEventContextMenu) return;
                  event.preventDefault();
                  event.stopPropagation();
                  onEventContextMenu(ev, event.clientX, event.clientY, event.currentTarget);
                }}
                onKeyDown={(event) => {
                  if (
                    !onEventContextMenu ||
                    (event.key !== "ContextMenu" && !(event.shiftKey && event.key === "F10"))
                  )
                    return;
                  event.preventDefault();
                  event.stopPropagation();
                  const rect = event.currentTarget.getBoundingClientRect();
                  onEventContextMenu(ev, rect.left, rect.bottom, event.currentTarget);
                }}
                onClick={() => onSelectEvent(ev)}
              >
                <span
                  className="day-popover-dot"
                  style={{ background: ev.color, borderColor: ev.color }}
                />
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
