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
} from "@fullcalendar/core";
import type { EventResizeDoneArg } from "@fullcalendar/interaction";
import "./App.css";
import {
  bootListeners,
  deleteEvent,
  respondInvite,
  saveEvent,
  useApp,
  type CalEvent,
} from "./store";
import { EventEditor } from "./components/EventEditor";
import { SettingsModal } from "./components/SettingsModal";
import { EventDetail } from "./components/EventDetail";
import { CalendarSidebar } from "./components/CalendarSidebar";
import { InvitesPanel } from "./components/InvitesPanel";

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

function toFcEvents(events: CalEvent[]): FCEventInput[] {
  return events.flatMap((e) => {
    if (!e.start) return [];
    const invite = isInvite(e);
    const statusClass = partstatClass(e);
    const base: FCEventInput = {
      id: `${e.id}:${e.start}`,
      title: e.title || "(no title)",
      start: e.start,
      end: e.end || undefined,
      allDay: e.all_day,
      backgroundColor: invite ? "transparent" : e.color,
      borderColor: e.color,
      editable: !e.readonly && !invite,
      classNames: statusClass ? [statusClass] : undefined,
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

  useEffect(() => {
    bootListeners().then(load);
  }, [load]);

  useEffect(() => {
    const onKey = (ev: KeyboardEvent) => {
      const tag = (ev.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (ev.key === "n") {
        ev.preventDefault();
        openNew();
      } else if (ev.key === "t") {
        ev.preventDefault();
        calRef.current?.getApi().today();
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
  }, [selected, calendars, config, defaultCalendarId]);

  const fcEvents = useMemo(() => toFcEvents(events), [events]);

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

    if (!invite || !calEvent) {
      return (
        <div className="fc-event-main-frame">
          {arg.timeText && <div className="fc-event-time">{arg.timeText}</div>}
          <div className="fc-event-title-container">
            <div className="fc-event-title fc-sticky">{arg.event.title}</div>
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
          <span className="fc-event-title">{arg.event.title}</span>
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

  function openNew(start?: Date, end?: Date, allDay = false) {
    const subscribed = calendars.filter((c) => c.subscribed !== false);
    const defaultCal =
      defaultCalendarId != null
        ? subscribed.find(
            (c) =>
              c.id === defaultCalendarId &&
              c.visible &&
              !c.readonly,
          )
        : undefined;
    const firstWritable =
      defaultCal ||
      subscribed.find((c) => c.visible && !c.readonly) ||
      subscribed.find((c) => !c.readonly) ||
      subscribed[0];
    if (!firstWritable) {
      setShowSettings(true);
      return;
    }
    const s = start || new Date();
    const e = end || new Date(s.getTime() + 60 * 60 * 1000);
    setEditorDraft({
      calendar_id: firstWritable.id,
      summary: "",
      description: "",
      location: "",
      dtstart: allDay
        ? s.toISOString().slice(0, 10)
        : s.toISOString().slice(0, 19),
      dtend: allDay
        ? e.toISOString().slice(0, 10)
        : e.toISOString().slice(0, 19),
      all_day: allDay,
      timezone: config?.locale.timezone || "Europe/Stockholm",
      alarms: [{ trigger: "-PT15M" }],
      attendees: [],
    });
    setShowEditor(true);
  }

  function openEdit(ev: CalEvent) {
    const baseUid = ev.uid.includes("::") ? ev.uid.split("::")[0] : ev.uid;
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

  async function onSelect(sel: DateSelectArg) {
    openNew(sel.start, sel.end, sel.allDay);
  }

  function onEventClick(arg: EventClickArg) {
    const ev = arg.event.extendedProps.calEvent as CalEvent;
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
        dtstart: arg.event.start?.toISOString() || ev.start || "",
        dtend:
          arg.event.end?.toISOString() ||
          arg.event.start?.toISOString() ||
          ev.end ||
          "",
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

  if (!ready) {
    return <div className="app" style={{ placeItems: "center", display: "grid" }}>Loading…</div>;
  }

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
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
        <div className="toolbar">
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
            headerToolbar={{
              left: "prev,next",
              center: "title",
              right: "",
            }}
            height="100%"
            nowIndicator
            selectable
            editable
            eventStartEditable
            eventDurationEditable
            weekends
            firstDay={config?.locale.week_starts_on ?? 1}
            slotMinTime="06:00:00"
            slotMaxTime="22:00:00"
            eventTimeFormat={{
              hour: "2-digit",
              minute: "2-digit",
              hour12: !(config?.locale.time_24h ?? true),
            }}
            events={fcEvents}
            eventContent={renderEventContent}
            select={onSelect}
            eventClick={onEventClick}
            eventDrop={persistMove}
            eventResize={persistMove}
          />
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

      {showEditor && editorDraft && (
        <EventEditor
          draft={editorDraft}
          calendars={calendars.filter((c) => c.subscribed !== false)}
          timezone={config?.locale.timezone || "Europe/Stockholm"}
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
