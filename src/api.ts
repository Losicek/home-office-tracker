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
  /** Projekt běžící akce; mimo práci naposledy použitý (předvýběr). */
  project: ProjectRef | null;
}

export interface ProjectRef {
  id: number;
  name: string;
  color: string;
}

export interface Project extends ProjectRef {
  archived: boolean;
  totalMs: number;
}

/** Musí odpovídat `projects::PALETTE` v Rustu. */
export const PROJECT_COLORS = [
  "#3b82c4",
  "#1f9d6b",
  "#d9822b",
  "#c2415d",
  "#7c5cc4",
  "#2aa3b8",
  "#b38a1d",
  "#6b7686",
];

/** "all" | "none" (bez projektu) | id projektu jako text */
export type ProjectFilter = string;

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
  projectId: number | null;
  projectName: string | null;
  projectColor: string | null;
}

export interface ProjectRow {
  id: number | null;
  name: string | null;
  color: string | null;
  workedMs: number;
  sessions: number;
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
  projects: ProjectRow[];
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
  start: (project: number | null) => invoke<StatusView>("start_work", { project }),
  switchProject: (project: number | null) =>
    invoke<StatusView>("switch_project", { project }),
  projects: () => invoke<Project[]>("list_projects"),
  createProject: (name: string, color: string) =>
    invoke<number>("create_project", { name, color }),
  updateProject: (p: { id: number; name: string; color: string; archived: boolean }) =>
    invoke<void>("update_project", p),
  pause: () => invoke<StatusView>("pause_work"),
  resume: () => invoke<StatusView>("resume_work"),
  end: () => invoke<StatusView>("end_work"),
  report: (from: string, to: string, project: ProjectFilter = "all") =>
    invoke<Report>("get_report", { from, to, project }),
  exportReport: (
    path: string,
    format: "csv" | "xlsx",
    from: string,
    to: string,
    project: ProjectFilter = "all",
  ) => invoke<void>("export_report", { path, format, from, to, project }),
  settings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
};
