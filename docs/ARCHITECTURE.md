# Architecture

This document is for developers who want to understand or change how Home
Office Tracker works internally.

## Overview

```
┌─────────────────────────── Tauri app ────────────────────────────┐
│                                                                  │
│  React UI (src/)                 Rust core (src-tauri/src/)      │
│  ┌──────────────┐   invoke()    ┌──────────────────────────┐     │
│  │ App.tsx      │ ────────────► │ lib.rs   commands, tray, │     │
│  │  Today       │               │          ticker thread   │     │
│  │  Reports     │ ◄──────────── │ tracker.rs  state machine│     │
│  │  Settings    │ "tracker-     │ report.rs   aggregation  │     │
│  │ i18n.ts      │  changed"     │ export.rs   CSV / XLSX   │     │
│  │ format.ts    │  event        │ i18n.rs     Rust strings │     │
│  └──────────────┘               │ platform.rs OS APIs      │     │
│                                 │ db.rs       SQLite       │     │
│                                 └────────────┬─────────────┘     │
└──────────────────────────────────────────────┼───────────────────┘
                                               ▼
                                tracker.sqlite (app data dir)
```

The Rust core owns all state and all data. The UI is a thin view: it calls
commands, polls `get_status` once per second, and re-fetches reports when the
core emits the `tracker-changed` event.

## Rust modules

| Module | Responsibility |
|---|---|
| `lib.rs` | Tauri setup: plugins, tray icon + menu, window close → hide, single instance, the **ticker thread** (1 Hz), commands exposed to the UI, ending work on app exit. |
| `tracker.rs` | The state machine. **Pure logic over the DB:** `tick(now, idle_secs, app)` receives time, idle time and the foreground app from outside, so it is fully unit-testable without an OS. |
| `db.rs` | Schema, migrations (`PRAGMA user_version`), settings key/value, closing dangling sessions after a crash. |
| `report.rs` | Builds a `Report` for an inclusive local-date range: per-day rows, sessions, app totals. Clips intervals to day boundaries. |
| `export.rs` | CSV and XLSX output from a `Report`, localized through `i18n::Texts`. |
| `i18n.rs` | Language resolution (`system` → OS locale → supported language or English) and all user-facing strings produced in Rust. |
| `projects.rs` | Project CRUD (archive instead of delete), color palette, totals. |
| `platform.rs` | Foreground app name, seconds since last input, iCloud container, computer name, per OS. |
| `sync.rs` | Multi-device sync through a shared folder (the app's iCloud container on macOS). |

## State machine

```
            start()                pause()
   Off ───────────────► Working ───────────► Paused
    ▲                    │   ▲                 │
    │        end()       │   └──── resume() ───┤
    └────────────────────┤                     │
    │                    │ idle ≥ limit        │
    │                    │ or sleep gap > 60 s │
    │                    ▼                     │
    │                AutoPaused ── resume() ───┘
    │                    │
    └────── end() ───────┘
```

- **Idle auto-pause:** when `idle_secs ≥ idle_minutes × 60`, the pause starts at
  `now − idle_secs` (never earlier than the current work segment). The idle time
  is therefore removed from work retroactively.
- **Sleep detection:** if two consecutive ticks are more than 60 s apart, the
  machine was asleep. The pause starts at the previous tick, because OS idle
  counters are unreliable across sleep (a key press to wake resets them).
- **Resume is always manual.** The user decides that they are working again.
- **Heartbeat:** `sessions.last_seen` is updated every 10 s. On startup,
  `db::close_dangling_sessions` closes any open session and its segments at
  `last_seen`, so time when the app was not running never counts.
- **App exit** (tray → Quit, Cmd+Q, shutdown) ends the current session.
- **Switching project** during work ends the current session and immediately
  starts a new one with the other project, so every session belongs to exactly
  one project (or none).

## Data model

All timestamps are **UTC milliseconds**. Conversion to local time happens only
in `report.rs` / `export.rs` / the UI.

```sql
projects  (id, uuid UNIQUE, name, color, archived, created_at, updated_at)
sessions  (id, uuid UNIQUE, started_at, ended_at NULL, last_seen,
           project_id NULL → projects,
           device_id NULL)   -- NULL = this computer, else synced from device_id
segments  (id, session_id, kind 'work'|'pause',
           reason NULL|'manual'|'idle', started_at, ended_at NULL)
app_usage (id, session_id, app_name, started_at, ended_at NULL) -- only while working
settings  (key, value)   -- idle_minutes, employee_name, language, theme, last_project_id,
                         -- icloud_sync, device_id, sync_last_export, sync_device_name
devices    (id, name, updated_at)            -- other Macs known from sync
sync_files (path, modified, size)            -- sync files already imported
```

Invariants:

- At most one session has `ended_at IS NULL`.
- Within a session, segments do not overlap and at most one is open.
- `app_usage` rows exist only inside `work` segments. Switching apps closes the
  previous row and opens a new one at the same timestamp.

Schema changes: bump `SCHEMA_VERSION` in `db.rs` and add an
`if version < N { … }` migration step. Never edit an existing step.

Location: `~/Library/Application Support/com.losicek.homeofficetracker/`
(macOS), `%APPDATA%\com.losicek.homeofficetracker\` (Windows).

## Sync (iCloud)

The SQLite database is **never** put into iCloud (concurrent writers would
corrupt it). Instead every device exports its own data as JSON into its own
folder, and only reads the others':

```
<iCloud container>/Data/devices/<device-id>/device.json
                                          /projects.json          all known projects
                                          /sessions-YYYY-MM.json  own sessions by start month (UTC)
```

- One writer per file, so there are no conflicts. Writes are atomic (temp
  file + rename).
- Every 60 s (and right after enabling it, and on quit): export changed months
  and projects since `sync_last_export`, then import changed files of other
  devices (tracked in `sync_files` by mtime + size). Files still in the cloud
  (`.name.icloud`) are requested for download and picked up on the next pass.
- Imported sessions get `sessions.device_id`. They are replaced as a whole by
  `uuid` on re-import and never touch local sessions. A session still running
  on another Mac counts until its `last_seen`.
- Projects merge by `uuid`, and the newer `updated_at` wins (rename, color,
  archive).
- The container requires the app to be signed with the iCloud entitlements
  (`entitlements/developer-id.plist`) and an embedded Developer ID
  provisioning profile (`tauri.icloud.conf.json`). No sandbox is needed for the
  Developer ID build. Unsigned or dev builds just report iCloud as unavailable.

## Reports

`report::build(conn, from, to, now, filter)` takes an **inclusive local-date
range** and a `ProjectFilter` (`All`, `NoProject`, `Project(id)`), which
applies to days, sessions, apps and per-project totals alike.
Week and month are just different ranges computed in the UI (`format.ts →
periodRange`, weeks start on Monday). Open intervals are counted up to `now`.
Every interval is clipped to each local day `[midnight, next midnight)`, which
handles work across midnight and DST days (a missing midnight falls back to
01:00).

## Platform layer

| | macOS | Windows |
|---|---|---|
| Foreground app | `NSWorkspace.frontmostApplication.localizedName` | `GetForegroundWindow` → PID → `QueryFullProcessImageNameW` → version resource `FileDescription` (falls back to exe name) |
| Idle seconds | `CGEventSourceSecondsSinceLastEventType(HIDSystemState, any)` | `GetTickCount() − GetLastInputInfo()` |
| Permissions | none | none |

**macOS gotcha:** `NSWorkspace` keeps `frontmostApplication` current only
while the **main thread's run loop** is running. A process without a run loop
keeps getting a stale value (verified). The ticker therefore asks for the
foreground app via `app.run_on_main_thread`. `cargo run --example probe` pumps
a run loop and prints the live values, which helps when debugging this layer.

## Frontend

- `App.tsx`: the three tabs, settings (language & theme apply instantly).
- `api.ts`: typed wrappers around `invoke()`, mirroring the Rust structs
  (camelCase via `serde(rename_all)`).
- `format.ts`: durations, locale-aware dates, day/week/month ranges.
- `i18n.ts`: dictionaries for all UI strings, typed so every language must
  define every key.
- Theme: CSS custom properties on `:root`. Dark values apply under
  `prefers-color-scheme: dark` unless `data-theme="light"`, or always with
  `data-theme="dark"`.

## Localization

Two dictionaries must stay in sync:

- `src/i18n.ts`: everything in the window.
- `src-tauri/src/i18n.rs`: notifications, tray menu, export headers, sheet
  names, date format, decimal separator.

The language list (`SUPPORTED` in Rust, `LANGUAGES` in TypeScript) must match.
Rust resolves `"system"` to a concrete language and returns it as
`resolvedLanguage`, so the UI and the exports always agree.

## Security / capabilities

`src-tauri/capabilities/default.json` grants the webview only what it needs:
core defaults, window theme, save dialog, notifications, and opening URLs
**only** under `https://www.losenicky.design`. Extend this list deliberately.
