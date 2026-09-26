# Home Office Tracker

[English](README.md) · [Stáhnout](../../releases/latest) · [Pro vývojáře](CONTRIBUTING.md) · [Architektura](docs/ARCHITECTURE.md)

**Zdarma pro všechny navždy.** Desktopová appka pro macOS a Windows na evidenci pracovní doby na home office.
Zaměstnanec sám spouští a ukončuje práci. Appka měří odpracovaný čas a pauzy
a zaznamenává, v jaké aplikaci jak dlouho byl. Při neaktivitě sama zapne pauzu.
Přehledy jsou po dnech, týdnech a měsících, s exportem do Excelu a CSV.

**Rozsah v1: jen lokálně.** Data zůstávají v počítači zaměstnance, přehled se
zaměstnavateli posílá jako export. Centrální server nebo administrace zatím
nejsou, dají se přidat později (viz Roadmap).

## Co umí

- **Dnes:** datum, hodiny, tlačítka *Začít pracovat* / *Pauza* / *Pokračovat* /
  *Konec práce*, stopky aktuální pracovní akce, aplikace, ve které se právě
  pracuje, dnešní součet, seznam dnešních pracovních akcí a čas po aplikacích.
- **Projekty:** každý si vytváří vlastní projekty (název + barva), před
  začátkem práce jeden vybere a za běhu ho může přepnout (přepnutí = nová
  pracovní akce). Přehledy ukazují čas po projektech a jdou filtrovat na jeden
  projekt. Exporty mají sloupec a list „Projekty“. Projekty se nemažou, jen
  archivují.
- **Více pracovních akcí za den.** Každé *Začít pracovat → Konec práce* je
  samostatná akce.
- **Automatická pauza** po N minutách bez pohybu myši nebo klávesnice
  (výchozí 10 min, nastavitelné). Pauza se počítá **zpětně od poslední
  aktivity**, takže čas nečinnosti se do práce nezapočte. Pokračovat je potřeba
  ručně. Appka pošle notifikaci a vyskočí okno.
- **Uspání nebo vypnutí počítače** během práce: když mezi dvěma kontrolami
  uplyne přes 60 s, pauza se zapne od poslední kontroly. Po pádu appky se
  otevřená akce při dalším spuštění uzavře k poslednímu okamžiku, kdy appka
  běžela (heartbeat každých 10 s).
- **Sleduje se jen název aplikace v popředí** (např. „Microsoft Excel“,
  „Safari“), ne titulky oken, weby ani obsah. Na macOS proto nejsou potřeba
  oprávnění Zpřístupnění ani Záznam obrazovky.
- **Přehledy:** Den / Týden (Po–Ne) / Měsíc s listováním, dlaždice
  (odpracováno, pauzy, pracovní dny, průměr), tabulka po dnech (první začátek,
  poslední konec, graf) a čas v aplikacích. Práce přes půlnoc se rozdělí mezi
  oba dny.
- **Exporty:**
  - **Excel (.xlsx):** listy *Souhrn*, *Po dnech*, *Pracovní akce* a
    *Aplikace*. Doby jsou skutečné časové hodnoty (`[h]:mm`), dají se sčítat.
  - **CSV:** jeden řádek na pracovní akci. Je v UTF-8 s BOM, se středníkem a
    desetinnou čárkou, takže ho český Excel otevře dvojklikem správně.
- **Jazyky:** čeština, angličtina, němčina, španělština, francouzština a
  polština. Výchozí je jazyk systému (nepodporovaný jazyk → angličtina), v
  Nastavení jde přepnout. Přeložené je i to, co vzniká v Rustu: notifikace,
  menu v liště a exporty (názvy listů a sloupců, formát data, desetinná
  čárka/tečka a oddělovač v CSV). Texty okna jsou v `src/i18n.ts`, texty
  Rustu v `src-tauri/src/i18n.rs`. Seznam jazyků musí být v obou souborech
  stejný.
- **Světlý / tmavý režim:** podle systému, nebo vynucený v Nastavení
  (`data-theme` na `<html>` + `setTheme` pro titulek okna).
- **TOP 5 aplikací** + rozbalovací řádek „Ostatní“ se zbytkem. Export do
  Excelu obsahuje všechny aplikace.
- **O aplikaci** v Nastavení: „Zdarma pro všechny navždy“, logo a odkaz na
  www.losenicky.design (otevírá se v prohlížeči přes tauri-plugin-opener,
  povolena je jen tahle doména).
- **Synchronizace přes iCloud (macOS):** volitelně (Nastavení) sdílí
  pracovní akce a projekty mezi Macy se stejným iCloudem. Každý Mac zapisuje
  jen do své složky v kontejneru `iCloud.com.losicek.homeofficetracker`
  (JSON po měsících), ostatní jen čtou, takže nehrozí konflikty. U akcí
  z jiného Macu je v přehledu vidět jeho název. Vyžaduje podepsanou appku
  s profilem Developer ID (`~/.tauri/profiles/HOT_Developer_ID.provisionprofile`).
- **Automatické aktualizace:** appka po spuštění a pak každých 6 hodin
  zkontroluje vydání na GitHubu a nabídne aktualizaci jedním kliknutím
  (podepsané klíčem v `~/.tauri/home-office-tracker.key`, heslo je v
  klíčence). Během práce se neaktualizuje. Ve verzi pro Mac App Store je
  vypnutá (feature `app-store`), tam aktualizuje obchod.
- **Běží v liště.** Zavřením okna se sledování nezastaví. Na macOS je v
  horní liště vidět `▶ 1:23` / `⏸ 1:23`. Na Windows otevře okno levé
  kliknutí na ikonu u hodin, menu je na pravém kliknutí. *Ukončit* v menu
  lišty ukončí i probíhající práci. Druhé spuštění appky jen zobrazí okno
  běžící instance.

## Architektura

[Tauri 2](https://tauri.app): jádro v Rustu, rozhraní v Reactu a TypeScriptu.
Instalátor má jednotky MB a appka běží s malou pamětí, což se hodí pro
program, který jede celý den.

```
src-tauri/src/
├── lib.rs       — Tauri: příkazy pro UI, lišta, vlákno na pozadí (tick 1×/s),
│                  notifikace, zavření okna = schovat
├── tracker.rs   — stavový automat Nepracuje → Pracuje ⇄ Pauza → Konec;
│                  tick(now, idle, app) je čistá funkce nad DB → testovatelné
├── db.rs        — SQLite schéma + migrace + uzavření „visících“ akcí po pádu
├── report.rs    — přehled za období (dny, akce, aplikace), ořez na hranice dní
├── export.rs    — CSV a XLSX (rust_xlsxwriter)
├── i18n.rs      — jazyk (systém → podporovaný jazyk) + texty z Rustu
└── platform.rs  — aplikace v popředí + doba nečinnosti
                   macOS: NSWorkspace.frontmostApplication, CGEventSource…
                   Windows: GetForegroundWindow → exe → FileDescription,
                            GetLastInputInfo
src/
├── App.tsx      — záložky Dnes / Přehledy / Nastavení
├── api.ts       — typy a volání Rust příkazů
├── format.ts    — formát časů, výpočet rozsahů den/týden/měsíc
└── i18n.ts      — texty okna ve všech jazycích
```

**Data:** SQLite `tracker.sqlite` ve složce appky
(macOS: `~/Library/Application Support/com.losicek.homeofficetracker/`,
Windows: `%APPDATA%\com.losicek.homeofficetracker\`). Časy se ukládají v UTC
ms a na místní čas se převádějí až v přehledech.

- `sessions`: pracovní akce (`started_at`, `ended_at`, `last_seen`)
- `segments`: úseky `work` / `pause` (s důvodem `manual` / `idle`)
- `app_usage`: úseky s aplikací v popředí, jen během práce
- `projects`: projekty (`uuid`, název, barva, `archived`, `updated_at`), na které odkazuje `sessions.project_id`
- `settings`: jméno zaměstnance, limit nečinnosti, jazyk, vzhled, naposledy použitý projekt

Podrobný popis stavového automatu, datového modelu a platformních API je v
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) (anglicky).

**macOS detail:** `NSWorkspace` drží aktuální aplikaci v popředí aktuální jen
díky run loopu hlavního vlákna. Ověřeno: v procesu bez run loopu vrací
zastaralou hodnotu. Tick proto aplikaci zjišťuje přes `run_on_main_thread`.

## Vývoj

Potřeba: Node 20+, Rust (`brew install rustup && rustup default stable`).

```sh
npm install
npm run tauri dev          # vývoj s hot-reloadem
cd src-tauri && cargo test # unit testy (tracker, přehledy, export)
npm run tauri build        # macOS .app + .dmg
scripts/release-mac.sh     # podepsaná + notarizovaná univerzální verze pro Mac
cargo run --example probe  # (v src-tauri) ruční test detekce aplikace/nečinnosti
```

**Výstupy sestavení jsou v `src-tauri/target.nosync/`**, ne v `target/`.
Nastavuje to `src-tauri/.cargo/config.toml`, aby iCloud nesynchronizoval
gigabajty mezivýsledků.

### Windows

- **Přes GitHub Actions (doporučeno):** `.github/workflows/build.yml` sestaví
  `.dmg` (universal, Apple Silicon + Intel) i Windows `.msi` a `.exe`. Po
  pushnutí tagu `v*` je přiloží ke konceptu vydání (draft release), při
  ručním spuštění z karty Actions je dá ke stažení jako artefakty.
- **Křížově z Macu:** `brew install nsis llvm lld`, `cargo install cargo-xwin`,
  `rustup target add x86_64-pc-windows-msvc`, pak
  `npm run tauri build -- --runner cargo-xwin --target x86_64-pc-windows-msvc`.
  Vytvoří jen NSIS `.exe` instalátor (MSI jde sestavit jen na Windows).

## Známá omezení / co ověřit

- **Podepsání:** Mac verzi podepíše a notarizuje `scripts/release-mac.sh`.
  Potřebuje certifikát Developer ID Application a uložené přihlašovací
  údaje pro notarizaci, postup je v [CONTRIBUTING.md](CONTRIBUTING.md).
  Windows build nemá code-signing certifikát, takže Windows ukáže
  SmartScreen („Další informace → Přesto spustit“).
- **Windows verze se sestavuje křížově z Macu** (překlad Win32 kódu prošel),
  ale na reálném Windows zatím nebyla spuštěna.
- Zaměstnanec může data teoreticky upravit (SQLite je lokální soubor). Pro
  důvěryhodnou evidenci by byl potřeba server (viz Roadmap).
- Automatické spuštění po přihlášení zatím není.

## Roadmap

- Autostart po přihlášení (tauri-plugin-autostart)
- Ruční oprava záznamu (zapomenutý „Konec práce“) s poznámkou, že šlo o úpravu
- Kategorie aplikací (pracovní / nepracovní) a produktivní čas
- PDF export / tisk přehledu
- Centrální režim: odesílání dat na server + webová administrace pro
  zaměstnavatele
- Podepsání Windows verze, auto-update

## Licence

[MIT](LICENSE) © 2026 Losenicky Design ·
[www.losenicky.design](https://www.losenicky.design)
