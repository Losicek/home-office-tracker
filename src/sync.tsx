import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, Settings, SyncStatus } from "./api";
import { clock, longDate } from "./format";
import { useT } from "./i18n";

function when(ms: number): string {
  const d = new Date(ms);
  const today = new Date();
  return d.toDateString() === today.toDateString() ? clock(ms) : `${longDate(d)} ${clock(ms)}`;
}

/** Karta „Synchronizace přes iCloud“ v Nastavení (jen macOS). */
export function SyncCard({
  settings,
  onToggle,
}: {
  settings: Settings;
  onToggle: (enabled: boolean) => void;
}) {
  const t = useT();
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(() => {
    api.syncStatus().then(setStatus).catch(() => {});
  }, []);
  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 3000);
    const unlisten = listen("tracker-changed", refresh);
    return () => {
      clearInterval(id);
      unlisten.then((u) => u());
    };
  }, [refresh]);

  if (!status?.supported) return null;

  const syncNow = () => {
    setBusy(true);
    api
      .syncNow()
      .finally(() => {
        setBusy(false);
        refresh();
      });
  };

  return (
    <section className="card settings">
      <h2>{t.syncTitle}</h2>
      <label className="toggle">
        <input
          type="checkbox"
          checked={settings.icloudSync}
          onChange={(e) => onToggle(e.target.checked)}
        />
        {t.syncToggle}
      </label>
      <p className="muted">{t.syncHelp}</p>

      {settings.icloudSync && (
        <>
          {status.available === false && <div className="error">{t.syncUnavailable}</div>}
          {status.available === null && <p className="muted">{t.syncChecking}</p>}
          {status.error && <div className="error">{t.syncError(status.error)}</div>}
          {status.available && (
            <>
              <div className="sync-row">
                <span className="muted">
                  {status.lastSync ? t.syncLast(when(status.lastSync)) : t.syncChecking}
                </span>
                <button className="btn small" onClick={syncNow} disabled={busy}>
                  {t.syncNow}
                </button>
              </div>
              <div>
                <strong>{t.syncDevices}</strong>
                {status.devices.length === 0 ? (
                  <p className="muted">{t.syncNoDevices}</p>
                ) : (
                  <ul className="device-list">
                    {status.devices.map((d) => (
                      <li key={d.name}>💻 {d.name}</li>
                    ))}
                  </ul>
                )}
              </div>
            </>
          )}
        </>
      )}
    </section>
  );
}
