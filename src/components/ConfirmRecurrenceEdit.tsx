type Props = {
  title: string;
  onClose: () => void;
  onEditSingle: () => void;
  onEditSeries: () => void;
};

export function ConfirmRecurrenceEdit({ title, onClose, onEditSingle, onEditSeries }: Props) {
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(event) => event.stopPropagation()} style={{ maxWidth: 460 }}>
        <h2>Edit repeating event</h2>
        <p className="muted" style={{ marginTop: 0, fontSize: "0.82rem" }}>
          “{title || "(no title)"}” is one occurrence of a repeating event.
        </p>
        <div style={{ display: "grid", gap: "0.45rem", marginTop: "0.8rem" }}>
          <button
            type="button"
            onClick={onEditSingle}
            style={{ textAlign: "left", padding: "0.55rem 0.65rem" }}
          >
            <strong>Only this event</strong>
            <div className="muted" style={{ fontSize: "0.72rem" }}>
              Change this occurrence. Other events in the series stay the same.
            </div>
          </button>
          <button
            type="button"
            onClick={onEditSeries}
            style={{ textAlign: "left", padding: "0.55rem 0.65rem" }}
          >
            <strong>Entire series</strong>
            <div className="muted" style={{ fontSize: "0.72rem" }}>
              Change the repeating event while keeping existing one-off edits.
            </div>
          </button>
          <button type="button" onClick={onClose} style={{ marginTop: "0.3rem" }}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
