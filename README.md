# Home Office Tracker

**A simple, privacy-friendly working time tracker for people who work from home.**
Free for everyone, forever. For macOS and Windows.

[Čeština](README.cs.md) · [Download](../../releases/latest) · [Contributing](CONTRIBUTING.md) · [Architecture](docs/ARCHITECTURE.md)

---

The employee starts and stops work themselves. Home Office Tracker runs a
stopwatch, records breaks, shows which applications the time went into, and
pauses automatically when the computer is left alone. Daily, weekly and
monthly reports can be exported to Excel or CSV and sent to the employer.

Everything stays on the employee's computer: no account, no server, no cloud.

## Features

- **One-click workflow:** *Start working* → *Pause* / *Resume* → *End work*,
  with a live stopwatch, today's total and the list of today's work sessions.
  A day can have any number of work sessions.
- **Projects:** create your own projects (name + color), pick one before you
  start, and switch it during work. Reports show time per project and can be
  filtered to one project. Archived projects stay in reports.
- **Time per application:** the app in the foreground is recorded while you
  work. The UI shows the top 5 apps and an expandable *Other* row.
- **Automatic break after inactivity:** after N minutes without mouse or
  keyboard input (default 10, configurable) a break starts, *backdated to the
  last activity*, so idle time never counts as work. You get a notification
  and resume with one click.
- **Sleep and crash safe:** if the computer sleeps during work, a break starts
  from the moment it went to sleep. If the app is killed, the open session is
  closed at the last heartbeat (every 10 s) on the next start.
- **Reports:** day / week (Mon–Sun) / month, with totals, working days,
  average per day, per-day table with first start and last end, and app
  usage. Work across midnight is split correctly between the days.
- **Exports:**
  - **Excel (.xlsx)** with *Summary*, *By day*, *Work sessions* and
    *Applications* sheets. Durations are real Excel time values (`[h]:mm`), so
    they can be summed.
  - **CSV** with one row per work session. It uses UTF-8 with BOM, and the
    delimiter and decimal separator follow the app language, so Excel opens it
    correctly with a double-click.
- **6 languages:** Czech, English, German, Spanish, French and Polish.
  The default follows the system language and can be changed in Settings.
  Notifications, the tray menu and exports are translated too.
- **Light & dark mode:** follows the system or can be forced in Settings.
- **Lives in the tray / menu bar:** closing the window keeps tracking. On
  macOS the menu bar shows `▶ 1:23` / `⏸ 1:23`. Only one instance can run.

## Privacy

Home Office Tracker is meant to be run **by the employee, visibly**. It is not
a covert monitoring tool:

- Only the **name of the foreground application** is recorded (e.g. *Microsoft
  Excel*). It does not record window titles, URLs, documents, keystrokes or
  screenshots.
- On macOS no Accessibility or Screen Recording permission is required.
- Data is stored locally in SQLite. Nothing is sent anywhere, and the employee
  decides what to export and share.

## Download & install

Installers are attached to each [GitHub release](../../releases/latest):

| Platform | File |
|---|---|
| macOS 11+ (Apple Silicon & Intel) | `Home Office Tracker_x.y.z_universal.dmg` |
| Windows 10 / 11 (x64) | `Home Office Tracker_x.y.z_x64-setup.exe` or `.msi` |

The Windows installer is not code-signed yet. SmartScreen may warn you: click
*More info → Run anyway*. If a macOS build is not notarized, right-click the
app → *Open* the first time.

## Building from source

Requirements: [Node.js](https://nodejs.org) 20+, [Rust](https://rustup.rs)
(stable), and the [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/)
for your OS (Xcode Command Line Tools on macOS, WebView2 and the MSVC build
tools on Windows).

```sh
git clone <this repository>
cd HomeOfficeTracker
npm install
npm run tauri dev            # run with hot reload
cd src-tauri && cargo test   # unit tests (tracker, reports, export, i18n)
npm run tauri build          # installers for the current OS
```

Build output goes to `src-tauri/target.nosync/` instead of `target/` (set in
`src-tauri/.cargo/config.toml`). The `.nosync` suffix keeps iCloud Drive from
syncing gigabytes of build artifacts.

See [CONTRIBUTING.md](CONTRIBUTING.md) for cross-compiling Windows from a Mac,
signing, releasing, adding a language, and code conventions.

## Tech stack

[Tauri 2](https://tauri.app) with a Rust core (tracking, SQLite via
`rusqlite`, reports, XLSX via `rust_xlsxwriter`, native macOS and Win32 APIs)
and a React + TypeScript UI built with Vite. Installers are a few MB, and
memory use stays low for an app that runs all day.

## Roadmap

- Launch at login
- Manual correction of a forgotten *End work*, marked as edited
- App categories (work / non-work) and productive time
- PDF export / print
- Optional central mode: sync to a server + a web dashboard for employers
- Code signing for Windows, auto-update

Ideas and pull requests are welcome, see [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE) © 2026 Losenicky Design

Designed by **Losenicky Design**: [www.losenicky.design](https://www.losenicky.design)
