import { useMemo, useState, type MouseEvent } from "react";
import { useApp, type Account, type Calendar } from "../store";

type MenuState = {
  calendarId: number;
  x: number;
  y: number;
};

export function CalendarSidebar() {
  const {
    calendars,
    config,
    defaultCalendarId,
    toggleCalendar,
    setCalendarColor,
    setDefaultCalendar,
    setCalendarSubscribed,
    reorderCalendars,
  } = useApp();

  const [menu, setMenu] = useState<MenuState | null>(null);
  const [colorFor, setColorFor] = useState<number | null>(null);
  const [showUnsubscribed, setShowUnsubscribed] = useState(false);
  const [draggingId, setDraggingId] = useState<number | null>(null);
  const [dragOverId, setDragOverId] = useState<number | null>(null);

  const accounts = config?.accounts ?? [];

  const groups = useMemo(() => {
    const byAccount = new Map<string, Calendar[]>();
    for (const c of calendars) {
      const list = byAccount.get(c.account_id) || [];
      list.push(c);
      byAccount.set(c.account_id, list);
    }
    const orderedAccountIds: string[] = [];
    for (const a of accounts) {
      if (byAccount.has(a.id)) orderedAccountIds.push(a.id);
    }
    for (const id of byAccount.keys()) {
      if (!orderedAccountIds.includes(id)) orderedAccountIds.push(id);
    }
    return orderedAccountIds.map((accountId) => {
      const account = accounts.find((a) => a.id === accountId);
      const all = byAccount.get(accountId) || [];
      const subscribed = all
        .filter((c) => c.subscribed !== false)
        .sort(
          (a, b) =>
            (a.sort_order ?? 0) - (b.sort_order ?? 0) ||
            a.displayname.localeCompare(b.displayname),
        );
      const unsubscribed = all
        .filter((c) => c.subscribed === false)
        .sort((a, b) => a.displayname.localeCompare(b.displayname));
      return { accountId, account, subscribed, unsubscribed };
    });
  }, [calendars, accounts]);

  const unsubscribedTotal = groups.reduce(
    (n, g) => n + g.unsubscribed.length,
    0,
  );

  function accountLabel(account: Account | undefined, accountId: string) {
    return account?.display_name || accountId.slice(0, 8);
  }

  function openMenu(e: MouseEvent, calendarId: number) {
    e.preventDefault();
    e.stopPropagation();
    setMenu({ calendarId, x: e.clientX, y: e.clientY });
    setColorFor(null);
  }

  function closeMenu() {
    setMenu(null);
    setColorFor(null);
  }

  async function onUnsubscribe(id: number) {
    const ok = window.confirm(
      "Stop syncing this calendar in Omarcal? It stays on Nextcloud and you can restore it later.",
    );
    if (!ok) return;
    closeMenu();
    await setCalendarSubscribed(id, false);
  }

  async function onDropReorder(accountId: string, targetId: number) {
    if (draggingId == null || draggingId === targetId) {
      setDraggingId(null);
      setDragOverId(null);
      return;
    }
    const group = groups.find((g) => g.accountId === accountId);
    if (!group) {
      setDraggingId(null);
      setDragOverId(null);
      return;
    }
    const ids = group.subscribed.map((c) => c.id);
    const from = ids.indexOf(draggingId);
    const to = ids.indexOf(targetId);
    if (from < 0 || to < 0) {
      setDraggingId(null);
      setDragOverId(null);
      return;
    }
    ids.splice(from, 1);
    ids.splice(to, 0, draggingId);
    setDraggingId(null);
    setDragOverId(null);
    await reorderCalendars(accountId, ids);
  }

  const menuCal = menu
    ? calendars.find((c) => c.id === menu.calendarId)
    : null;

  return (
    <div
      className="sidebar-section calendar-sidebar"
      style={{ flex: 1, overflow: "auto" }}
    >
      <h2>Calendars</h2>
      {calendars.length === 0 && (
        <p className="muted">Add a Nextcloud account to sync calendars.</p>
      )}

      {groups.map(({ accountId, account, subscribed }) => (
        <div key={accountId} className="cal-account-group">
          {(accounts.length > 1 || groups.length > 1) && (
            <div className="cal-account-label">
              {accountLabel(account, accountId)}
            </div>
          )}

          {subscribed.map((c) => (
            <div
              key={c.id}
              className={[
                "cal-row",
                draggingId === c.id ? "dragging" : "",
                dragOverId === c.id ? "drag-over" : "",
              ]
                .filter(Boolean)
                .join(" ")}
              draggable
              onDragStart={(e) => {
                setDraggingId(c.id);
                e.dataTransfer.effectAllowed = "move";
                e.dataTransfer.setData("text/plain", String(c.id));
              }}
              onDragEnd={() => {
                setDraggingId(null);
                setDragOverId(null);
              }}
              onDragOver={(e) => {
                e.preventDefault();
                if (dragOverId !== c.id) setDragOverId(c.id);
              }}
              onDragLeave={() => {
                if (dragOverId === c.id) setDragOverId(null);
              }}
              onDrop={(e) => {
                e.preventDefault();
                void onDropReorder(accountId, c.id);
              }}
              onContextMenu={(e) => openMenu(e, c.id)}
            >
              <span
                className="cal-drag-handle"
                title="Drag to reorder"
                aria-hidden
              >
                ⋮⋮
              </span>
              <input
                type="checkbox"
                checked={c.visible}
                onChange={(e) => toggleCalendar(c.id, e.target.checked)}
                title="Show on calendar"
              />
              <span className="swatch" style={{ background: c.color }} />
              <span className="cal-name">
                {c.displayname}
                {c.readonly ? " 🔒" : ""}
                {defaultCalendarId === c.id ? (
                  <span className="cal-default-star" title="Default calendar">
                    {" "}
                    ★
                  </span>
                ) : null}
              </span>
              <button
                type="button"
                className="cal-menu-btn ghost"
                aria-label="Calendar options"
                onClick={(e) => openMenu(e, c.id)}
              >
                ⋮
              </button>
            </div>
          ))}
        </div>
      ))}

      {unsubscribedTotal > 0 && (
        <div className="cal-unsubscribed">
          <button
            type="button"
            className="ghost cal-unsubscribed-toggle"
            onClick={() => setShowUnsubscribed((v) => !v)}
          >
            {showUnsubscribed ? "▾" : "▸"} Unsubscribed ({unsubscribedTotal})
          </button>
          {showUnsubscribed &&
            groups.flatMap(({ accountId, account, unsubscribed }) =>
              unsubscribed.map((c) => (
                <div key={c.id} className="cal-row unsubscribed">
                  <span className="swatch" style={{ background: c.color }} />
                  <span className="cal-name muted">
                    {accounts.length > 1
                      ? `${accountLabel(account, accountId)} · `
                      : ""}
                    {c.displayname}
                  </span>
                  <button
                    type="button"
                    className="ghost"
                    onClick={() => setCalendarSubscribed(c.id, true)}
                  >
                    Restore
                  </button>
                </div>
              )),
            )}
        </div>
      )}

      {menu && menuCal && (
        <>
          <div className="cal-menu-backdrop" onClick={closeMenu} />
          <div
            className="cal-context-menu"
            style={{ left: menu.x, top: menu.y }}
            role="menu"
          >
            {!menuCal.readonly && (
              <button
                type="button"
                role="menuitem"
                onClick={async () => {
                  closeMenu();
                  await setDefaultCalendar(menuCal.id);
                }}
              >
                {defaultCalendarId === menuCal.id
                  ? "Default calendar ★"
                  : "Set as default"}
              </button>
            )}
            <button
              type="button"
              role="menuitem"
              onClick={() => setColorFor(menuCal.id)}
            >
              Change color…
            </button>
            {colorFor === menuCal.id && (
              <div className="cal-color-picker">
                <input
                  type="color"
                  value={normalizeColor(menuCal.color)}
                  onChange={async (e) => {
                    await setCalendarColor(menuCal.id, e.target.value);
                    closeMenu();
                  }}
                />
              </div>
            )}
            <button
              type="button"
              role="menuitem"
              className="danger-text"
              onClick={() => onUnsubscribe(menuCal.id)}
            >
              Unsubscribe…
            </button>
          </div>
        </>
      )}
    </div>
  );
}

function normalizeColor(color: string): string {
  if (/^#[0-9a-fA-F]{6}$/.test(color)) return color;
  if (/^#[0-9a-fA-F]{3}$/.test(color)) {
    const r = color[1];
    const g = color[2];
    const b = color[3];
    return `#${r}${r}${g}${g}${b}${b}`;
  }
  return "#829dd4";
}
