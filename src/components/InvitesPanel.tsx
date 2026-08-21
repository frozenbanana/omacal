import { useMemo, useState, type MouseEvent } from "react";
import { respondInvite, respondInvitesBulk, useApp, type CalEvent } from "../store";

type Filter = "upcoming" | "past" | "all";

function isPastInvite(ev: CalEvent, now: number): boolean {
  const end = ev.end || ev.start;
  if (!end) return false;
  const t = Date.parse(end);
  if (Number.isNaN(t)) return false;
  return t < now;
}

export function InvitesPanel() {
  const { pending, load } = useApp();
  const [expanded, setExpanded] = useState(false);
  const [filter, setFilter] = useState<Filter>("upcoming");
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [status, setStatus] = useState<string>();

  const now = useMemo(() => Date.now(), [pending]);

  const { upcoming, past, filtered } = useMemo(() => {
    const upcoming: CalEvent[] = [];
    const past: CalEvent[] = [];
    for (const p of pending) {
      if (isPastInvite(p, now)) past.push(p);
      else upcoming.push(p);
    }
    const filtered = filter === "upcoming" ? upcoming : filter === "past" ? past : pending;
    return { upcoming, past, filtered };
  }, [pending, now, filter]);

  if (pending.length === 0) return null;

  function toggleOne(id: number) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function selectAllFiltered() {
    setSelected(new Set(filtered.map((p) => p.id)));
  }

  function clearSelection() {
    setSelected(new Set());
  }

  async function rsvpOne(id: number, partstat: string) {
    setBusy(true);
    setBusyId(id);
    setStatus(undefined);
    try {
      await respondInvite(id, partstat);
      setSelected((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
      await load();
    } catch (e) {
      const msg = String(e);
      setStatus(msg);
      useApp.setState({ error: msg });
    } finally {
      setBusy(false);
      setBusyId(null);
    }
  }

  async function bulk(partstat: string, ids: number[]) {
    if (ids.length === 0) return;
    const label =
      partstat === "ACCEPTED" ? "accept" : partstat === "DECLINED" ? "decline" : "maybe";
    const ok = window.confirm(
      `${label[0].toUpperCase()}${label.slice(1)} ${ids.length} invite(s)? This updates them on the server.`
    );
    if (!ok) return;
    setBusy(true);
    setStatus(`Updating ${ids.length}…`);
    try {
      const result = await respondInvitesBulk(ids, partstat);
      setStatus(
        result.failed > 0
          ? `Done: ${result.ok} ok, ${result.failed} failed`
          : `Updated ${result.ok}`
      );
      setSelected(new Set());
      await load();
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  }

  const selectedIds = filtered.map((p) => p.id).filter((id) => selected.has(id));

  return (
    <div className="sidebar-section invites-panel">
      <button
        type="button"
        className="section-collapse-toggle"
        onClick={() => setExpanded((v) => !v)}
        aria-expanded={expanded}
      >
        <span className="section-collapse-arrow">{expanded ? "▾" : "▸"}</span>
        <h2>
          Invites <span className="badge">{pending.length}</span>
        </h2>
        {!expanded && upcoming.length > 0 && (
          <span className="muted invites-summary">{upcoming.length} upcoming</span>
        )}
      </button>

      {expanded && (
        <>
          <div className="invite-filters">
            {(
              [
                ["upcoming", `Upcoming (${upcoming.length})`],
                ["past", `Past (${past.length})`],
                ["all", `All (${pending.length})`],
              ] as const
            ).map(([id, label]) => (
              <button
                key={id}
                type="button"
                className={filter === id ? "active" : "ghost"}
                onClick={() => {
                  setFilter(id);
                  setSelected(new Set());
                }}
              >
                {label}
              </button>
            ))}
          </div>

          <div className="invite-bulk-bar">
            <button
              type="button"
              className="ghost"
              disabled={busy || filtered.length === 0}
              onClick={selectAllFiltered}
            >
              Select all
            </button>
            <button
              type="button"
              className="ghost"
              disabled={busy || selectedIds.length === 0}
              onClick={clearSelection}
            >
              Clear
            </button>
            {past.length > 0 && filter !== "upcoming" && (
              <button
                type="button"
                className="danger"
                disabled={busy}
                onClick={() =>
                  bulk(
                    "DECLINED",
                    past.map((p) => p.id)
                  )
                }
              >
                Decline all past
              </button>
            )}
          </div>

          {selectedIds.length > 0 && (
            <div className="invite-bulk-actions">
              <span className="muted">{selectedIds.length} selected</span>
              <button
                type="button"
                className="primary"
                disabled={busy}
                onClick={() => bulk("ACCEPTED", selectedIds)}
              >
                Accept
              </button>
              <button type="button" disabled={busy} onClick={() => bulk("TENTATIVE", selectedIds)}>
                Maybe
              </button>
              <button
                type="button"
                className="danger"
                disabled={busy}
                onClick={() => bulk("DECLINED", selectedIds)}
              >
                Decline
              </button>
            </div>
          )}

          {status && <div className="muted invites-status">{status}</div>}

          <div className="invite-list">
            {filtered.length === 0 && <p className="muted">No invites in this filter.</p>}
            {filtered.map((p) => (
              <div key={p.id} className="invite-item">
                <label className="invite-select">
                  <input
                    type="checkbox"
                    checked={selected.has(p.id)}
                    disabled={busy}
                    onChange={() => toggleOne(p.id)}
                  />
                  <span>
                    <strong>{p.title || "(no title)"}</strong>
                    <div className="muted">{p.start}</div>
                  </span>
                </label>
                <div className="actions">
                  <button
                    type="button"
                    className="primary"
                    disabled={busy}
                    onClick={(e: MouseEvent) => {
                      e.preventDefault();
                      e.stopPropagation();
                      void rsvpOne(p.id, "ACCEPTED");
                    }}
                  >
                    {busyId === p.id ? "…" : "Accept"}
                  </button>
                  <button
                    type="button"
                    disabled={busy}
                    onClick={(e: MouseEvent) => {
                      e.preventDefault();
                      e.stopPropagation();
                      void rsvpOne(p.id, "TENTATIVE");
                    }}
                  >
                    Maybe
                  </button>
                  <button
                    type="button"
                    className="danger"
                    disabled={busy}
                    onClick={(e: MouseEvent) => {
                      e.preventDefault();
                      e.stopPropagation();
                      void rsvpOne(p.id, "DECLINED");
                    }}
                  >
                    Decline
                  </button>
                </div>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
