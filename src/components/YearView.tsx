import { useMemo } from "react";
import type { CalEvent } from "../store";
import {
  addCalendarDays,
  dayKey,
  eventDensityForDate,
  layoutMultiDayEventsForWeek,
  startOfWeek,
} from "../eventLayout";

type Props = {
  year: number;
  events: CalEvent[];
  today: Date;
  weekStartsOn: number;
  onSelectDay: (date: Date) => void;
  onSelectMonth: (monthStart: Date) => void;
};

const WEEKDAY_LABELS = (weekStartsOn: number) => {
  const base = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  return [...base.slice(weekStartsOn), ...base.slice(0, weekStartsOn)];
};

function Dots({ count, colors }: { count: number; colors: string[] }) {
  const dots = Math.max(0, Math.min(count, 3));
  if (dots === 0) return null;
  const items: string[] = [];
  for (let i = 0; i < dots; i += 1) {
    items.push(colors[i] ?? colors[colors.length - 1] ?? "var(--accent)");
  }
  return (
    <span className="mm-dots" aria-hidden="true">
      {items.map((c, i) => (
        <span key={i} className="mm-dot" style={{ background: c }} />
      ))}
    </span>
  );
}

function MiniMonth({
  month,
  year,
  events,
  today,
  weekStartsOn,
  onSelectDay,
  onSelectMonth,
}: {
  month: number;
  year: number;
  events: CalEvent[];
  today: Date;
  weekStartsOn: number;
  onSelectDay: (date: Date) => void;
  onSelectMonth: (monthStart: Date) => void;
}) {
  const allDayEvents = useMemo(() => events.filter((e) => e.all_day), [events]);

  const grid = useMemo(() => {
    const first = new Date(year, month, 1);
    const gridStart = startOfWeek(first, weekStartsOn);
    const dates: Date[] = [];
    for (let i = 0; i < 42; i += 1) dates.push(addCalendarDays(gridStart, i));
    const weeks: Date[][] = [];
    for (let i = 0; i < 42; i += 7) weeks.push(dates.slice(i, i + 7));
    return { dates, weeks };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [month, year, weekStartsOn]);

  const todayKey = dayKey(today);
  const title = new Intl.DateTimeFormat("en-US", { month: "long" }).format(new Date(year, month, 1));
  const monthStart = new Date(year, month, 1);

  return (
    <div className="mm" aria-label={`${title} ${year}`}>
      <button
        type="button"
        className="mm-title"
        onClick={() => onSelectMonth(monthStart)}
        title={`Open ${title} ${year} in Month view`}
      >
        {title}
      </button>
      <div className="mm-weekdays" aria-hidden="true">
        {WEEKDAY_LABELS(weekStartsOn).map((d) => (
          <span key={d} className="mm-weekday">
            {d}
          </span>
        ))}
      </div>
      <div className="mm-body">
        {grid.weeks.map((week, wi) => {
          const segs = layoutMultiDayEventsForWeek(allDayEvents, week[0], weekStartsOn);
          return (
            <div key={wi} className="mm-week">
              {week.map((date) => {
                const inMonth = date.getMonth() === month && date.getFullYear() === year;
                if (!inMonth) {
                  return <div key={dayKey(date)} className="mm-cell mm-cell-out" aria-hidden="true" />;
                }
                const key = dayKey(date);
                const { count, colors } = eventDensityForDate(events, date);
                const isToday = key === todayKey;
                return (
                  <button
                    key={key}
                    type="button"
                    className={`mm-cell${isToday ? " mm-today" : ""}`}
                    aria-label={`${date.toLocaleDateString("en-US", { month: "long", day: "numeric" })}${count ? `, ${count} event${count === 1 ? "" : "s"}` : ""}`}
                    onClick={() => onSelectDay(date)}
                  >
                    <span className="mm-daynum">{date.getDate()}</span>
                    <Dots count={count} colors={colors} />
                  </button>
                );
              })}
              {segs.map((seg) => {
                const wide = seg.endCol - seg.startCol >= 2;
                const label = wide && seg.ev.title ? seg.ev.title : "";
                return (
                  <div
                    key={`${seg.ev.id}-${seg.startCol}`}
                    className={`mm-span${seg.isStart ? " mm-span-start" : ""}${seg.isEnd ? " mm-span-end" : ""}`}
                    style={{
                      left: `${(seg.startCol / 7) * 100}%`,
                      width: `${((seg.endCol - seg.startCol + 1) / 7) * 100}%`,
                      bottom: 2 + seg.lane * 3,
                      background: seg.ev.color,
                      opacity: seg.isStart && seg.isEnd ? 0.85 : 0.55,
                    }}
                    title={seg.ev.title || undefined}
                  >
                    {label}
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
    </div>
  );
}

export function YearView({ year, events, today, weekStartsOn, onSelectDay, onSelectMonth }: Props) {
  const months = useMemo(() => Array.from({ length: 12 }, (_, i) => i), []);
  return (
    <div className="year-view">
      <h2 className="year-title">{year}</h2>
      <div className="year-grid">
        {months.map((m) => (
          <MiniMonth
            key={m}
            month={m}
            year={year}
            events={events}
            today={today}
            weekStartsOn={weekStartsOn}
            onSelectDay={onSelectDay}
            onSelectMonth={onSelectMonth}
          />
        ))}
      </div>
    </div>
  );
}