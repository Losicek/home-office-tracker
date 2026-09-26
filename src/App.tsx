import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import logoDark from "./assets/losenicky-logo-dark.svg";
import logoLight from "./assets/losenicky-logo-light.svg";
import {
  api,
  AppRow,
  ProjectFilter,
  ProjectRow,
  Report,
  SessionRow,
  Settings,
  StatusView,
  ThemeSetting,
} from "./api";
import {
  clock,
  clockWithSeconds,
  duration,
  longDate,
  Period,
  periodLabel,
  periodRange,
  setFormatLocale,
  shiftAnchor,
  shortDate,
  stopwatch,
  toIso,
} from "./format";
import { DICTIONARIES, I18nContext, Lang, LANGUAGES, useT } from "./i18n";
import {
  ProjectDot,
  ProjectSelect,
  ProjectsView,
  QuickCreateProject,
  useProjects,
} from "./projects";
import { SyncCard } from "./sync";
import { UpdateBanner, useUpdater } from "./updater";
import "./App.css";

type Tab = "today" | "reports" | "projects" | "settings";

/** Překreslení každých `ms` — pro běžící stopky a hodiny. */
function useNow(ms = 500): number {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(id);
  }, [ms]);
  return now;
}

/** Zavolá `fn` hned, při každé změně stavu trackeru a pak pravidelně. */
function useRefresh(fn: () => void, intervalMs: number) {
  useEffect(() => {
    fn();
    const id = setInterval(fn, intervalMs);
    const unlisten = listen("tracker-changed", fn);
    return () => {
      clearInterval(id);
      unlisten.then((u) => u());
    };
  }, [fn, intervalMs]);
}

function resolvedLang(settings: Settings): Lang {
  const lang = settings.resolvedLanguage ?? "en";
  return lang in LANGUAGES ? (lang as Lang) : "en";
}

/** Světlý/tmavý režim: CSS přes data-theme, titulek okna přes Tauri. */
function applyTheme(theme: ThemeSetting) {
  const root = document.documentElement;
  if (theme === "system") delete root.dataset.theme;
  else root.dataset.theme = theme;
  getCurrentWindow()
    .setTheme(theme === "system" ? null : theme)
    .catch(() => {});
}

export default function App() {
  const [tab, setTab] = useState<Tab>("today");
  const [settings, setSettings] = useState<Settings | null>(null);
  const updater = useUpdater();

  useEffect(() => {
    api.settings().then(setSettings);
  }, []);
  useEffect(() => {
    if (settings) applyTheme(settings.theme);
  }, [settings?.theme]);

  if (!settings) return null;

  const lang = resolvedLang(settings);
  setFormatLocale(LANGUAGES[lang].locale);
  document.documentElement.lang = lang;
  const t = DICTIONARIES[lang];

  return (
    <I18nContext.Provider value={t}>
      <div className="app">
        <nav className="tabs">
          <span className="brand">Home Office Tracker</span>
          {(
            [
              ["today", t.tabToday],
              ["reports", t.tabReports],
              ["projects", t.tabProjects],
              ["settings", t.tabSettings],
            ] as [Tab, string][]
          ).map(([id, label]) => (
            <button key={id} className={tab === id ? "tab active" : "tab"} onClick={() => setTab(id)}>
              {label}
            </button>
          ))}
        </nav>
        {updater.update && <UpdateBanner update={updater.update} />}
        {/* key: po změně jazyka se vše vykreslí znovu s novým formátem dat */}
        <main key={lang}>
          {tab === "today" && <TodayView />}
          {tab === "reports" && <ReportsView />}
          {tab === "projects" && <ProjectsView />}
          {tab === "settings" && (
            <SettingsView settings={settings} onChange={setSettings} updater={updater} />
          )}
        </main>
      </div>
    </I18nContext.Provider>
  );
}

function TodayView() {
  const t = useT();
  const now = useNow();
  const [status, setStatus] = useState<StatusView | null>(null);
  const [today, setToday] = useState<{ report: Report; fetchedAt: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [projects, refreshProjects] = useProjects();
  // undefined = předvybraný naposledy použitý projekt z jádra
  const [picked, setPicked] = useState<number | null | undefined>(undefined);

  const refresh = useCallback(() => {
    api.status().then(setStatus).catch((e) => setError(String(e)));
    const iso = toIso(new Date());
    api
      .report(iso, iso)
      .then((report) => setToday({ report, fetchedAt: Date.now() }))
      .catch((e) => setError(String(e)));
  }, []);
  useRefresh(refresh, 5000);

  // Stav se čte jen jednou za pár sekund; mezitím dopočítáváme lokálně.
  useEffect(() => {
    const id = setInterval(() => api.status().then(setStatus).catch(() => {}), 1000);
    return () => clearInterval(id);
  }, []);

  const run = (action: () => Promise<StatusView>) => {
    setError(null);
    action()
      .then((s) => {
        setStatus(s);
        refresh();
      })
      .catch((e) => setError(String(e)));
  };

  const working = status?.status === "working";
  const drift = working && status ? now - status.now : 0;
  const sessionWorked = (status?.sessionWorkedMs ?? 0) + drift;
  const todayWorked = (today?.report.workedMs ?? 0) + (working && today ? now - today.fetchedAt : 0);
  const state = status?.status ?? "off";
  const selectedProject = picked !== undefined ? picked : (status?.project?.id ?? null);
  const statusLabel = {
    off: t.statusOff,
    working: t.statusWorking,
    paused: t.statusPaused,
    auto_paused: t.statusAutoPaused,
  }[state];

  return (
    <div className="today">
      <header className="today-header">
        <div>
          <div className="date">{longDate(new Date(now))}</div>
          <div className="clock">{clockWithSeconds(now)}</div>
        </div>
      </header>

      <section className={`card status-card ${state}`}>
        <div className="status-label">
          <span className="dot" />
          {statusLabel}
          {status?.segmentStartedAt && state !== "working" && state !== "off" && (
            <span className="muted"> {t.since(clock(status.segmentStartedAt))}</span>
          )}
        </div>
        <div className="stopwatch">{stopwatch(sessionWorked)}</div>
        <div className="muted">
          {status?.sessionStartedAt
            ? t.sessionInfo(clock(status.sessionStartedAt), duration(status.sessionPausedMs))
            : t.noSession}
        </div>
        {working && status?.currentApp && (
          <div className="current-app">
            {t.currentApp} <strong>{status.currentApp}</strong>
          </div>
        )}

        <div className="project-bar">
          <span className="muted">{state === "off" ? t.project : t.switchProject}</span>
          {state === "off" ? (
            <ProjectSelect projects={projects} value={selectedProject} onChange={setPicked} />
          ) : (
            <ProjectSelect
              projects={projects}
              value={status?.project?.id ?? null}
              onChange={(id) => run(() => api.switchProject(id))}
            />
          )}
          {state === "off" && (
            <QuickCreateProject
              onCreated={(id) => {
                refreshProjects();
                setPicked(id);
              }}
            />
          )}
        </div>

        <div className="actions">
          {state === "off" && (
            <button
              className="btn primary big"
              onClick={() => {
                run(() => api.start(selectedProject));
                setPicked(undefined);
              }}
            >
              {t.start}
            </button>
          )}
          {state === "working" && (
            <button className="btn warn big" onClick={() => run(api.pause)}>
              {t.pause}
            </button>
          )}
          {(state === "paused" || state === "auto_paused") && (
            <button className="btn primary big" onClick={() => run(api.resume)}>
              {t.resume}
            </button>
          )}
          {state !== "off" && (
            <button className="btn danger big" onClick={() => run(api.end)}>
              {t.end}
            </button>
          )}
        </div>
        {error && <div className="error">{error}</div>}
      </section>

      <div className="grid-2">
        <section className="card">
          <h2>{t.todayTotal}</h2>
          <div className="big-number">{duration(todayWorked)}</div>
          <SessionList sessions={today?.report.sessions ?? []} />
        </section>
        <section className="card">
          <h2>{t.topAppsToday}</h2>
          <AppList apps={today?.report.apps ?? []} />
        </section>
      </div>
    </div>
  );
}

function SessionList({ sessions }: { sessions: SessionRow[] }) {
  const t = useT();
  if (sessions.length === 0) return <p className="muted">{t.noSessions}</p>;
  return (
    <table className="table">
      <thead>
        <tr>
          <th>{t.colFrom}</th>
          <th>{t.colTo}</th>
          <th>{t.project}</th>
          <th className="num">{t.colWork}</th>
          <th className="num">{t.colBreaks}</th>
        </tr>
      </thead>
      <tbody>
        {sessions.map((s) => (
          <tr key={s.id}>
            <td>{clock(s.startedAt)}</td>
            <td>{s.endedAt ? clock(s.endedAt) : <span className="badge">{t.inProgress}</span>}</td>
            <td className="project-cell">
              <ProjectDot color={s.projectColor} />
              <span className={s.projectName ? "" : "muted"}>{s.projectName ?? t.noProject}</span>
              {s.deviceName && (
                <span className="device-badge" title={s.deviceName}>
                  💻 {s.deviceName}
                </span>
              )}
            </td>
            <td className="num">{duration(s.workedMs)}</td>
            <td className="num">
              {duration(s.pausedMs)}
              {s.autoPauses > 0 && (
                <span className="muted" title={t.autoPausesTitle}>
                  {" "}
                  ({s.autoPauses}× auto)
                </span>
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

const TOP_APPS = 5;

function AppList({ apps, limit = TOP_APPS }: { apps: AppRow[]; limit?: number }) {
  const t = useT();
  if (apps.length === 0) return <p className="muted">{t.noApps}</p>;
  const total = apps.reduce((sum, a) => sum + a.ms, 0);
  const top = apps.slice(0, limit);
  const rest = apps.slice(limit);
  const restMs = rest.reduce((sum, a) => sum + a.ms, 0);
  const max = Math.max(top[0].ms, restMs);
  const share = (ms: number) => `${duration(ms)} · ${Math.round((ms / total) * 100)} %`;

  return (
    <ul className="app-list">
      {top.map((a) => (
        <li key={a.name}>
          <div className="app-row">
            <span className="app-name">{a.name}</span>
            <span className="muted">{share(a.ms)}</span>
          </div>
          <div className="bar">
            <div style={{ width: `${(a.ms / max) * 100}%` }} />
          </div>
        </li>
      ))}
      {rest.length > 0 && (
        <li>
          <details className="others">
            <summary>
              <div className="app-row">
                <span className="app-name">
                  {t.others} <span className="muted">({t.appsCount(rest.length)})</span>
                </span>
                <span className="muted">{share(restMs)}</span>
              </div>
              <div className="bar others-bar">
                <div style={{ width: `${(restMs / max) * 100}%` }} />
              </div>
            </summary>
            <ul className="others-list">
              {rest.map((a) => (
                <li key={a.name} className="app-row">
                  <span className="app-name">{a.name}</span>
                  <span className="muted">{share(a.ms)}</span>
                </li>
              ))}
            </ul>
          </details>
        </li>
      )}
    </ul>
  );
}

function ReportsView() {
  const t = useT();
  const [period, setPeriod] = useState<Period>("week");
  const [anchor, setAnchor] = useState(new Date());
  const [report, setReport] = useState<Report | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [filter, setFilter] = useState<ProjectFilter>("all");
  const [projects] = useProjects();

  const { from, to } = periodRange(period, anchor);
  const fromIso = toIso(from);
  const toIsoStr = toIso(to);

  const refresh = useCallback(() => {
    api
      .report(fromIso, toIsoStr, filter)
      .then(setReport)
      .catch((e) => setMessage(String(e)));
  }, [fromIso, toIsoStr, filter]);
  useRefresh(refresh, 30000);

  const doExport = async (format: "csv" | "xlsx") => {
    setMessage(null);
    const path = await save({
      defaultPath: `${t.fileName}_${fromIso}_${toIsoStr}.${format}`,
      filters: [
        format === "csv"
          ? { name: "CSV", extensions: ["csv"] }
          : { name: "Excel", extensions: ["xlsx"] },
      ],
    });
    if (!path) return;
    try {
      await api.exportReport(path, format, fromIso, toIsoStr, filter);
      setMessage(t.savedTo(path));
    } catch (e) {
      setMessage(t.exportFailed(String(e)));
    }
  };

  const workedDays = report?.days.filter((d) => d.workedMs > 0).length ?? 0;
  const maxDay = Math.max(1, ...(report?.days.map((d) => d.workedMs) ?? [1]));

  return (
    <div className="reports">
      <div className="toolbar">
        <div className="segmented">
          {(
            [
              ["day", t.day],
              ["week", t.week],
              ["month", t.month],
            ] as [Period, string][]
          ).map(([id, label]) => (
            <button key={id} className={period === id ? "active" : ""} onClick={() => setPeriod(id)}>
              {label}
            </button>
          ))}
        </div>
        <div className="nav">
          <button className="btn" onClick={() => setAnchor(shiftAnchor(period, anchor, -1))}>
            ‹
          </button>
          <span className="period-label">{periodLabel(period, anchor)}</span>
          <button className="btn" onClick={() => setAnchor(shiftAnchor(period, anchor, 1))}>
            ›
          </button>
          <button className="btn" onClick={() => setAnchor(new Date())}>
            {t.today}
          </button>
        </div>
        <div className="filter">
          <select value={filter} onChange={(e) => setFilter(e.target.value)}>
            <option value="all">{t.allProjects}</option>
            <option value="none">{t.noProject}</option>
            {projects.map((p) => (
              <option key={p.id} value={String(p.id)}>
                {p.name}
                {p.archived ? ` (${t.archived.toLowerCase()})` : ""}
              </option>
            ))}
          </select>
        </div>
        <div className="export">
          <button className="btn" onClick={() => doExport("xlsx")}>
            {t.exportExcel}
          </button>
          <button className="btn" onClick={() => doExport("csv")}>
            {t.exportCsv}
          </button>
        </div>
      </div>
      {message && <div className="notice">{message}</div>}

      <div className="tiles">
        <Tile label={t.tileWorked} value={duration(report?.workedMs ?? 0)} />
        <Tile label={t.tileBreaks} value={duration(report?.pausedMs ?? 0)} />
        <Tile label={t.tileDays} value={String(workedDays)} />
        <Tile
          label={t.tileAverage}
          value={duration(workedDays ? (report?.workedMs ?? 0) / workedDays : 0)}
        />
      </div>

      {filter === "all" && (report?.projects.length ?? 0) > 0 && (
        <section className="card projects-card">
          <h2>{t.tabProjects}</h2>
          <ProjectTotals rows={report!.projects} total={report!.workedMs} />
        </section>
      )}

      <div className="grid-2">
        <section className="card">
          {period === "day" ? (
            <>
              <h2>{t.sessionsTitle}</h2>
              <SessionList sessions={report?.sessions ?? []} />
            </>
          ) : (
            <>
              <h2>{t.byDay}</h2>
              <table className="table">
                <thead>
                  <tr>
                    <th>{t.colDay}</th>
                    <th>{t.colRange}</th>
                    <th className="num">{t.colWork}</th>
                    <th className="bar-col" />
                  </tr>
                </thead>
                <tbody>
                  {report?.days.map((d) => (
                    <tr key={d.date} className={d.workedMs ? "" : "empty"}>
                      <td>{shortDate(d.date)}</td>
                      <td>
                        {d.firstStart && d.lastEnd
                          ? `${clock(d.firstStart)} – ${clock(d.lastEnd)}`
                          : "—"}
                      </td>
                      <td className="num">{d.workedMs ? duration(d.workedMs) : "—"}</td>
                      <td className="bar-col">
                        <div className="bar">
                          <div style={{ width: `${(d.workedMs / maxDay) * 100}%` }} />
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </>
          )}
        </section>
        <section className="card">
          <h2>{t.topApps}</h2>
          <AppList apps={report?.apps ?? []} />
        </section>
      </div>
    </div>
  );
}

function ProjectTotals({ rows, total }: { rows: ProjectRow[]; total: number }) {
  const t = useT();
  const max = Math.max(1, ...rows.map((r) => r.workedMs));
  return (
    <ul className="app-list">
      {rows.map((r) => (
        <li key={r.id ?? "none"}>
          <div className="app-row">
            <span className="app-name">
              <ProjectDot color={r.color} /> {r.name ?? t.noProject}
            </span>
            <span className="muted">
              {duration(r.workedMs)} · {total ? Math.round((r.workedMs / total) * 100) : 0} %
            </span>
          </div>
          <div className="bar">
            <div
              style={{ width: `${(r.workedMs / max) * 100}%`, background: r.color ?? undefined }}
            />
          </div>
        </li>
      ))}
    </ul>
  );
}

function Tile({ label, value }: { label: string; value: string }) {
  return (
    <div className="card tile">
      <div className="muted">{label}</div>
      <div className="tile-value">{value}</div>
    </div>
  );
}

function SettingsView({
  settings,
  onChange,
  updater,
}: {
  settings: Settings;
  onChange: (s: Settings) => void;
  updater: ReturnType<typeof useUpdater>;
}) {
  const t = useT();
  const [draft, setDraft] = useState(settings);
  const [saved, setSaved] = useState(false);

  // Jazyk a vzhled se uloží a projeví hned, bez tlačítka Uložit.
  const saveNow = (next: Settings) => api.saveSettings(next).then(onChange);

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    saveNow({ ...settings, employeeName: draft.employeeName, idleMinutes: draft.idleMinutes }).then(
      () => {
        setSaved(true);
        setTimeout(() => setSaved(false), 2000);
      },
    );
  };

  return (
    <div className="settings-page">
      <section className="card settings">
        <h2>{t.appearanceTitle}</h2>
        <label>
          {t.language}
          <select
            value={settings.language}
            onChange={(e) => saveNow({ ...settings, language: e.target.value })}
          >
            <option value="system">{t.systemDefault}</option>
            {(Object.keys(LANGUAGES) as Lang[]).map((code) => (
              <option key={code} value={code}>
                {LANGUAGES[code].name}
              </option>
            ))}
          </select>
        </label>
        <div className="field">
          <span>{t.theme}</span>
          <div className="segmented">
            {(
              [
                ["system", t.systemDefault],
                ["light", t.themeLight],
                ["dark", t.themeDark],
              ] as [ThemeSetting, string][]
            ).map(([id, label]) => (
              <button
                key={id}
                type="button"
                className={settings.theme === id ? "active" : ""}
                onClick={() => saveNow({ ...settings, theme: id })}
              >
                {label}
              </button>
            ))}
          </div>
        </div>
      </section>

      <SyncCard
        settings={settings}
        onToggle={(icloudSync) => saveNow({ ...settings, icloudSync })}
      />

      <form className="card settings" onSubmit={submit}>
        <h2>{t.settingsTitle}</h2>
        <label>
          {t.employeeName}
          <input
            value={draft.employeeName}
            onChange={(e) => setDraft({ ...draft, employeeName: e.target.value })}
            placeholder={t.namePlaceholder}
          />
        </label>
        <label>
          {t.idleMinutes}
          <input
            type="number"
            min={1}
            max={240}
            value={draft.idleMinutes}
            onChange={(e) => setDraft({ ...draft, idleMinutes: Number(e.target.value) })}
          />
        </label>
        <p className="muted">{t.idleHelp}</p>
        <p className="muted">{t.privacyHelp}</p>
        <div className="actions">
          <button className="btn primary" type="submit">
            {t.save}
          </button>
          {saved && <span className="muted">{t.saved}</span>}
        </div>
      </form>

      <AboutCard updater={updater} />
    </div>
  );
}

const AUTHOR_URL = "https://www.losenicky.design";

function AboutCard({ updater }: { updater: ReturnType<typeof useUpdater> }) {
  const t = useT();
  return (
    <section className="card about">
      <div className="free-badge">{t.free}</div>
      <button className="logo-link" onClick={() => openUrl(AUTHOR_URL)} title="www.losenicky.design">
        <img className="logo-light" src={logoLight} alt="Designed by Losenický" />
        <img className="logo-dark" src={logoDark} alt="Designed by Losenický" />
      </button>
      <button className="link" onClick={() => openUrl(AUTHOR_URL)}>
        www.losenicky.design
      </button>
      {updater.version && <div className="muted">{t.version(updater.version)}</div>}
      {updater.enabled && (
        <div className="update-check">
          <button className="btn small" onClick={updater.checkNow}>
            {t.checkForUpdates}
          </button>
          {updater.checked === "latest" && !updater.update && (
            <span className="muted">{t.upToDate}</span>
          )}
        </div>
      )}
    </section>
  );
}
