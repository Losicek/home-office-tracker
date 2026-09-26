import { invoke } from "@tauri-apps/api/core";

export type Status = "off" | "working" | "paused" | "auto_paused";

export interface StatusView {
  status: Status;
  now: number;
  sessionStartedAt: number | null;
  segmentStartedAt: number | null;
  sessionWorkedMs: number;
  sessionPausedMs: number;
  currentApp: string | null;
}

export interface DayRow {
  date: string;
  workedMs: number;
  pausedMs: number;
  sessions: number;
  firstStart: number | null;
  lastEnd: number | null;
}

export interface SessionRow {
  id: number;
  startedAt: number;
  endedAt: number | null;
  workedMs: number;
  pausedMs: number;
  autoPauses: number;
}

export interface AppRow {
  name: string;
  ms: number;
}

export interface Report {
  from: string;
  to: string;
  workedMs: number;
  pausedMs: number;
  days: DayRow[];
  sessions: SessionRow[];
  apps: AppRow[];
}

export type ThemeSetting = "system" | "light" | "dark";

export interface Settings {
  idleMinutes: number;
  employeeName: string;
  /** "system" nebo kód jazyka */
  language: string;
  theme: ThemeSetting;
  /** Jen pro čtení — jazyk, který se opravdu použije. */
  resolvedLanguage?: string;
}

export const api = {
  status: () => invoke<StatusView>("get_status"),
  start: () => invoke<StatusView>("start_work"),
  pause: () => invoke<StatusView>("pause_work"),
  resume: () => invoke<StatusView>("resume_work"),
  end: () => invoke<StatusView>("end_work"),
  report: (from: string, to: string) => invoke<Report>("get_report", { from, to }),
  exportReport: (path: string, format: "csv" | "xlsx", from: string, to: string) =>
    invoke<void>("export_report", { path, format, from, to }),
  settings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
};
