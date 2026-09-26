//! Texty, které vznikají v Rustu (notifikace, lišta, exporty). Texty okna
//! jsou v `src/i18n.ts`; seznam jazyků musí být v obou souborech stejný.

pub const SUPPORTED: [&str; 6] = ["cs", "en", "de", "es", "fr", "pl"];

/// `setting` je kód jazyka nebo "system" → jazyk systému, jinak angličtina.
pub fn resolve(setting: &str) -> &'static str {
    let wanted = if setting == "system" {
        sys_locale::get_locale().unwrap_or_default()
    } else {
        setting.to_string()
    };
    let code = wanted.get(..2).unwrap_or("").to_ascii_lowercase();
    SUPPORTED
        .iter()
        .find(|l| **l == code)
        .copied()
        .unwrap_or("en")
}

pub struct Texts {
    pub notif_title: &'static str,
    /// `{min}` a `{since}` se nahradí.
    pub notif_body: &'static str,
    pub tray_show: &'static str,
    pub tray_quit: &'static str,
    pub tray_not_working: &'static str,
    pub in_progress: &'static str,
    pub csv_headers: [&'static str; 8],
    pub sheets: [&'static str; 4],
    /// Nadpis, zaměstnanec, od, do, odpracováno, pauzy, pracovní dny, počet akcí
    pub summary: [&'static str; 8],
    pub day_headers: [&'static str; 6],
    pub session_headers: [&'static str; 6],
    pub app_headers: [&'static str; 3],
    pub decimal_comma: bool,
    /// chrono formát data pro CSV + Excel formát pro XLSX
    pub date_chrono: &'static str,
    pub date_excel: &'static str,
}

impl Texts {
    pub fn notification(&self, minutes: u32, since: &str) -> String {
        self.notif_body
            .replace("{min}", &minutes.to_string())
            .replace("{since}", since)
    }
}

pub fn texts(lang: &str) -> &'static Texts {
    match lang {
        "cs" => &CS,
        "de" => &DE,
        "es" => &ES,
        "fr" => &FR,
        "pl" => &PL,
        _ => &EN,
    }
}

static CS: Texts = Texts {
    notif_title: "Automatická pauza",
    notif_body:
        "Žádná aktivita přes {min} min — pauza od {since}. Až se vrátíš, klikni na Pokračovat.",
    tray_show: "Zobrazit okno",
    tray_quit: "Ukončit (ukončí i práci)",
    tray_not_working: "nepracuješ",
    in_progress: "probíhá",
    csv_headers: [
        "Zaměstnanec",
        "Datum",
        "Začátek",
        "Konec",
        "Odpracováno (h:mm)",
        "Odpracováno (hod)",
        "Pauzy (h:mm)",
        "Automatické pauzy",
    ],
    sheets: ["Souhrn", "Po dnech", "Pracovní akce", "Aplikace"],
    summary: [
        "Přehled pracovní doby",
        "Zaměstnanec",
        "Období od",
        "Období do",
        "Odpracováno celkem",
        "Pauzy celkem",
        "Počet pracovních dní",
        "Počet pracovních akcí",
    ],
    day_headers: [
        "Datum",
        "První začátek",
        "Poslední konec",
        "Odpracováno",
        "Pauzy",
        "Počet akcí",
    ],
    session_headers: [
        "Datum",
        "Začátek",
        "Konec",
        "Odpracováno",
        "Pauzy",
        "Automatické pauzy",
    ],
    app_headers: ["Aplikace", "Čas", "Podíl"],
    decimal_comma: true,
    date_chrono: "%d.%m.%Y",
    date_excel: "d.m.yyyy",
};

static EN: Texts = Texts {
    notif_title: "Automatic break",
    notif_body:
        "No activity for over {min} min — on break since {since}. Click Resume when you're back.",
    tray_show: "Show window",
    tray_quit: "Quit (also ends work)",
    tray_not_working: "not working",
    in_progress: "in progress",
    csv_headers: [
        "Employee",
        "Date",
        "Start",
        "End",
        "Worked (h:mm)",
        "Worked (hours)",
        "Breaks (h:mm)",
        "Automatic breaks",
    ],
    sheets: ["Summary", "By day", "Work sessions", "Applications"],
    summary: [
        "Working time report",
        "Employee",
        "Period from",
        "Period to",
        "Total worked",
        "Total breaks",
        "Working days",
        "Work sessions",
    ],
    day_headers: [
        "Date",
        "First start",
        "Last end",
        "Worked",
        "Breaks",
        "Sessions",
    ],
    session_headers: [
        "Date",
        "Start",
        "End",
        "Worked",
        "Breaks",
        "Automatic breaks",
    ],
    app_headers: ["Application", "Time", "Share"],
    decimal_comma: false,
    date_chrono: "%d/%m/%Y",
    date_excel: "dd/mm/yyyy",
};

static DE: Texts = Texts {
    notif_title: "Automatische Pause",
    notif_body: "Seit über {min} Min. keine Aktivität – Pause seit {since}. Klicke auf „Fortsetzen“, wenn du zurück bist.",
    tray_show: "Fenster anzeigen",
    tray_quit: "Beenden (beendet auch die Arbeit)",
    tray_not_working: "keine Arbeit",
    in_progress: "läuft",
    csv_headers: [
        "Mitarbeiter",
        "Datum",
        "Beginn",
        "Ende",
        "Gearbeitet (h:mm)",
        "Gearbeitet (Std.)",
        "Pausen (h:mm)",
        "Automatische Pausen",
    ],
    sheets: ["Übersicht", "Nach Tagen", "Arbeitsblöcke", "Anwendungen"],
    summary: [
        "Arbeitszeitbericht",
        "Mitarbeiter",
        "Zeitraum von",
        "Zeitraum bis",
        "Gearbeitet gesamt",
        "Pausen gesamt",
        "Arbeitstage",
        "Arbeitsblöcke",
    ],
    day_headers: ["Datum", "Erster Beginn", "Letztes Ende", "Gearbeitet", "Pausen", "Arbeitsblöcke"],
    session_headers: ["Datum", "Beginn", "Ende", "Gearbeitet", "Pausen", "Automatische Pausen"],
    app_headers: ["Anwendung", "Zeit", "Anteil"],
    decimal_comma: true,
    date_chrono: "%d.%m.%Y",
    date_excel: "dd.mm.yyyy",
};

static ES: Texts = Texts {
    notif_title: "Pausa automática",
    notif_body: "Sin actividad durante más de {min} min: en pausa desde las {since}. Pulsa Continuar cuando vuelvas.",
    tray_show: "Mostrar ventana",
    tray_quit: "Salir (también finaliza el trabajo)",
    tray_not_working: "sin trabajar",
    in_progress: "en curso",
    csv_headers: [
        "Empleado",
        "Fecha",
        "Inicio",
        "Fin",
        "Trabajado (h:mm)",
        "Trabajado (horas)",
        "Pausas (h:mm)",
        "Pausas automáticas",
    ],
    sheets: ["Resumen", "Por días", "Sesiones de trabajo", "Aplicaciones"],
    summary: [
        "Informe de jornada laboral",
        "Empleado",
        "Periodo desde",
        "Periodo hasta",
        "Total trabajado",
        "Total de pausas",
        "Días trabajados",
        "Sesiones de trabajo",
    ],
    day_headers: ["Fecha", "Primer inicio", "Último fin", "Trabajado", "Pausas", "Sesiones"],
    session_headers: ["Fecha", "Inicio", "Fin", "Trabajado", "Pausas", "Pausas automáticas"],
    app_headers: ["Aplicación", "Tiempo", "Proporción"],
    decimal_comma: true,
    date_chrono: "%d/%m/%Y",
    date_excel: "dd/mm/yyyy",
};

static FR: Texts = Texts {
    notif_title: "Pause automatique",
    notif_body: "Aucune activité depuis plus de {min} min — en pause depuis {since}. Cliquez sur Reprendre à votre retour.",
    tray_show: "Afficher la fenêtre",
    tray_quit: "Quitter (termine aussi le travail)",
    tray_not_working: "hors travail",
    in_progress: "en cours",
    csv_headers: [
        "Employé",
        "Date",
        "Début",
        "Fin",
        "Travaillé (h:mm)",
        "Travaillé (heures)",
        "Pauses (h:mm)",
        "Pauses automatiques",
    ],
    sheets: ["Résumé", "Par jour", "Sessions de travail", "Applications"],
    summary: [
        "Rapport du temps de travail",
        "Employé",
        "Période du",
        "Période au",
        "Total travaillé",
        "Total des pauses",
        "Jours travaillés",
        "Sessions de travail",
    ],
    day_headers: ["Date", "Premier début", "Dernière fin", "Travaillé", "Pauses", "Sessions"],
    session_headers: ["Date", "Début", "Fin", "Travaillé", "Pauses", "Pauses automatiques"],
    app_headers: ["Application", "Durée", "Part"],
    decimal_comma: true,
    date_chrono: "%d/%m/%Y",
    date_excel: "dd/mm/yyyy",
};

static PL: Texts = Texts {
    notif_title: "Automatyczna przerwa",
    notif_body:
        "Brak aktywności od ponad {min} min — przerwa od {since}. Kliknij Wznów, gdy wrócisz.",
    tray_show: "Pokaż okno",
    tray_quit: "Zakończ (kończy też pracę)",
    tray_not_working: "nie pracujesz",
    in_progress: "w toku",
    csv_headers: [
        "Pracownik",
        "Data",
        "Początek",
        "Koniec",
        "Przepracowano (h:mm)",
        "Przepracowano (godz.)",
        "Przerwy (h:mm)",
        "Automatyczne przerwy",
    ],
    sheets: ["Podsumowanie", "Według dni", "Sesje pracy", "Aplikacje"],
    summary: [
        "Raport czasu pracy",
        "Pracownik",
        "Okres od",
        "Okres do",
        "Przepracowano łącznie",
        "Przerwy łącznie",
        "Dni pracy",
        "Sesje pracy",
    ],
    day_headers: [
        "Data",
        "Pierwszy początek",
        "Ostatni koniec",
        "Przepracowano",
        "Przerwy",
        "Sesje",
    ],
    session_headers: [
        "Data",
        "Początek",
        "Koniec",
        "Przepracowano",
        "Przerwy",
        "Automatyczne przerwy",
    ],
    app_headers: ["Aplikacja", "Czas", "Udział"],
    decimal_comma: true,
    date_chrono: "%d.%m.%Y",
    date_excel: "dd.mm.yyyy",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_language_wins_and_unknown_falls_back_to_english() {
        assert_eq!(resolve("de"), "de");
        assert_eq!(resolve("pl-PL"), "pl");
        assert_eq!(resolve("ja"), "en");
        assert!(SUPPORTED.contains(&resolve("system")));
    }
}
