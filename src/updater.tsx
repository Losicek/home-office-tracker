import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, Update } from "@tauri-apps/plugin-updater";
import { api } from "./api";
import { useT } from "./i18n";

const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;

type UpdaterState = {
  /** false ve verzi pro App Store — tam aktualizuje obchod */
  enabled: boolean;
  version: string;
  update: Update | null;
  /** výsledek ruční kontroly: "latest" = žádná novější verze */
  checked: "latest" | null;
  checkNow: () => void;
};

/** Kontrola nové verze na GitHubu po startu a pak každých 6 hodin. */
export function useUpdater(): UpdaterState {
  const [enabled, setEnabled] = useState(false);
  const [version, setVersion] = useState("");
  const [update, setUpdate] = useState<Update | null>(null);
  const [checked, setChecked] = useState<"latest" | null>(null);

  const run = useCallback((manual: boolean) => {
    check()
      .then((u) => {
        setUpdate(u);
        if (manual && !u) setChecked("latest");
      })
      .catch(() => {
        // offline / GitHub nedostupný — zkusí se příště
      });
  }, []);

  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
    invoke<boolean>("updates_enabled").then(setEnabled).catch(() => {});
  }, []);

  useEffect(() => {
    if (!enabled) return;
    const first = setTimeout(() => run(false), 5000);
    const id = setInterval(() => run(false), CHECK_EVERY_MS);
    return () => {
      clearTimeout(first);
      clearInterval(id);
    };
  }, [enabled, run]);

  return { enabled, version, update, checked, checkNow: () => run(true) };
}

/** Lišta nahoře: „Je dostupná verze X · Aktualizovat / Později“. */
export function UpdateBanner({ update }: { update: Update }) {
  const t = useT();
  const [hidden, setHidden] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  if (hidden) return null;

  const install = async () => {
    setMessage(null);
    // Restart appky ukončí probíhající práci — aktualizuje se až po ní.
    const status = await api.status();
    if (status.status !== "off") {
      setMessage(t.updateAfterWork);
      return;
    }
    try {
      let total = 0;
      let done = 0;
      setProgress(0);
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") total = event.data.contentLength ?? 0;
        if (event.event === "Progress") {
          done += event.data.chunkLength;
          if (total) setProgress(Math.round((done / total) * 100));
        }
      });
      await relaunch();
    } catch (e) {
      setProgress(null);
      setMessage(t.updateFailed(String(e)));
    }
  };

  return (
    <div className="update-banner">
      <span>
        {progress === null ? t.updateAvailable(update.version) : t.updateDownloading(progress)}
      </span>
      {progress === null && (
        <span className="update-actions">
          <button className="btn primary small" onClick={install}>
            {t.updateNow}
          </button>
          <button className="btn small" onClick={() => setHidden(true)}>
            {t.later}
          </button>
        </span>
      )}
      {message && <span className="update-message">{message}</span>}
    </div>
  );
}
