import { useEffect, useMemo, useRef, useState } from "react";
import FullCalendar from "@fullcalendar/react";
import dayGridPlugin from "@fullcalendar/daygrid";
import timeGridPlugin from "@fullcalendar/timegrid";
import multiMonthPlugin from "@fullcalendar/multimonth";
import interactionPlugin from "@fullcalendar/interaction";
import luxonPlugin from "@fullcalendar/luxon3";
import { DateTime } from "luxon";
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
import { eventPalette } from "./themeColors";
import { readPreference, writePreference } from "./uiPreferences";
import { ContextMenu } from "./components/ContextMenu";
import { eventContextItems } from "./eventContextMenu";
import { observeCalendarSize } from "./calendarResize";
import {
  bootListeners,
  deleteEvent,
  deleteEventOccurrence,
  previewIcs,
  respondInvite,
  saveConfig,
  saveEvent,
  saveEventOccurrence,
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
import { ConfirmRecurrenceDelete } from "./components/ConfirmRecurrenceDelete";
import { ConfirmRecurrenceEdit } from "./components/ConfirmRecurrenceEdit";

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

function toDraftWallClock(d: Date, allDay: boolean, timeZone?: string): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  if (timeZone) {
    const dt = DateTime.fromJSDate(d, { zone: timeZone });
    if (dt.isValid) {
      if (allDay) return dt.toFormat("yyyy-MM-dd");
      return dt.toFormat("yyyy-MM-dd'T'HH:mm:00");
    }
  }
  if (allDay) {
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  }
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}:00`;
}

function toFcEvents(
  events: CalEvent[],
  marked: CalEvent | null,
  background = "#2e3440",
  foreground = "#d8dee9"
): FCEventInput[] {
  return events.flatMap((e) => {
    if (!e.start) return [];
    const invite = isInvite(e);
    const statusClass = partstatClass(e);
    const isMarked = marked != null && e.id === marked.id && e.start === marked.start;
    const isRecurringEvent = !!e.rrule;
    const palette = eventPalette(e.color, background, foreground);
    const base: FCEventInput = {
      id: `${e.id}:${e.start}`,
      title: e.title || "(no title)",
      start: e.start,
      end: e.end || undefined,
      allDay: e.all_day,
      backgroundColor: invite ? "transparent" : palette.surface,
      borderColor: palette.edge,
      textColor: palette.text,
      // v1: recurring series can be edited via dialog, not dragged (avoids BYDAY shift ambiguity)
      editable: !e.readonly && !invite && !isRecurringEvent,
      classNames: [
        e.all_day ? "om-allday" : "om-timed",
        ...(statusClass ? [statusClass] : []),
        ...(isMarked ? ["om-marked"] : []),
        ...(isRecurringEvent ? ["om-recurring"] : []),
      ],
      extendedProps: { calEvent: e, invite, palette },
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
    outboxCount,
    theme,
    config,
    defaultCalendarId,
  } = useApp();

  // Display timezone: the home timezone from config, or the OS timezone when
  // "follow system timezone" is enabled (iOS-style when traveling).
  const systemTz = (() => {
    try {
      return Intl.DateTimeFormat().resolvedOptions().timeZone;
    } catch {
      return "";
    }
  })();
  const followSystemTz = !!config?.locale.follow_system_tz && !!systemTz;
  const displayTz = followSystemTz
    ? (systemTz as string)
    : config?.locale.timezone || "Europe/Stockholm";
  const calRef = useRef<FullCalendar | null>(null);
  const calendarWrapRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const element = calendarWrapRef.current;
    if (!ready || !element) return;
    return observeCalendarSize(element, () => calRef.current?.getApi().updateSize());
  }, [ready]);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(() =>
    readPreference("sidebar-collapsed")
  );
  useEffect(() => {
    writePreference("sidebar-collapsed", sidebarCollapsed);
  }, [sidebarCollapsed]);
  const [rsvpBusyId, setRsvpBusyId] = useState<number | null>(null);
  const [navTitle, setNavTitle] = useState("");
  const [currentDate, setCurrentDate] = useState<Date>(() => new Date());
  const [dayPopover, setDayPopover] = useState<{
    date: Date;
    events: CalEvent[];
    anchor: DayPopoverAnchor;
  } | null>(null);
  const [eventMenu, setEventMenu] = useState<{
    event: CalEvent;
    x: number;
    y: number;
    trigger: HTMLElement;
  } | null>(null);
  useEffect(() => {
    setEventMenu(null);
  }, [events, view, displayTz, showEditor, showSettings]);
  const [marked, setMarked] = useState<CalEvent | null>(null);
  const [clipboard, setClipboard] = useState<CalEvent | null>(null);
  const [lastClickedDay, setLastClickedDay] = useState<Date | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const noticeTimer = useRef<number | null>(null);
  const [pendingDelete, setPendingDelete] = useState<CalEvent | null>(null);
  const [pendingRecurrenceEdit, setPendingRecurrenceEdit] = useState<CalEvent | null>(null);
  const [recurrenceEditScope, setRecurrenceEditScope] = useState<"single" | "series">();
  const [editingOccurrence, setEditingOccurrence] = useState<CalEvent | null>(null);

  function showNotice(msg: string) {
    setNotice(msg);
    if (noticeTimer.current != null) window.clearTimeout(noticeTimer.current);
    noticeTimer.current = window.setTimeout(() => setNotice(null), 1800);
  }

  function isRecurring(ev: CalEvent): boolean {
    return !!ev.rrule;
  }
  function isInstance(ev: CalEvent): boolean {
    return !!ev.recurrence_id || ev.uid.includes("::");
  }

  function requestDelete(ev: CalEvent) {
    if (ev.readonly) {
      showNotice("Read-only, cannot delete");
      return;
    }
    if (!isRecurring(ev)) {
      // Single event — delete immediately
      const id = ev.id;
      if (selected?.id === ev.id) setSelected(null);
      if (marked?.id === ev.id) setMarked(null);
      void (async () => {
        try {
          await deleteEvent(id);
          await load();
          showNotice("Deleted");
        } catch (e) {
          useApp.setState({ error: String(e) });
        }
      })();
      return;
    }
    // Recurring — show choice
    setPendingDelete(ev);
  }

  async function doDeleteOccurrence(mode: "single" | "future" | "all") {
    const ev = pendingDelete;
    if (!ev) return;
    const id = ev.id;
    const occurrence = ev.recurrence_id || ev.start || "";
    setPendingDelete(null);
    if (selected?.id === ev.id && selected.start === ev.start) setSelected(null);
    if (marked?.id === ev.id && marked.start === ev.start) setMarked(null);
    try {
      if (mode === "all") {
        await deleteEvent(id);
      } else {
        if (!occurrence) {
          await deleteEvent(id);
        } else {
          await deleteEventOccurrence(id, occurrence, mode);
        }
      }
      await load();
      showNotice(
        mode === "single"
          ? "Removed one occurrence"
          : mode === "future"
            ? "Truncated series"
            : "Deleted series"
      );
    } catch (e) {
      useApp.setState({ error: String(e) });
    }
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [load]);

  // Fade out the inline splash once the app is ready.
  useEffect(() => {
    if (ready) {
      const splash = document.getElementById("splash");
      if (splash) {
        splash.classList.add("hidden");
        const t = window.setTimeout(() => splash.remove(), 300);
        return () => window.clearTimeout(t);
      }
    }
  }, [ready]);

  // Safety net: if load hangs (e.g. CSP or invoke blocked), force-hide splash after 5s
  // so the app never appears frozen. User will see the error banner instead.
  useEffect(() => {
    if (ready) return;
    const t = window.setTimeout(() => {
      if (!useApp.getState().ready) {
        console.error("[omacal] load timeout — forcing ready");
        const splash = document.getElementById("splash");
        if (splash) {
          splash.classList.add("hidden");
          window.setTimeout(() => splash.remove(), 300);
        }
        useApp.setState({
          ready: true,
          error: useApp.getState().error || "Load timeout — check terminal / RUST_LOG",
        });
      }
    }, 5000);
    return () => window.clearTimeout(t);
  }, [ready]);

  useEffect(() => {
    if (pendingImport) {
      openImport(pendingImport);
      setPendingImport(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pendingImport]);

  const fcEvents = useMemo(
    () => toFcEvents(events, marked, theme?.background, theme?.foreground),
    [events, marked, theme]
  );

  const visibleEvents = useMemo(() => {
    const visibleIds = new Set(
      calendars.filter((c) => c.visible && c.subscribed !== false).map((c) => c.id)
    );
    return events.filter((e) => visibleIds.has(e.calendar_id));
  }, [events, calendars]);

  useEffect(() => {
    const onKey = (ev: KeyboardEvent) => {
      const tag = (ev.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (showEditor || showSettings || document.querySelector(".event-context-menu")) return;
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
        if (isRecurring(marked)) {
          showNotice("Cannot cut recurring event — duplicate instead");
          return;
        }
        setClipboard(marked);
        const evMarked = marked;
        setMarked(null);
        void (async () => {
          try {
            // cut is delete original
            if (isRecurring(evMarked)) {
              requestDelete(evMarked);
            } else {
              await deleteEvent(evMarked.id);
              await load();
              showNotice("Moved to clipboard");
            }
          } catch (e) {
            useApp.setState({ error: String(e) });
          }
        })();
      } else if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "v") {
        if (!clipboard) return;
        ev.preventDefault();
        void (async () => {
          try {
            await pasteClipboard();
            showNotice("Pasted");
          } catch (e) {
            useApp.setState({ error: String(e) });
          }
        })();
      } else if (ev.key === "Escape") {
        // Stack: close detail drawer → clear mark → clear anchor.
        if (selected) {
          ev.preventDefault();
          setSelected(null);
          return;
        }
        if (marked) {
          ev.preventDefault();
          setMarked(null);
          return;
        }
        if (lastClickedDay) {
          ev.preventDefault();
          setLastClickedDay(null);
          return;
        }
      } else if ((ev.key === "Delete" || ev.key === "Backspace") && marked) {
        ev.preventDefault();
        if (marked.readonly) {
          showNotice("Read-only, cannot delete");
          return;
        }
        const evMarked = marked;
        setMarked(null);
        requestDelete(evMarked);
      } else if (ev.key === "ArrowLeft" || ev.key === "ArrowUp") {
        if (ev.metaKey || ev.ctrlKey) {
          ev.preventDefault();
          calRef.current?.getApi().prev();
          return;
        }
        // Apple-style: plain arrows navigate event selection.
        if (visibleEvents.length > 0) {
          ev.preventDefault();
          const sorted = [...visibleEvents].sort((a, b) =>
            (a.start || "").localeCompare(b.start || "")
          );
          if (!marked) {
            const first = sorted[0];
            setMarked(first);
            if (first.start) setLastClickedDay(new Date(first.start));
            requestAnimationFrame(() => {
              document
                .querySelector(`[data-event-id="${first.id}:${first.start}"]`)
                ?.scrollIntoView({ block: "nearest", behavior: "smooth" });
            });
          } else {
            const idx = sorted.findIndex((e) => e.id === marked.id && e.start === marked.start);
            const nextIdx = idx <= 0 ? sorted.length - 1 : idx - 1;
            const next = sorted[nextIdx];
            setMarked(next);
            if (next.start) setLastClickedDay(new Date(next.start));
            requestAnimationFrame(() => {
              document
                .querySelector(`[data-event-id="${next.id}:${next.start}"]`)
                ?.scrollIntoView({ block: "nearest", behavior: "smooth" });
            });
          }
        }
      } else if (ev.key === "ArrowRight" || ev.key === "ArrowDown") {
        if (ev.metaKey || ev.ctrlKey) {
          ev.preventDefault();
          calRef.current?.getApi().next();
          return;
        }
        if (visibleEvents.length > 0) {
          ev.preventDefault();
          const sorted = [...visibleEvents].sort((a, b) =>
            (a.start || "").localeCompare(b.start || "")
          );
          if (!marked) {
            const first = sorted[0];
            setMarked(first);
            if (first.start) setLastClickedDay(new Date(first.start));
            requestAnimationFrame(() => {
              document
                .querySelector(`[data-event-id="${first.id}:${first.start}"]`)
                ?.scrollIntoView({ block: "nearest", behavior: "smooth" });
            });
          } else {
            const idx = sorted.findIndex((e) => e.id === marked.id && e.start === marked.start);
            const nextIdx = idx >= sorted.length - 1 ? 0 : idx + 1;
            const next = sorted[nextIdx];
            setMarked(next);
            if (next.start) setLastClickedDay(new Date(next.start));
            requestAnimationFrame(() => {
              document
                .querySelector(`[data-event-id="${next.id}:${next.start}"]`)
                ?.scrollIntoView({ block: "nearest", behavior: "smooth" });
            });
          }
        }
      } else if (ev.key === "e" && (marked || selected)) {
        const target = marked ?? selected;
        if (target) openEdit(target);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    selected,
    calendars,
    config,
    defaultCalendarId,
    marked,
    clipboard,
    lastClickedDay,
    showEditor,
    showSettings,
    visibleEvents,
  ]);

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
    const compact = !!(
      arg.event.start &&
      arg.event.end &&
      arg.event.end.getTime() - arg.event.start.getTime() <= 30 * 60 * 1000
    );

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
        <div
          className={`om-event-content${compact ? " om-event-compact" : ""}`}
          style={{
            ["--event-secondary" as string]: arg.event.extendedProps.palette?.secondary,
          }}
        >
          <div className="om-event-title">{title}</div>
          {arg.timeText && (
            <div className="om-event-time">
              {compact ? arg.timeText.replace(/\s*[–-].*$/, "") : arg.timeText}
            </div>
          )}
        </div>
      );
    }

    return (
      <div className="fc-invite-body" style={{ ["--invite-color" as string]: color }}>
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
        ? subscribed.find((c) => c.id === defaultCalendarId && c.visible && !c.readonly)
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
    setRecurrenceEditScope(undefined);
    setEditingOccurrence(null);
    const tz = displayTz;
    setEditorDraft({
      calendar_id: firstWritable.id,
      summary: "",
      description: "",
      location: "",
      dtstart: toDraftWallClock(s, allDay, tz),
      dtend: toDraftWallClock(e, allDay, tz),
      all_day: allDay,
      timezone: tz,
      alarms: [{ trigger: "-PT15M" }],
      attendees: [],
    });
    setShowEditor(true);
  }

  function closeEventMenu(restoreFocus = false) {
    if (restoreFocus && eventMenu?.trigger.isConnected) eventMenu.trigger.focus();
    setEventMenu(null);
  }

  function openEventMenu(ev: CalEvent, x: number, y: number, trigger: HTMLElement) {
    if (showEditor || showSettings || pendingDelete || pendingRecurrenceEdit) return;
    setMarked(ev);
    setSelected(null);
    setEventMenu({ event: ev, x, y, trigger });
  }

  function calendarContextMenu(event: React.MouseEvent<HTMLDivElement>) {
    const element = (event.target as HTMLElement).closest<HTMLElement>("[data-event-id]");
    const ev = events.find((item) => `${item.id}:${item.start}` === element?.dataset.eventId);
    if (!element || !ev) return;
    event.preventDefault();
    event.stopPropagation();
    openEventMenu(ev, event.clientX, event.clientY, element);
  }

  function calendarMenuKey(event: React.KeyboardEvent<HTMLDivElement>) {
    if (event.key !== "ContextMenu" && !(event.shiftKey && event.key === "F10")) return;
    const element = (event.target as HTMLElement).closest<HTMLElement>("[data-event-id]");
    const ev = events.find((item) => `${item.id}:${item.start}` === element?.dataset.eventId);
    if (!element || !ev) return;
    event.preventDefault();
    event.stopPropagation();
    const rect = element.getBoundingClientRect();
    openEventMenu(ev, rect.left, rect.bottom, element);
  }

  function runEventAction(action: string) {
    if (!eventMenu) return;
    const ev = eventMenu.event;
    closeEventMenu(action === "copy");
    if (action !== "copy") setDayPopover(null);
    if (action === "details") setSelected(ev);
    if (action === "copy") {
      setClipboard(ev);
      showNotice("Copied");
    }
    if (action === "edit" && !ev.readonly) openEdit(ev);
    if (action === "edit-single" && !ev.readonly && ev.recurrence_id) openEdit(ev, "single");
    if (action === "edit-series" && !ev.readonly) openEdit(ev, "series");
    if (action === "delete" && !ev.readonly) requestDelete(ev);
    if (action === "duplicate") {
      const target =
        calendars.find((c) => c.id === ev.calendar_id && !c.readonly && c.subscribed !== false) ??
        calendars.find((c) => !c.readonly && c.subscribed !== false);
      if (!target) return;
      setSelected(null);
      setRecurrenceEditScope(undefined);
      setEditingOccurrence(null);
      setEditorTitle("Duplicate event");
      setEditorDraft({
        calendar_id: target.id,
        summary: ev.title,
        description: ev.description,
        location: ev.location,
        dtstart: ev.start || "",
        dtend: ev.end || ev.start || "",
        all_day: ev.all_day,
        timezone: displayTz,
        rrule: null,
        alarms: ev.alarms,
        attendees: [],
      });
      setShowEditor(true);
    }
  }

  function openEdit(ev: CalEvent, scope?: "single" | "series") {
    const baseUid = ev.uid.includes("::") ? ev.uid.split("::")[0] : ev.uid;
    const isInst = isInstance(ev);
    const isRec = isRecurring(ev);
    const canEditSingle = isRec && isInst && !!ev.recurrence_id;
    if (canEditSingle && !scope) {
      setPendingRecurrenceEdit(ev);
      return;
    }
    const editScope = canEditSingle ? scope : undefined;
    const master = editScope === "series" ? ev.series_master : undefined;
    const useMaster = editScope === "series" && (master?.start || ev.master_start);
    setRecurrenceEditScope(editScope);
    setEditingOccurrence(editScope === "single" ? ev : null);
    setEditorTitle(
      editScope === "single"
        ? "Edit this event"
        : editScope === "series"
          ? "Edit entire series"
          : undefined
    );
    setEditorDraft({
      id: ev.id,
      calendar_id: ev.calendar_id,
      uid: baseUid,
      summary: master?.summary ?? ev.title,
      description: master?.description ?? ev.description,
      location: master?.location ?? ev.location,
      dtstart: (useMaster ? (master?.start ?? ev.master_start) : ev.start) || ev.start || "",
      dtend: (useMaster ? (master?.end ?? ev.master_end) : ev.end) || ev.end || ev.start || "",
      all_day: master?.all_day ?? ev.all_day,
      timezone: displayTz,
      rrule: master?.rrule ?? ev.rrule,
      alarms: master?.alarms ?? ev.alarms,
      attendees: master?.attendees ?? ev.attendees,
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
        ? subscribed.find((c) => c.id === s.defaultCalendarId && c.visible && !c.readonly)
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
    setRecurrenceEditScope(undefined);
    setEditingOccurrence(null);
    setEditorDraft({
      calendar_id: target.id,
      // no uid: always create a fresh event on import
      summary: preview.summary,
      description: preview.description,
      location: preview.location,
      dtstart: preview.dtstart || "",
      dtend: preview.dtend || preview.dtstart || "",
      all_day: preview.all_day,
      timezone: displayTz,
      rrule: preview.rrule || null,
      alarms: preview.alarms?.length ? preview.alarms : [{ trigger: "-PT15M" }],
      attendees: preview.attendees || [],
    });
    setShowEditor(true);
  }

  async function onSelect(sel: DateSelectArg) {
    setMarked(null);
    setSelected(null);
    setLastClickedDay(sel.start);
    openNew(sel.start, sel.end, sel.allDay);
  }

  function onDateClick(arg: DateClickArg) {
    // Clicking empty space clears the current mark and anchors the paste target.
    setMarked(null);
    setSelected(null);
    setLastClickedDay(arg.date);
    // Double-click on an empty slot creates a new 1-hour event (iCalendar style).
    if (arg.jsEvent.detail === 2) {
      const start = arg.date;
      const end = new Date(start.getTime() + 60 * 60 * 1000);
      openNew(start, end, arg.allDay);
    }
  }

  function onEventClick(arg: EventClickArg) {
    const ev = arg.event.extendedProps.calEvent as CalEvent;
    // Double-click opens the editor (iCalendar style). The first click already
    // marked the event; the second click (detail === 2) edits it.
    if (arg.jsEvent.detail === 2) {
      arg.jsEvent.preventDefault();
      setMarked(ev);
      setSelected(null);
      openEdit(ev);
      return;
    }
    // Shift+Click is reserved for additive multi-select (v1 toggles single).
    if (arg.jsEvent.shiftKey) {
      arg.jsEvent.preventDefault();
      setMarked((prev) =>
        prev != null && prev.id === ev.id && prev.start === ev.start ? null : ev
      );
      setSelected(null);
      return;
    }
    // Single click: mark (highlight) exclusively; clicking an already-marked
    // event unmarks it.
    setSelected(null);
    setMarked((prev) => (prev != null && prev.id === ev.id && prev.start === ev.start ? null : ev));
    if (ev.start) setLastClickedDay(new Date(ev.start));
  }

  async function persistMove(arg: EventDropArg | EventResizeDoneArg) {
    const ev = arg.event.extendedProps.calEvent as CalEvent;
    if (ev.readonly) {
      arg.revert();
      return;
    }
    try {
      const baseUid = ev.uid.includes("::") ? ev.uid.split("::")[0] : ev.uid;
      const tz = displayTz;
      await saveEvent({
        calendar_id: ev.calendar_id,
        uid: baseUid,
        summary: ev.title,
        description: ev.description,
        location: ev.location,
        dtstart: arg.event.start
          ? toDraftWallClock(arg.event.start, arg.event.allDay, tz)
          : ev.start || "",
        dtend: arg.event.end ? toDraftWallClock(arg.event.end, arg.event.allDay, tz) : ev.end || "",
        all_day: arg.event.allDay,
        timezone: tz,
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
    if (!src) return;
    // Paste target: last clicked day, else fall back to the marked/source event's
    // own day (duplicate adjacent). Keeps source wall-clock HH:mm on that date.
    const day =
      lastClickedDay ??
      (marked ? new Date(marked.start || src.start || "") : new Date(src.start || ""));
    if (Number.isNaN(day.getTime())) {
      throw new Error("no paste target date");
    }

    const tz = displayTz;
    const sStart = new Date(src.start || "");
    if (Number.isNaN(sStart.getTime())) {
      throw new Error("copied event has no start time");
    }
    // Config-driven: keep wall time in event's timezone
    let targetStart: Date;
    if (src.all_day) {
      const dayDt = DateTime.fromJSDate(day, { zone: tz }).startOf("day");
      targetStart = dayDt.toJSDate();
    } else {
      const sDt = DateTime.fromJSDate(sStart, { zone: tz });
      const dayDt = DateTime.fromJSDate(day, { zone: tz });
      const targetDt = dayDt.set({ hour: sDt.hour, minute: sDt.minute, second: 0, millisecond: 0 });
      targetStart = targetDt.toJSDate();
    }
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
      dtstart: toDraftWallClock(targetStart, src.all_day, tz),
      dtend: toDraftWallClock(targetEnd, src.all_day, tz),
      all_day: src.all_day,
      timezone: tz,
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
    const is24h = config?.locale.time_24h ?? true;
    const tz = displayTz;
    info.el.setAttribute(
      "aria-label",
      ev.all_day ? ev.title : `${eventTimeLabel(ev, is24h, tz)}, ${ev.title}`
    );
    info.el.setAttribute("data-event-id", `${ev.id}:${ev.start}`);
    info.el.tabIndex = 0;
    info.el.setAttribute("aria-haspopup", "menu");
    const calendarName = calendars.find((c) => c.id === ev.calendar_id)?.displayname;
    info.el.setAttribute(
      "title",
      [ev.title, ev.all_day ? "All day" : eventTimeLabel(ev, is24h, tz), calendarName, ev.location]
        .filter(Boolean)
        .join("\n")
    );
  }

  function openDayPopover(arg: MoreLinkArg) {
    const t = arg.jsEvent.currentTarget as HTMLElement | null;
    const r = t?.getBoundingClientRect();
    const anchor: DayPopoverAnchor = r
      ? { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
      : { left: 0, top: 0, right: 0, bottom: 0 };
    const tz = displayTz;
    const key = dayKey(arg.date, tz);
    const timed = getTimedEventsForDayKey(visibleEvents, key, tz)
      .slice()
      .sort((a, b) => (a.start || "").localeCompare(b.start || ""));
    const allDay = getAllDayEventsForDayKey(visibleEvents, key, tz)
      .slice()
      .sort((a, b) => (a.start || "").localeCompare(b.start || ""));
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
    // Splash (pulsating calendar icon) is shown in index.html until ready.
    return null;
  }

  return (
    <div className={`app${sidebarCollapsed ? " sidebar-collapsed" : ""}`}>
      <aside
        className="sidebar"
        id="calendar-sidebar"
        hidden={sidebarCollapsed}
        aria-label="Calendars and search"
      >
        <div className="brand" data-tauri-drag-region="deep">
          <h1>Omacal</h1>
          <p title={`Following the ${theme?.name || "Omarchy"} theme`}>Calendar</p>
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
                  onClick={() => {
                    setSelected(null);
                    setMarked((prev) =>
                      prev != null && prev.id === r.id && prev.start === r.start ? null : r
                    );
                    if (r.start) setLastClickedDay(new Date(r.start));
                  }}
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

        <div className="sidebar-section sidebar-status">
          <details className="timezone-details">
            <summary title="Calendar timezone and travel settings">
              <span>{displayTz.split("/").pop()?.replace(/_/g, " ")}</span>
              <span className="timezone-mode">{followSystemTz ? "System time" : "Home time"}</span>
            </summary>
            <div className="timezone-settings">
              <div>Calendar: {config?.locale.timezone}</div>
              <div>System: {systemTz || "Unavailable"}</div>
              <label>
                <input
                  type="checkbox"
                  checked={followSystemTz}
                  disabled={!config || !systemTz}
                  onChange={(e) => {
                    if (!config) return;
                    void saveConfig({
                      ...config,
                      locale: { ...config.locale, follow_system_tz: e.target.checked },
                    });
                  }}
                />
                Follow system timezone
              </label>
              <p>Events are shown in {displayTz}. Their saved times are not changed.</p>
            </div>
          </details>
          <div
            className="sync-status"
            title={
              lastSync
                ? `Last synced ${new Date(lastSync).toLocaleString()}`
                : "No successful sync yet"
            }
          >
            <span className={`sync-dot${lastSyncError ? " has-error" : ""}`} aria-hidden="true" />
            {syncing
              ? "Syncing…"
              : lastSyncError
                ? "Sync needs attention"
                : lastSync
                  ? "Last sync successful"
                  : "Not synced yet"}
          </div>
          {lastSyncError && <div className="error">{lastSyncError}</div>}
          {outboxCount > 0 && (
            <div
              className="muted"
              style={{ fontSize: "0.75rem" }}
              title="Changes made while offline will sync when the CalDAV server is reachable"
            >
              ⟳ {outboxCount} pending change{outboxCount === 1 ? "" : "s"} to sync
            </div>
          )}
        </div>
      </aside>

      <main className="main">
        <header className="toolbar" data-tauri-drag-region="deep">
          <button
            type="button"
            className="ghost sidebar-toggle"
            aria-label={
              sidebarCollapsed ? "Show calendars and search" : "Hide calendars and search"
            }
            aria-expanded={!sidebarCollapsed}
            aria-controls="calendar-sidebar"
            title={sidebarCollapsed ? "Show sidebar" : "Hide sidebar"}
            onClick={() => setSidebarCollapsed((collapsed) => !collapsed)}
          >
            <svg width="18" height="18" viewBox="0 0 20 20" fill="none" aria-hidden="true">
              <rect
                x="2"
                y="3"
                width="16"
                height="14"
                rx="2"
                stroke="currentColor"
                strokeWidth="1.5"
              />
              <path d="M7 3v14" stroke="currentColor" strokeWidth="1.5" />
            </svg>
          </button>
          <div className="date-navigation">
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
            <div className="nav-heading">
              <span className="nav-title">{navTitle}</span>
              {view === "timeGridWeek" && (
                <span className="week-label">
                  Week {DateTime.fromJSDate(currentDate, { zone: displayTz }).weekNumber}
                </span>
              )}
            </div>
          </div>
          <div className="toolbar-views">
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
                  aria-pressed={view === id}
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
          </div>
          <div className="toolbar-actions">
            <button className="primary" onClick={() => openNew()}>
              New event
            </button>
            <button className="ghost" onClick={() => sync()} disabled={syncing}>
              {syncing ? "Syncing…" : "Sync"}
            </button>
            <button className="ghost" onClick={() => setShowSettings(true)}>
              Accounts
            </button>
          </div>
        </header>
        {notice && (
          <div className="nav-notice" role="status">
            {notice}
          </div>
        )}
        {error && <div className="error">{error}</div>}
        {importError && <div className="error">{importError}</div>}
        <div
          className="calendar-wrap"
          ref={calendarWrapRef}
          onContextMenu={calendarContextMenu}
          onKeyDown={calendarMenuKey}
        >
          <FullCalendar
            ref={calRef}
            plugins={[
              dayGridPlugin,
              timeGridPlugin,
              multiMonthPlugin,
              interactionPlugin,
              luxonPlugin,
            ]}
            timeZone={displayTz}
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
            slotMinTime="00:00:00"
            slotMaxTime="24:00:00"
            slotDuration="00:30:00"
            slotLabelInterval="01:00:00"
            slotLabelFormat={{
              hour: "2-digit",
              minute: "2-digit",
              hour12: !(config?.locale.time_24h ?? true),
            }}
            slotEventOverlap={false}
            scrollTime="08:00:00"
            expandRows
            views={{
              timeGridWeek: {
                titleFormat: { year: "numeric", month: "long", day: "numeric" },
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
            dayCellClassNames={(arg: { date: Date }) => {
              if (!lastClickedDay || !arg.date) return [];
              const tz = displayTz;
              const a = DateTime.fromJSDate(arg.date, { zone: tz }).startOf("day");
              const b = DateTime.fromJSDate(lastClickedDay, { zone: tz }).startOf("day");
              return a.hasSame(b, "day") ? ["om-paste-anchor"] : [];
            }}
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            slotLaneClassNames={(arg: any) => {
              if (!lastClickedDay || !arg.date) return [];
              const tz = displayTz;
              const a = DateTime.fromJSDate(arg.date as Date, { zone: tz }).startOf("day");
              const b = DateTime.fromJSDate(lastClickedDay, { zone: tz }).startOf("day");
              return a.hasSame(b, "day") ? ["om-paste-anchor"] : [];
            }}
            datesSet={(info) => {
              setEventMenu(null);
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
          timeZone={displayTz}
          onClose={() => setSelected(null)}
          onEdit={() => openEdit(selected)}
          onDelete={async () => {
            const ev = selected;
            setSelected(null);
            requestDelete(ev);
          }}
          onRsvp={async (partstat) => {
            await respondInvite(selected.id, partstat);
            await load();
            // Keep detail open with refreshed event
            const next = useApp.getState().events.find((e) => e.id === selected.id);
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
          timezone={displayTz}
          onEventContextMenu={openEventMenu}
          anchor={dayPopover.anchor}
          onClose={() => setDayPopover(null)}
          onSelectEvent={(ev) => {
            setDayPopover(null);
            setSelected(null);
            setMarked((prev) =>
              prev != null && prev.id === ev.id && prev.start === ev.start ? null : ev
            );
            if (ev.start) setLastClickedDay(new Date(ev.start));
          }}
        />
      )}

      {showEditor && editorDraft && (
        <EventEditor
          draft={editorDraft}
          calendars={calendars.filter((c) => c.subscribed !== false)}
          timezone={displayTz}
          title={editorTitle}
          recurrenceEditScope={recurrenceEditScope}
          onClose={() => setShowEditor(false)}
          onSave={async (input) => {
            if (recurrenceEditScope === "single" && editingOccurrence?.recurrence_id) {
              await saveEventOccurrence(
                editingOccurrence.id,
                editingOccurrence.recurrence_id,
                input
              );
            } else {
              await saveEvent(input);
            }
            setShowEditor(false);
            await load();
          }}
        />
      )}

      {showSettings && <SettingsModal onClose={() => setShowSettings(false)} />}

      {pendingDelete && (
        <ConfirmRecurrenceDelete
          isInstance={isInstance(pendingDelete)}
          title={pendingDelete.title}
          onClose={() => setPendingDelete(null)}
          onDeleteSingle={() => doDeleteOccurrence("single")}
          onDeleteFuture={() => doDeleteOccurrence("future")}
          onDeleteSeries={() => doDeleteOccurrence("all")}
        />
      )}

      {eventMenu && (
        <ContextMenu
          x={eventMenu.x}
          y={eventMenu.y}
          title={eventMenu.event.title || "(no title)"}
          subtitle={
            eventMenu.event.readonly
              ? "Read-only calendar"
              : eventTimeLabel(eventMenu.event, config?.locale.time_24h ?? true, displayTz)
          }
          items={eventContextItems(
            eventMenu.event,
            calendars.some((c) => !c.readonly && c.subscribed !== false)
          )}
          onAction={runEventAction}
          onClose={closeEventMenu}
        />
      )}
      {pendingRecurrenceEdit && (
        <ConfirmRecurrenceEdit
          title={pendingRecurrenceEdit.title}
          onClose={() => setPendingRecurrenceEdit(null)}
          onEditSingle={() => {
            const event = pendingRecurrenceEdit;
            setPendingRecurrenceEdit(null);
            openEdit(event, "single");
          }}
          onEditSeries={() => {
            const event = pendingRecurrenceEdit;
            setPendingRecurrenceEdit(null);
            openEdit(event, "series");
          }}
        />
      )}
    </div>
  );
}
