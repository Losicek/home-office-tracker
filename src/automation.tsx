import { useEffect, useState } from "react";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { api, Settings } from "./api";
import { useT } from "./i18n";

const IS_MAC = navigator.userAgent.includes("Mac");

/** Nabídka zkratek — musí jít zaregistrovat na macOS i Windows. */
const SHORTCUTS = ["Ctrl+Alt+P", "Ctrl+Alt+W", "Ctrl+Shift+Space", "Alt+Shift+P"];

/** „Ctrl+Alt+P“ → „⌃⌥P“ na Macu, jinak beze změny. */
function shortcutLabel(s: string): string {
  if (!IS_MAC) return s;
  return s
    .replace("Ctrl+", "⌃")
    .replace("Alt+", "⌥")
    .replace("Shift+", "⇧")
    .replace("Space", "␣");
}

/** Nastavení → Automatizace: autostart, připomínky, globální zkratka. */
export function AutomationCard({
  settings,
  onChange,
}: {
  settings: Settings;
  onChange: (s: Settings) => Promise<void>;
}) {
  const t = useT();
  const [autostartSupported, setAutostartSupported] = useState(false);
  const [autostart, setAutostart] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.autostartSupported().then((ok) => {
      setAutostartSupported(ok);
      if (ok) isEnabled().then(setAutostart).catch(() => {});
    });
  }, []);

  const toggleAutostart = async (on: boolean) => {
    setError(null);
    try {
      await (on ? enable() : disable());
      setAutostart(await isEnabled());
    } catch (e) {
      setError(String(e));
    }
  };

  const save = (next: Partial<Settings>) => {
    setError(null);
    onChange({ ...settings, ...next }).catch((e) => setError(String(e)));
  };

  return (
    <section className="card settings">
      <h2>{t.automationTitle}</h2>

      {autostartSupported && (
        <label className="toggle">
          <input
            type="checkbox"
            checked={autostart}
            onChange={(e) => toggleAutostart(e.target.checked)}
          />
          {t.autostart}
        </label>
      )}

      <label className="toggle">
        <input
          type="checkbox"
          checked={settings.remindStart}
          onChange={(e) => save({ remindStart: e.target.checked })}
        />
        {t.remindStart}
      </label>
      {settings.remindStart && (
        <label className="indent">
          {t.remindStartAfter}
          <input
            type="number"
            min={1}
            max={120}
            value={settings.remindStartMinutes}
            onChange={(e) => save({ remindStartMinutes: Number(e.target.value) || 1 })}
          />
        </label>
      )}

      <label>
        {t.breakReminder}
        <input
          type="number"
          min={0}
          max={600}
          step={15}
          value={settings.breakReminderMinutes}
          onChange={(e) => save({ breakReminderMinutes: Number(e.target.value) || 0 })}
        />
      </label>

      <label>
        {t.shortcutLabel}
        <select value={settings.shortcut} onChange={(e) => save({ shortcut: e.target.value })}>
          <option value="">{t.shortcutOff}</option>
          {SHORTCUTS.map((s) => (
            <option key={s} value={s}>
              {shortcutLabel(s)}
            </option>
          ))}
        </select>
      </label>
      <p className="muted">{t.shortcutHelp}</p>
      {error && <div className="error">{error}</div>}
    </section>
  );
}
