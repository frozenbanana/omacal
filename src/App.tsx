import { useEffect, useMemo, useRef, useState } from "react";
import FullCalendar from "@fullcalendar/react";
import dayGridPlugin from "@fullcalendar/daygrid";
import timeGridPlugin from "@fullcalendar/timegrid";
import multiMonthPlugin from "@fullcalendar/multimonth";
import interactionPlugin from "@fullcalendar/interaction";
import type {
  DateSelectArg,
  EventClickArg,
  EventContentArg,
  EventDropArg,
  EventInput as FCEventInput,
  EventMountArg,
  MoreLinkArg,
} from "@fullcalendar/core";
import type { EventResizeDoneArg, DateClickArg } from "@fullcalendar/interaction";
import "./App.css";
import {
  bootListeners,
  deleteEvent,
  previewIcs,
  respondInvite,
  saveEvent,
  takePendingImports,
  useApp,
  type CalEvent,
  type Calendar,
  type ImportPreview,
} from "./store";
import {
  dayKey,
  eventTimeLabel,
  getAllDayEventsForDayKey,
  getTimedEventsForDayKey,
} from "./eventLayout";
import { EventEditor } from "./components/EventEditor";
import { SettingsModal } from "./components/SettingsModal";
import { EventDetail } from "./components/EventDetail";
import { CalendarSidebar } from "./components/CalendarSidebar";
import { InvitesPanel } from "./components/InvitesPanel";
import { DayPopover, type DayPopoverAnchor } from "./components/DayPopover";
import { YearView } from "./components/YearView";

function isInvite(e: CalEvent): boolean {
  return e.my_partstat === "NEEDS-ACTION";
}

function partstatClass(e: CalEvent): string | undefined {
  switch ((e.my_partstat || "").toUpperCase()) {
    case "NEEDS-ACTION":
      return "fc-invite";
    case "TENTATIVE":
      return "fc-rsvp-tentative";
    case "DECLINED":
      return "fc-rsvp-declined";
    default:
      return undefined;
  }
}

function toDraftWallClock(d: Date, allDay: boolean): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  if (allDay) {
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  }
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}:00`;
}

function toFcEvents(events: CalEvent[], marked: CalEvent | null): FCEventInput[] {
  return events.flatMap((e) => {
    if (!e.start) return [];
    const invite = isInvite(e);
    const statusClass = partstatClass(e);
    const isMarked =
      marked != null && e.id === marked.id && e.start === marked.start;
    const base: FCEventInput = {
      id: `${e.id}:${e.start}`,
      title: e.title || "(no title)",
      start: e.start,
      end: e.end || undefined,
      allDay: e.all_day,
      backgroundColor: invite ? "transparent" : e.color,
      borderColor: e.color,
      editable: !e.readonly && !invite,
      classNames: [
        e.all_day ? "om-allday" : "om-timed",
        ...(statusClass ? [statusClass] : []),
        ...(isMarked ? ["om-marked"] : []),
      ],
      extendedProps: { calEvent: e, invite },
    };
    return [base];
  });
}

export default function App() {
  const {
    ready,
    load,
    sync,
    syncing,
    calendars,
    events,
    view,
    setView,
    selected,
    setSelected,
    showSettings,
    setShowSettings,
    showEditor,
    setShowEditor,
    editorDraft,
    setEditorDraft,
    editorTitle,
    setEditorTitle,
    setPendingImport,
    pendingImport,
    importError,
    search,
    setSearch,
    searchResults,
    error,
    lastSync,
    lastSyncError,
    theme,
    config,
    defaultCalendarId,
  } = useApp();

  const calRef = useRef<FullCalendar | null>(null);
  const [rsvpBusyId, setRsvpBusyId] = useState<number | null>(null);
  const [navTitle, setNavTitle] = useState("");
  const [currentDate, setCurrentDate] = useState<Date>(() => new Date());
  const [dayPopover, setDayPopover] = useState<{
    date: Date;
    events: CalEvent[];
    anchor: DayPopoverAnchor;
  } | null>(null);
  const [marked, setMarked] = useState<CalEvent | null>(null);
  const [clipboard, setClipboard] = useState<CalEvent | null>(null);
  const [lastClickedDay, setLastClickedDay] = useState<Date | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const noticeTimer = useRef<number | null>(null);

  function showNotice(msg: string) {
    setNotice(msg);
    if (noticeTimer.current != null) window.clearTimeout(noticeTimer.current);
    noticeTimer.current = window.setTimeout(() => setNotice(null), 1800);
  }

  useEffect(() => {
    bootListeners().then(async () => {
      await load();
      // cold-start import (app launched via file association before listeners existed)
      const paths = await takePendingImports();
      for (const p of paths) {
        try {
          const preview = await previewIcs(p);
          openImport(preview);
        } catch (e) {
          useApp.setState({ error: String(e) });
        }
      }
    });
  }, [load]);

  useEffect(() => {
    if (pendingImport) {
      openImport(pendingImport);
      setPendingImport(null);
    }
  }, [pendingImport]);

  useEffect(() => {
    const onKey = (ev: KeyboardEvent) => {
      const tag = (ev.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (showEditor || showSettings) return;
      if (ev.key === "n") {
        ev.preventDefault();
        openNew();
      } else if (ev.key === "t") {
        ev.preventDefault();
        calRef.current?.getApi().today();
      } else if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "c") {
        if (!marked) return;
        ev.preventDefault();
        setClipboard(marked);
        showNotice("Copied");
      } else if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "x") {
        if (!marked) return;
        ev.preventDefault();
        if (marked.readonly) {
          showNotice("Read-only, cannot cut");
          return;
        }
        setClipboard(marked);
        const id = marked.id;
        setMarked(null);
        void (async () => {
          try {
            await deleteEvent(id);
            await load();
            showNotice("Moved to clipboard");
          } catch (e) {
            useApp.setState({ error: String(e) });
          }
        })();
      } else if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "v") {
        if (!clipboard) return;
        ev.preventDefault();
        if (!lastClickedDay) {
          showNotice("Click a day first");
          return;
        }
        void (async () => {
          try {
            await pasteClipboard();
            showNotice("Pasted");
          } catch (e) {
            useApp.setState({ error: String(e) });
          }
        })();
      } else if (ev.key === "Escape" && marked) {
        ev.preventDefault();
        setMarked(null);
      } else if ((ev.key === "Delete" || ev.key === "Backspace") && marked) {
        ev.preventDefault();
        if (marked.readonly) {
          showNotice("Read-only, cannot delete");
          return;
        }
        const id = marked.id;
        setMarked(null);
        void (async () => {
          try {
            await deleteEvent(id);
            await load();
            showNotice("Deleted");
          } catch (e) {
            useApp.setState({ error: String(e) });
          }
        })();
      } else if (ev.key === "ArrowLeft" && !ev.metaKey && !ev.ctrlKey) {
        calRef.current?.getApi().prev();
      } else if (ev.key === "ArrowRight" && !ev.metaKey && !ev.ctrlKey) {
        calRef.current?.getApi().next();
      } else if (ev.key === "e" && selected) {
        openEdit(selected);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selected, calendars, config, defaultCalendarId, marked, clipboard, lastClickedDay, showEditor, showSettings]);

  const fcEvents = useMemo(() => toFcEvents(events, marked), [events, marked]);

  const visibleEvents = useMemo(() => {
    const visibleIds = new Set(
      calendars
        .filter((c) => c.visible && c.subscribed !== false)
        .map((c) => c.id),
    );
    return events.filter((e) => visibleIds.has(e.calendar_id));
  }, [events, calendars]);

  async function rsvpFromCalendar(ev: CalEvent, partstat: string) {
    if (rsvpBusyId != null) return;
    setRsvpBusyId(ev.id);
    try {
      await respondInvite(ev.id, partstat);
      if (selected?.id === ev.id) setSelected(null);
      await load();
    } catch (e) {
      useApp.setState({ error: String(e) });
    } finally {
      setRsvpBusyId(null);
    }
  }

  function renderEventContent(arg: EventContentArg) {
    const calEvent = arg.event.extendedProps.calEvent as CalEvent | undefined;
    const invite = !!arg.event.extendedProps.invite || (calEvent && isInvite(calEvent));
    const color = calEvent?.color || arg.event.borderColor || "var(--accent)";
    const busy = calEvent != null && rsvpBusyId === calEvent.id;
    const isMonthGrid = arg.view.type === "dayGridMonth";
    const title = arg.event.title || "(no title)";

    if (isMonthGrid && calEvent && !invite && !arg.event.allDay) {
      const label = `${arg.timeText ? arg.timeText + ", " : ""}${title}`;
      return (
        <div className="om-timed-frame" title={label} aria-label={label}>
          <span className="om-dot" style={{ background: color }} aria-hidden="true" />
          {arg.timeText && <span className="om-time">{arg.timeText}</span>}
          <span className="om-title">{title}</span>
        </div>
      );
    }

    if (isMonthGrid && calEvent && !invite && arg.event.allDay) {
      const contBefore = !arg.isStart;
      const contAfter = !arg.isEnd;
      return (
        <div
          className={`om-pill-frame${contBefore ? " om-cont-before" : ""}${contAfter ? " om-cont-after" : ""}`}
          title={title}
          aria-label={title}
        >
          {contBefore && (
            <span className="om-cont" aria-hidden="true">
              ‹
            </span>
          )}
          {arg.isStart && <span className="om-pill-title">{title}</span>}
          {contAfter && (
            <span className="om-cont" aria-hidden="true">
              ›
            </span>
          )}
        </div>
      );
    }

    if (!invite || !calEvent) {
      return (
        <div className="fc-event-main-frame">
          {arg.timeText && <div className="fc-event-time">{arg.timeText}</div>}
          <div className="fc-event-title-container">
            <div className="fc-event-title fc-sticky">{title}</div>
          </div>
        </div>
      );
    }

    return (
      <div
        className="fc-invite-body"
        style={{ ["--invite-color" as string]: color }}
      >
        <div className="fc-invite-main">
          {arg.timeText && <span className="fc-event-time">{arg.timeText}</span>}
          <span className="fc-event-title">{title}</span>
          <span className="fc-invite-badge" title="Invitation">
            ?
          </span>
        </div>
        <div className="fc-invite-actions">
          <button
            type="button"
            className="fc-invite-btn accept"
            title="Accept"
            disabled={busy}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              void rsvpFromCalendar(calEvent, "ACCEPTED");
            }}
          >
            ✓
          </button>
          <button
            type="button"
            className="fc-invite-btn maybe"
            title="Maybe"
            disabled={busy}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              void rsvpFromCalendar(calEvent, "TENTATIVE");
            }}
          >
            ~
          </button>
          <button
            type="button"
            className="fc-invite-btn decline"
            title="Decline"
            disabled={busy}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              void rsvpFromCalendar(calEvent, "DECLINED");
            }}
          >
            ✕
          </button>
        </div>
      </div>
    );
  }

  function pickWritableCalendar(preferId?: number | null): Calendar | undefined {
    const subscribed = calendars.filter((c) => c.subscribed !== false);
    const preferred =
      preferId != null
        ? subscribed.find((c) => c.id === preferId && c.visible && !c.readonly)
        : undefined;
    const defaultCal =
      preferred ||
      (defaultCalendarId != null
        ? subscribed.find(
            (c) => c.id === defaultCalendarId && c.visible && !c.readonly,
          )
        : undefined);
    return (
      defaultCal ||
      subscribed.find((c) => c.visible && !c.readonly) ||
      subscribed.find((c) => !c.readonly) ||
      subscribed[0]
    );
  }

  function openNew(start?: Date, end?: Date, allDay = false) {
    const firstWritable = pickWritableCalendar();
    if (!firstWritable) {
      setShowSettings(true);
      return;
    }
    const s = start || new Date();
    const e = end || new Date(s.getTime() + 60 * 60 * 1000);
    setEditorTitle(undefined);
    setEditorDraft({
      calendar_id: firstWritable.id,
      summary: "",
      description: "",
      location: "",
      dtstart: toDraftWallClock(s, allDay),
      dtend: toDraftWallClock(e, allDay),
      all_day: allDay,
      timezone: config?.locale.timezone || "Europe/Stockholm",
      alarms: [{ trigger: "-PT15M" }],
      attendees: [],
    });
    setShowEditor(true);
  }

  function openEdit(ev: CalEvent) {
    const baseUid = ev.uid.includes("::") ? ev.uid.split("::")[0] : ev.uid;
    setEditorTitle(undefined);
    setEditorDraft({
      id: ev.id,
      calendar_id: ev.calendar_id,
      uid: baseUid,
      summary: ev.title,
      description: ev.description,
      location: ev.location,
      dtstart: ev.start || "",
      dtend: ev.end || ev.start || "",
      all_day: ev.all_day,
      timezone: config?.locale.timezone || "Europe/Stockholm",
      rrule: ev.rrule,
      alarms: ev.alarms,
      attendees: ev.attendees,
      href: ev.href,
      etag: ev.etag,
    });
    setShowEditor(true);
  }

  // Prefill the editor from an imported .ics, then let the user pick a calendar and save.
  function openImport(preview: ImportPreview) {
    const s = useApp.getState();
    const subscribed = s.calendars.filter((c) => c.subscribed !== false);
    const defaultCal =
      s.defaultCalendarId != null
        ? subscribed.find(
            (c) => c.id === s.defaultCalendarId && c.visible && !c.readonly,
          )
        : undefined;
    const target =
      defaultCal ||
      subscribed.find((c) => c.visible && !c.readonly) ||
      subscribed.find((c) => !c.readonly) ||
      subscribed[0];
    if (!target) {
      setShowSettings(true);
      return;
    }
    setEditorTitle("Import event");
    setEditorDraft({
      calendar_id: target.id,
      // no uid: always create a fresh event on import
      summary: preview.summary,
      description: preview.description,
      location: preview.location,
      dtstart: preview.dtstart || "",
      dtend: preview.dtend || preview.dtstart || "",
      all_day: preview.all_day,
      timezone: s.config?.locale.timezone || "Europe/Stockholm",
      rrule: preview.rrule || null,
      alarms: preview.alarms?.length ? preview.alarms : [{ trigger: "-PT15M" }],
      attendees: preview.attendees || [],
    });
    setShowEditor(true);
  }


  async function onSelect(sel: DateSelectArg) {
    openNew(sel.start, sel.end, sel.allDay);
  }

  function onDateClick(arg: DateClickArg) {
    setLastClickedDay(arg.date);
  }

  function onEventClick(arg: EventClickArg) {
    const ev = arg.event.extendedProps.calEvent as CalEvent;
    if (arg.jsEvent.shiftKey) {
      arg.jsEvent.preventDefault();
      setMarked((prev) => (prev?.id === ev.id ? null : ev));
      return;
    }
    setSelected(ev);
  }

  async function persistMove(arg: EventDropArg | EventResizeDoneArg) {
    const ev = arg.event.extendedProps.calEvent as CalEvent;
    if (ev.readonly) {
      arg.revert();
      return;
    }
    try {
      const baseUid = ev.uid.includes("::") ? ev.uid.split("::")[0] : ev.uid;
      await saveEvent({
        calendar_id: ev.calendar_id,
        uid: baseUid,
        summary: ev.title,
        description: ev.description,
        location: ev.location,
        dtstart: arg.event.start
          ? toDraftWallClock(arg.event.start, arg.event.allDay)
          : ev.start || "",
        dtend: arg.event.end
          ? toDraftWallClock(arg.event.end, arg.event.allDay)
          : ev.end || "",
        all_day: arg.event.allDay,
        timezone: config?.locale.timezone || "Europe/Stockholm",
        rrule: ev.rrule,
        alarms: ev.alarms,
        attendees: ev.attendees,
        href: ev.href,
        etag: ev.etag,
      });
      await load();
    } catch (e) {
      arg.revert();
      useApp.setState({ error: String(e) });
    }
  }

  async function pasteClipboard() {
    const src = clipboard;
    const day = lastClickedDay;
    if (!src || !day) return;

    const sStart = new Date(src.start || "");
    if (Number.isNaN(sStart.getTime())) {
      throw new Error("copied event has no start time");
    }
    const targetStart = src.all_day
      ? new Date(day.getFullYear(), day.getMonth(), day.getDate())
      : new Date(
          day.getFullYear(),
          day.getMonth(),
          day.getDate(),
          sStart.getHours(),
          sStart.getMinutes(),
        );
    let durMs = 60 * 60 * 1000;
    if (src.end) {
      const sEnd = new Date(src.end);
      if (!Number.isNaN(sEnd.getTime())) durMs = sEnd.getTime() - sStart.getTime();
    }
    const targetEnd = new Date(targetStart.getTime() + durMs);

    const target = pickWritableCalendar(src.calendar_id);
    if (!target) {
      setShowSettings(true);
      throw new Error("no writable calendar");
    }

    await saveEvent({
      calendar_id: target.id,
      uid: undefined,
      summary: src.title,
      description: src.description,
      location: src.location,
      dtstart: toDraftWallClock(targetStart, src.all_day),
      dtend: toDraftWallClock(targetEnd, src.all_day),
      all_day: src.all_day,
      timezone: config?.locale.timezone || "Europe/Stockholm",
      rrule: null,
      alarms: src.alarms,
      attendees: [],
      href: undefined,
      etag: undefined,
    });
    await load();
  }

  function goToDay(date: Date) {
    setView("timeGridDay");
    calRef.current?.getApi().changeView("timeGridDay", date);
  }

  function goToMonth(date: Date) {
    setView("dayGridMonth");
    calRef.current?.getApi().changeView("dayGridMonth", date);
  }

  function handleEventMount(info: EventMountArg) {
    const ev = info.event.extendedProps.calEvent as CalEvent | undefined;
    if (!ev) return;
    const tz = config?.locale.time_24h ?? true;
    info.el.setAttribute(
      "aria-label",
      ev.all_day ? ev.title : `${eventTimeLabel(ev, tz)}, ${ev.title}`,
    );
  }

  function openDayPopover(arg: MoreLinkArg) {
    const t = arg.jsEvent.currentTarget as HTMLElement | null;
    const r = t?.getBoundingClientRect();
    const anchor: DayPopoverAnchor = r
      ? { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
      : { left: 0, top: 0, right: 0, bottom: 0 };
    const key = dayKey(arg.date);
    const timed = getTimedEventsForDayKey(visibleEvents, key).slice().sort((a, b) =>
      (a.start || "").localeCompare(b.start || ""),
    );
    const allDay = getAllDayEventsForDayKey(visibleEvents, key).slice().sort((a, b) =>
      (a.start || "").localeCompare(b.start || ""),
    );
    setDayPopover({ date: arg.date, events: [...allDay, ...timed], anchor });
    // Truthy non-string return: suppress FullCalendar's built-in popover + nav.
    return true as unknown as string;
  }

  function navPrev() {
    calRef.current?.getApi().prev();
  }
  function navNext() {
    calRef.current?.getApi().next();
  }

  if (!ready) {
    return <div className="app" style={{ placeItems: "center", display: "grid" }}>Loading…</div>;
  }

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand" data-tauri-drag-region="deep">
          <h1>Omarcal</h1>
          <p>
            {theme?.name || "omarchy"} · {config?.locale.timezone || "UTC"}
          </p>
        </div>

        <div className="sidebar-section">
          <h2>Search</h2>
          <input
            placeholder="Title, place, notes…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          {searchResults.length > 0 && (
            <div className="search-results" style={{ marginTop: "0.5rem" }}>
              {searchResults.map((r) => (
                <button
                  key={r.id}
                  className="search-hit"
                  onClick={() => setSelected(r)}
                >
                  {r.title}
                  <div className="muted">{r.start}</div>
                </button>
              ))}
            </div>
          )}
        </div>

        <CalendarSidebar />

        <InvitesPanel />

        <div className="sidebar-section">
          <div className="muted" style={{ fontSize: "0.75rem" }}>
            {lastSync ? `Synced ${new Date(lastSync).toLocaleString()}` : "Never synced"}
          </div>
          {lastSyncError && <div className="error">{lastSyncError}</div>}
        </div>
      </aside>

      <main className="main">
        <div className="nav-row" data-tauri-drag-region="deep">
          <button
            type="button"
            className="ghost nav-btn"
            onClick={navPrev}
            aria-label="Previous"
            title="Previous"
          >
            ‹
          </button>
          <button
            type="button"
            className="ghost nav-btn"
            onClick={navNext}
            aria-label="Next"
            title="Next"
          >
            ›
          </button>
          <span className="nav-title">{navTitle}</span>
          {notice && <span className="nav-notice">{notice}</span>}
        </div>
        <div className="toolbar" data-tauri-drag-region="deep">
          <div className="view-toggle">
            {(
              [
                ["timeGridDay", "Day"],
                ["timeGridWeek", "Week"],
                ["dayGridMonth", "Month"],
                ["multiMonthYear", "Year"],
              ] as const
            ).map(([id, label]) => (
              <button
                key={id}
                className={view === id ? "active" : ""}
                onClick={() => {
                  setView(id);
                  calRef.current?.getApi().changeView(id);
                }}
              >
                {label}
              </button>
            ))}
          </div>
          <button className="ghost" onClick={() => calRef.current?.getApi().today()}>
            Today
          </button>
          <button className="primary" onClick={() => openNew()}>
            New event
          </button>
          <div className="spacer" />
          <button onClick={() => sync()} disabled={syncing}>
            {syncing ? "Syncing…" : "Sync"}
          </button>
          <button onClick={() => setShowSettings(true)}>Accounts</button>
        </div>
        {error && <div className="error">{error}</div>}
        {importError && <div className="error">{importError}</div>}
        <div className="calendar-wrap">
          <FullCalendar
            ref={calRef}
            plugins={[
              dayGridPlugin,
              timeGridPlugin,
              multiMonthPlugin,
              interactionPlugin,
            ]}
            initialView={view}
            headerToolbar={false}
            height="100%"
            nowIndicator
            selectable
            editable
            eventStartEditable
            eventDurationEditable
            weekends
            firstDay={config?.locale.week_starts_on ?? 1}
            views={{
              timeGridWeek: {
                weekNumbers: true,
                weekNumberCalculation: "ISO",
                weekNumberFormat: { week: "numeric" },
              },
              dayGridMonth: {
                weekNumbers: true,
                weekNumberCalculation: "ISO",
                weekNumberFormat: { week: "numeric" },
              },
            }}
            slotMinTime="06:00:00"
            slotMaxTime="22:00:00"
            dayMaxEvents
            moreLinkClick={openDayPopover}
            eventTimeFormat={{
              hour: "2-digit",
              minute: "2-digit",
              hour12: !(config?.locale.time_24h ?? true),
            }}
            events={view === "multiMonthYear" ? [] : fcEvents}
            eventContent={renderEventContent}
            eventDidMount={handleEventMount}
            datesSet={(info) => {
              setNavTitle(info.view.title);
              setCurrentDate(info.start);
            }}
            select={onSelect}
            dateClick={onDateClick}
            eventClick={onEventClick}
            eventDrop={persistMove}
            eventResize={persistMove}
          />
          {view === "multiMonthYear" && (
            <YearView
              year={currentDate.getFullYear()}
              events={visibleEvents}
              today={new Date()}
              weekStartsOn={config?.locale.week_starts_on ?? 1}
              onSelectDay={goToDay}
              onSelectMonth={goToMonth}
            />
          )}
        </div>
      </main>

      {selected && !showEditor && (
        <EventDetail
          event={selected}
          onClose={() => setSelected(null)}
          onEdit={() => openEdit(selected)}
          onDelete={async () => {
            await deleteEvent(selected.id);
            setSelected(null);
            await load();
          }}
          onRsvp={async (partstat) => {
            await respondInvite(selected.id, partstat);
            await load();
            // Keep detail open with refreshed event
            const next = useApp
              .getState()
              .events.find((e) => e.id === selected.id);
            if (next) setSelected(next);
            else setSelected(null);
          }}
        />
      )}

      {dayPopover && (
        <DayPopover
          date={dayPopover.date}
          events={dayPopover.events}
          time24h={config?.locale.time_24h ?? true}
          anchor={dayPopover.anchor}
          onClose={() => setDayPopover(null)}
          onSelectEvent={(ev) => {
            setDayPopover(null);
            setSelected(ev);
          }}
        />
      )}

      {showEditor && editorDraft && (
        <EventEditor
          draft={editorDraft}
          calendars={calendars.filter((c) => c.subscribed !== false)}
          timezone={config?.locale.timezone || "Europe/Stockholm"}
          title={editorTitle}
          onClose={() => setShowEditor(false)}
          onSave={async (input) => {
            await saveEvent(input);
            setShowEditor(false);
            await load();
          }}
        />
      )}

      {showSettings && (
        <SettingsModal onClose={() => setShowSettings(false)} />
      )}
    </div>
  );
}
