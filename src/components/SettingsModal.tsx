import { FormEvent, useState } from "react";
import { addAccount, removeAccount, testAccount, useApp } from "../store";

type Props = {
  onClose: () => void;
};

export function SettingsModal({ onClose }: Props) {
  const { config, load, sync } = useApp();
  const [displayName, setDisplayName] = useState("Personal Nextcloud");
  const [url, setUrl] = useState("https://nextcloud.example.com/remote.php/dav/");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [addresses, setAddresses] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string>();
  const [error, setError] = useState<string>();

  async function onTest() {
    setBusy(true);
    setError(undefined);
    setMessage(undefined);
    try {
      const cals = await testAccount(url, username, password);
      setMessage(`OK — found calendars: ${cals.join(", ") || "(none)"}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onAdd(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(undefined);
    try {
      await addAccount({
        display_name: displayName,
        caldav_url: url,
        username,
        password,
        addresses: addresses
          .split(/[,;\s]+/)
          .map((s) => s.trim())
          .filter(Boolean),
      });
      setPassword("");
      setMessage("Account added. Syncing…");
      await load();
      await sync();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>Accounts</h2>
        <p className="muted">
          Passwords are stored in the system keyring, never in config.toml. Disable vdirsyncer for
          these calendars to avoid sync races.
        </p>

        {(config?.accounts || []).map((a) => (
          <div key={a.id} className="invite-item">
            <strong>{a.display_name}</strong>
            <div className="muted">{a.caldav_url}</div>
            <div className="muted">{a.username}</div>
            <div className="actions">
              <button
                className="danger"
                onClick={async () => {
                  await removeAccount(a.id);
                  await load();
                }}
              >
                Remove
              </button>
            </div>
          </div>
        ))}

        <h2 style={{ marginTop: "1.25rem" }}>Add Nextcloud CalDAV</h2>
        <form className="form-grid" onSubmit={onAdd}>
          <label>
            Display name
            <input value={displayName} onChange={(e) => setDisplayName(e.target.value)} required />
          </label>
          <label>
            CalDAV URL
            <input
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://nextcloud.example.com/remote.php/dav/"
              required
            />
          </label>
          <label>
            Username
            <input value={username} onChange={(e) => setUsername(e.target.value)} required />
          </label>
          <label>
            App password
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              required
            />
          </label>
          <label>
            Your email addresses (for RSVP matching)
            <input
              placeholder="you@example.com"
              value={addresses}
              onChange={(e) => setAddresses(e.target.value)}
            />
          </label>
          {message && <p className="muted">{message}</p>}
          {error && <p className="error">{error}</p>}
          <div className="actions">
            <button type="button" onClick={onTest} disabled={busy}>
              Test connection
            </button>
            <button type="submit" className="primary" disabled={busy}>
              {busy ? "Working…" : "Add account"}
            </button>
            <button type="button" onClick={onClose}>
              Close
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
