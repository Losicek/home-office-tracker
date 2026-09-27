//! Exporty přehledu: CSV (pracovní akce, jeden řádek na akci) a Excel
//! (souhrn, po dnech, pracovní akce, aplikace).
//!
//! Texty, formát data i desetinný oddělovač jsou podle jazyka appky. CSV má
//! UTF-8 BOM; u jazyků s desetinnou čárkou je oddělovač středník (tak ho
//! místní Excel otevře dvojklikem správně), jinak čárka.

use crate::i18n::Texts;
use crate::report::Report;
use chrono::{Datelike, Local, NaiveDate, TimeZone};
use rust_xlsxwriter::{ExcelDateTime, Format, Workbook, XlsxError};
use std::path::Path;

const MS_PER_DAY: f64 = 86_400_000.0;

fn local_time(ms: i64) -> String {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_default()
}

fn local_date(ms: i64) -> NaiveDate {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|dt| dt.date_naive())
        .unwrap_or_default()
}

fn hmm(ms: i64) -> String {
    let minutes = ms / 60_000;
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

fn hours(ms: i64, decimal_comma: bool) -> String {
    let h = format!("{:.2}", ms as f64 / 3_600_000.0);
    if decimal_comma {
        h.replace('.', ",")
    } else {
        h
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([';', ',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// „upraveno ručně“ / „přidáno ručně“ / nic.
fn modification<'a>(s: &crate::report::SessionRow, t: &'a Texts) -> &'a str {
    if s.manual {
        t.manual
    } else if s.edited {
        t.edited
    } else {
        ""
    }
}

fn project_name<'a>(name: &'a Option<String>, t: &'a Texts) -> &'a str {
    name.as_deref().unwrap_or(t.no_project)
}

pub fn to_csv(report: &Report, employee: &str, t: &Texts) -> String {
    let sep = if t.decimal_comma { ";" } else { "," };
    let mut out = String::from("\u{FEFF}");
    out.push_str(&t.csv_headers.map(csv_field).join(sep));
    for h in [t.project, t.note, t.modified] {
        out.push_str(sep);
        out.push_str(&csv_field(h));
    }
    out.push('\n');
    for s in &report.sessions {
        let row = [
            csv_field(employee),
            local_date(s.started_at).format(t.date_chrono).to_string(),
            local_time(s.started_at),
            s.ended_at
                .map(local_time)
                .unwrap_or_else(|| t.in_progress.into()),
            hmm(s.worked_ms),
            csv_field(&hours(s.worked_ms, t.decimal_comma)),
            hmm(s.paused_ms),
            s.auto_pauses.to_string(),
            csv_field(project_name(&s.project_name, t)),
            csv_field(s.note.as_deref().unwrap_or("")),
            csv_field(modification(s, t)),
        ];
        out.push_str(&row.join(sep));
        out.push('\n');
    }
    out
}

fn excel_date(date: NaiveDate) -> Result<ExcelDateTime, XlsxError> {
    ExcelDateTime::from_ymd(date.year() as u16, date.month() as u8, date.day() as u8)
}

/// `project_label` = za jaký projekt export je (filtr v přehledu).
pub fn to_xlsx(
    report: &Report,
    employee: &str,
    project_label: &str,
    t: &Texts,
    path: &Path,
) -> Result<(), XlsxError> {
    let mut wb = Workbook::new();
    let bold = Format::new().set_bold();
    let header = Format::new().set_bold().set_background_color("#E8EEF6");
    let duration = Format::new().set_num_format("[h]:mm");
    let date_fmt = Format::new().set_num_format(t.date_excel);

    let from = NaiveDate::parse_from_str(&report.from, "%Y-%m-%d").unwrap_or_default();
    let to = NaiveDate::parse_from_str(&report.to, "%Y-%m-%d").unwrap_or_default();
    let days_worked = report.days.iter().filter(|d| d.worked_ms > 0).count();

    // Souhrn
    let ws = wb.add_worksheet().set_name(t.sheets[0])?;
    ws.set_column_width(0, 26)?;
    ws.set_column_width(1, 16)?;
    ws.write_string_with_format(0, 0, t.summary[0], &bold)?;
    ws.write_string(2, 0, t.summary[1])?;
    ws.write_string(2, 1, employee)?;
    ws.write_string(3, 0, t.summary[2])?;
    ws.write_datetime_with_format(3, 1, &excel_date(from)?, &date_fmt)?;
    ws.write_string(4, 0, t.summary[3])?;
    ws.write_datetime_with_format(4, 1, &excel_date(to)?, &date_fmt)?;
    ws.write_string(5, 0, t.summary[4])?;
    ws.write_number_with_format(5, 1, report.worked_ms as f64 / MS_PER_DAY, &duration)?;
    ws.write_string(6, 0, t.summary[5])?;
    ws.write_number_with_format(6, 1, report.paused_ms as f64 / MS_PER_DAY, &duration)?;
    ws.write_string(7, 0, t.summary[6])?;
    ws.write_number(7, 1, days_worked as f64)?;
    ws.write_string(8, 0, t.summary[7])?;
    ws.write_number(8, 1, report.sessions.len() as f64)?;
    ws.write_string(9, 0, t.project)?;
    ws.write_string(9, 1, project_label)?;

    // Po dnech
    let ws = wb.add_worksheet().set_name(t.sheets[1])?;
    for (col, title) in t.day_headers.iter().enumerate() {
        ws.write_string_with_format(0, col as u16, *title, &header)?;
        ws.set_column_width(col as u16, 15)?;
    }
    for (i, d) in report.days.iter().enumerate() {
        let row = i as u32 + 1;
        let date = NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").unwrap_or_default();
        ws.write_datetime_with_format(row, 0, &excel_date(date)?, &date_fmt)?;
        if let Some(t) = d.first_start {
            ws.write_string(row, 1, local_time(t))?;
        }
        if let Some(t) = d.last_end {
            ws.write_string(row, 2, local_time(t))?;
        }
        ws.write_number_with_format(row, 3, d.worked_ms as f64 / MS_PER_DAY, &duration)?;
        ws.write_number_with_format(row, 4, d.paused_ms as f64 / MS_PER_DAY, &duration)?;
        ws.write_number(row, 5, d.sessions as f64)?;
    }

    // Pracovní akce
    let ws = wb.add_worksheet().set_name(t.sheets[2])?;
    for (col, title) in t
        .session_headers
        .iter()
        .chain([&t.project, &t.note, &t.modified])
        .enumerate()
    {
        ws.write_string_with_format(0, col as u16, *title, &header)?;
        ws.set_column_width(col as u16, 16)?;
    }
    ws.set_column_width(6, 28)?;
    ws.set_column_width(7, 40)?;
    ws.set_column_width(8, 18)?;
    for (i, s) in report.sessions.iter().enumerate() {
        let row = i as u32 + 1;
        ws.write_datetime_with_format(row, 0, &excel_date(local_date(s.started_at))?, &date_fmt)?;
        ws.write_string(row, 1, local_time(s.started_at))?;
        ws.write_string(
            row,
            2,
            s.ended_at
                .map(local_time)
                .unwrap_or_else(|| t.in_progress.into()),
        )?;
        ws.write_number_with_format(row, 3, s.worked_ms as f64 / MS_PER_DAY, &duration)?;
        ws.write_number_with_format(row, 4, s.paused_ms as f64 / MS_PER_DAY, &duration)?;
        ws.write_number(row, 5, s.auto_pauses as f64)?;
        ws.write_string(row, 6, project_name(&s.project_name, t))?;
        if let Some(note) = &s.note {
            ws.write_string(row, 7, note)?;
        }
        ws.write_string(row, 8, modification(s, t))?;
    }

    // Projekty
    let percent = Format::new().set_num_format("0%");
    let ws = wb.add_worksheet().set_name(t.projects_sheet)?;
    ws.set_column_width(0, 32)?;
    for (col, title) in [
        t.project,
        t.app_headers[1],
        t.app_headers[2],
        t.day_headers[5],
    ]
    .iter()
    .enumerate()
    {
        ws.write_string_with_format(0, col as u16, *title, &header)?;
        if col > 0 {
            ws.set_column_width(col as u16, 14)?;
        }
    }
    for (i, p) in report.projects.iter().enumerate() {
        let row = i as u32 + 1;
        ws.write_string(row, 0, project_name(&p.name, t))?;
        ws.write_number_with_format(row, 1, p.worked_ms as f64 / MS_PER_DAY, &duration)?;
        if report.worked_ms > 0 {
            ws.write_number_with_format(
                row,
                2,
                p.worked_ms as f64 / report.worked_ms as f64,
                &percent,
            )?;
        }
        ws.write_number(row, 3, p.sessions as f64)?;
    }

    // Aplikace
    let ws = wb.add_worksheet().set_name(t.sheets[3])?;
    ws.set_column_width(0, 32)?;
    ws.set_column_width(1, 14)?;
    ws.set_column_width(2, 14)?;
    for (col, title) in t.app_headers.iter().enumerate() {
        ws.write_string_with_format(0, col as u16, *title, &header)?;
    }
    let total: i64 = report.apps.iter().map(|a| a.ms).sum();
    for (i, a) in report.apps.iter().enumerate() {
        let row = i as u32 + 1;
        ws.write_string(row, 0, &a.name)?;
        ws.write_number_with_format(row, 1, a.ms as f64 / MS_PER_DAY, &duration)?;
        if total > 0 {
            ws.write_number_with_format(row, 2, a.ms as f64 / total as f64, &percent)?;
        }
    }

    wb.save(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_formatting() {
        assert_eq!(hmm(90 * 60_000), "1:30");
        assert_eq!(hours(90 * 60_000, true), "1,50");
        assert_eq!(hours(90 * 60_000, false), "1.50");
        assert_eq!(csv_field("a;b"), "\"a;b\"");
    }
}
