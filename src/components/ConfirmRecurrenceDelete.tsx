type Props = {
  isInstance: boolean;
  title: string;
  onClose: () => void;
  onDeleteSingle: () => void;
  onDeleteFuture: () => void;
  onDeleteSeries: () => void;
};

export function ConfirmRecurrenceDelete({ isInstance, title, onClose, onDeleteSingle, onDeleteFuture, onDeleteSeries }: Props) {
  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 460 }}>
        <h2 style={{ marginTop: 0 }}>Delete recurring event</h2>
        <p className="muted" style={{ marginTop: 0, fontSize: "0.82rem" }}>
          “{title || "(no title)"}” is a repeating event.
          {isInstance ? " This is one occurrence." : " This will affect the entire series."}
        </p>
        <div style={{ display: "grid", gap: "0.45rem", marginTop: "0.8rem" }}>
          {isInstance && (
            <>
              <button type="button" onClick={onDeleteSingle} style={{ textAlign: "left", padding: "0.55rem 0.65rem" }}>
                <strong>Only this event</strong>
                <div className="muted" style={{ fontSize: "0.72rem" }}>Hide this occurrence (adds EXDATE). Others stay.</div>
              </button>
              <button type="button" onClick={onDeleteFuture} style={{ textAlign: "left", padding: "0.55rem 0.65rem" }}>
                <strong>This and future events</strong>
                <div className="muted" style={{ fontSize: "0.72rem" }}>Truncate series before this date (sets UNTIL). Keeps earlier events.</div>
              </button>
            </>
          )}
          <button type="button" className="danger" onClick={onDeleteSeries} style={{ textAlign: "left", padding: "0.55rem 0.65rem" }}>
            <strong>{isInstance ? "Entire series" : "Delete series"}</strong>
            <div className="muted" style={{ fontSize: "0.72rem" }}>{isInstance ? "Delete all occurrences." : "Delete every occurrence of this event."}</div>
          </button>
          <button type="button" onClick={onClose} style={{ marginTop: "0.3rem" }}>Cancel</button>
        </div>
      </div>
    </div>
  );
}
