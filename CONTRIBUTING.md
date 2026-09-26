# Contributing

Thanks for helping! This guide covers everything needed to develop, test and
release Home Office Tracker. For how the app works internally, read
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first.

## Setup

1. Install [Node.js](https://nodejs.org) 20+ and [Rust](https://rustup.rs)
   (stable).
2. Install the [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/)
   for your OS:
   - **macOS:** Xcode Command Line Tools (`xcode-select --install`)
   - **Windows:** Microsoft C++ Build Tools and WebView2 (preinstalled on
     Windows 10/11)
3. Then:

```sh
npm install
npm run tauri dev
```

The first Rust build takes a few minutes. Later builds are incremental.

## Project layout

```
.
├── src/                    React + TypeScript UI
│   ├── App.tsx             tabs: Today / Reports / Settings
│   ├── api.ts              typed invoke() wrappers (mirror of Rust structs)
│   ├── format.ts           durations, dates, day/week/month ranges
│   ├── i18n.ts             UI strings for all languages
│   └── assets/             logos (light + dark variant)
├── src-tauri/
│   ├── src/                Rust core (see docs/ARCHITECTURE.md)
│   ├── examples/probe.rs   live check of foreground app / idle detection
│   ├── capabilities/       what the webview is allowed to call
│   ├── icons/              app icons
│   ├── .cargo/config.toml  build output → target.nosync/
│   └── tauri.conf.json     app name, version, window, bundle settings
├── scripts/release-mac.sh  signed + notarized universal macOS build
├── .github/workflows/      CI: tests + installers for macOS and Windows
└── docs/ARCHITECTURE.md
```

## Everyday commands

| Command | What it does |
|---|---|
| `npm run tauri dev` | Run the app with hot reload (UI) and auto-rebuild (Rust) |
| `cd src-tauri && cargo test` | Rust unit tests |
| `npm run build` | Type-check and build the UI only |
| `cd src-tauri && cargo run --example probe` | Print foreground app + idle seconds every second (switch apps while it runs) |
| `npm run tauri build` | Installers for the current OS → `src-tauri/target.nosync/release/bundle/` |

## Testing

**Automated.** `cargo test` covers the state machine (pause accounting,
backdated idle pause, sleep gap, app switches, crash recovery), reports (work
across midnight), export formatting and language resolution. Put new logic in
`tracker.rs` / `report.rs` as functions that take time and inputs as
parameters, so it stays testable without an OS.

**Manual checklist** before a release (on each OS you touch):

- [ ] Start → switch between a few apps → the Today tab shows them with times
- [ ] Pause / Resume: the stopwatch stops and continues, breaks are counted
- [ ] Set the idle limit to 1 min, leave the computer. An auto-break starts,
      backdated. You get a notification and the window opens.
- [ ] Close the window: tracking continues and the tray icon shows the state
- [ ] Tray → Quit ends the session. The next start shows it as finished.
- [ ] Reports: day / week / month navigation. Export Excel and CSV, then open
      both in Excel or Numbers.
- [ ] Switch language and theme in Settings. The UI, tray menu and exports
      change.

## Code conventions

- **Rust:** `cargo fmt` before committing. No `unwrap()` on user data or I/O
  in production paths. Return errors to the UI as `String`.
- **TypeScript:** strict mode, no `any`. Keep `api.ts` types in sync with the
  Rust structs (`#[serde(rename_all = "camelCase")]`).
- **Comments:** existing comments are mostly in Czech. New comments may be in
  English. Explain *why*, not *what*.
- **Privacy is a feature.** Don't add collection of window titles, URLs,
  keystrokes or screenshots. Changes to what data is collected need
  discussion in an issue first.
- Keep the webview's permissions minimal (`src-tauri/capabilities/`).

## Adding a language

1. `src-tauri/src/i18n.rs`: add the code to `SUPPORTED`, add a `static XX:
   Texts` block (copy `EN`), and map it in `texts()`.
2. `src/i18n.ts`: add the language to `LANGUAGES` (native name + locale), add
   a `const xx: Dict` (TypeScript makes you define every key), and add it to
   `DICTIONARIES`.
3. Check plural forms (`appsCount`), the date format and the decimal separator
   (`decimal_comma`, `date_chrono`, `date_excel`).
4. `cargo test` and switch to the language in Settings. Also check an export.

Native-speaker review of existing translations is very welcome too.

## Windows builds

- **CI (easiest):** run the *build* workflow from the Actions tab, or push a
  `v*` tag. It produces `.msi` + `.exe` (and a universal `.dmg`).
- **Cross-compile from a Mac** (NSIS `.exe` only, no MSI):

  ```sh
  brew install nsis llvm lld
  cargo install --locked cargo-xwin
  rustup target add x86_64-pc-windows-msvc
  export PATH="/opt/homebrew/opt/llvm/bin:/opt/homebrew/opt/lld/bin:$PATH"
  npm run tauri build -- --runner cargo-xwin --target x86_64-pc-windows-msvc
  ```

## macOS signing & notarization

`scripts/release-mac.sh` builds a universal (Apple Silicon + Intel) app, signs
it with a *Developer ID Application* certificate, notarizes the `.dmg` with
Apple, and staples the ticket. It needs a one-time setup:

1. A **Developer ID Application** certificate in your keychain (Xcode →
   Settings → Accounts → Manage Certificates → + → Developer ID Application).
2. Notary credentials stored in the keychain:
   `xcrun notarytool store-credentials "homeoffice-notary" --apple-id <you> --team-id <TEAMID>`
   (use an app-specific password from account.apple.com).

Then run `APPLE_TEAM_ID=<TEAMID> scripts/release-mac.sh`. Without the
variable it uses the maintainer's team.

## Releasing

1. Bump the version in `package.json`, `src-tauri/Cargo.toml` and
   `src-tauri/tauri.conf.json` (keep them equal).
2. Commit, then `git tag vX.Y.Z && git push --tags`.
3. CI builds the installers and attaches them to a draft GitHub release.
   Replace the macOS `.dmg` with the notarized one from
   `scripts/release-mac.sh`, write release notes, and publish.

## Pull requests

- One topic per PR, with a short description of *what* and *why*.
- `cargo test` and `npm run build` must pass (CI checks both).
- UI changes: add a screenshot (light and dark if relevant).
- New user-facing text: add it in **all** languages (English is fine as a
  placeholder for languages you don't speak, just mention it in the PR).
